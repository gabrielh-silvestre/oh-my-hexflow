mod support;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use support::child::{Output as Child, omhl, omhl_json};
use support::ext::ExtTree;
use support::git::git_in;
use support::hook::assert_released_with_code;
use support::omhl::decision_data;
use support::project::Project;
use tempfile::TempDir;

const NATIVE_CHECKS: [&str; 3] = [
    "checks.run-tests",
    "checks.ci-green-before-merge",
    "checks.verify-review-comment-anchoring",
];
const NATIVE_GUIDELINES: [&str; 2] = [
    "guidelines.one-decision-per-turn",
    "guidelines.commit-policy",
];
const CONTEXT_CEILING: usize = 10_000;
const LINT_TOML: &str =
    "blocking = true\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";
const INVALID_TOML: &str = "this is = [not valid";
const PENDING_NOTE: &str = "NOTE: 1 extension change(s) not loaded into tenant acme; run omhl load";
const ORPHAN_NOTE: &str = "NOTE: 1 check(s) have no file in this working tree: checks.lint; if this branch or worktree predates the check, rebase it onto main (or restore the file); omhl check retire <id> is permanent for every clone and branch (the id never comes back), so run it only after the file removal is merged";

// Apoio

fn session_start(cwd: &Path) -> Value {
    json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": cwd,
        "source": "startup",
    })
}

/// Roda `omhl hook session-start` com o stdin dado, de uma pasta que não é a do projeto,
/// para que o `cwd` só possa vir do stdin.
fn run_hook(xdg: &Path, stdin: &str) -> Child {
    let elsewhere = TempDir::new().expect("other directory is created");

    omhl(&["hook", "session-start"])
        .cwd(elsewhere.path())
        .xdg(xdg)
        .stdin(stdin)
        .run()
}

fn hook_for(project: &Project) -> Child {
    run_hook(project.xdg(), &session_start(project.dir()).to_string())
}

fn in_project(project: &Project, args: &[&str]) -> Child {
    omhl_json(args).cwd(project.dir()).xdg(project.xdg()).run()
}

fn append_decisions(project: &Project, decisions: &[(String, String)]) {
    let items: Vec<Value> = decisions
        .iter()
        .map(|(id, text)| {
            json!({
                "body": {
                    "kind": "node",
                    "id": id,
                    "type": "decision",
                    "data": decision_data(text, None),
                }
            })
        })
        .collect();

    let appended = omhl(&["append"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(json!({ "items": items }).to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

fn append_use_wal(project: &Project) {
    append_decisions(
        project,
        &[("decisions.use-wal".to_owned(), "Use WAL".to_owned())],
    );
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// O `additionalContext` da saída do hook; falha se a saída não for o objeto esperado.
fn context_of(output: &Child) -> String {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

    json_line(&output.stdout)["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext is a string")
        .to_owned()
}

fn data_files(xdg: &Path) -> usize {
    fs::read_dir(xdg.join("omhl")).map_or(0, Iterator::count)
}

// Sem marcador

#[test]
fn it_should_print_nothing_and_create_nothing_without_a_marker() {
    let bare = TempDir::new().expect("bare directory is created");
    let xdg = TempDir::new().expect("data directory is created");

    let output = run_hook(xdg.path(), &session_start(bare.path()).to_string());

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert!(output.stderr.is_empty(), "stderr stays empty");
    assert_eq!(data_files(xdg.path()), 0, "no database file is created");
}

// Saída

#[test]
fn it_should_print_one_json_object_with_the_session_start_context() {
    let project = Project::new("acme");

    let output = hook_for(&project);

    let value = json_line(&output.stdout);
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stdout.matches('\n').count(), 1, "one line");
    assert!(output.stdout.ends_with('\n'), "the line ends with LF");
    assert_eq!(
        value["hookSpecificOutput"]["hookEventName"],
        json!("SessionStart")
    );
    assert!(value["hookSpecificOutput"]["additionalContext"].is_string());
    assert_eq!(
        value.as_object().expect("the root is an object").len(),
        1,
        "hookSpecificOutput is the only root key"
    );
}

#[test]
fn it_should_inject_the_native_base_and_the_project_nodes() {
    let project = Project::new("acme");
    append_use_wal(&project);

    let context = context_of(&hook_for(&project));

    for id in NATIVE_CHECKS.iter().chain(&NATIVE_GUIDELINES) {
        assert!(context.contains(&format!("### {id}\n")), "missing {id}");
    }
    assert!(context.contains("### decisions.use-wal\n"));
}

#[test]
fn it_should_keep_the_context_within_10000_characters() {
    let project = Project::new("acme");
    let text = "x".repeat(1_900);
    let decisions: Vec<(String, String)> = (0..200)
        .map(|n| (format!("decisions.d{n:03}"), text.clone()))
        .collect();
    for batch in decisions.chunks(50) {
        append_decisions(&project, batch);
    }

    let context = context_of(&hook_for(&project));

    let omitted = json_line(&in_project(&project, &["render"]).stdout)["omitted"]
        .as_u64()
        .expect("omitted is a number");
    let footer = format!("[{omitted} more nodes omitted for size; run `omhl list` to read them]");
    assert!(omitted > 0, "the budget forces an omission");
    assert!(context.chars().count() <= CONTEXT_CEILING);
    assert!(context.ends_with('\n'), "the context ends with LF");
    assert_eq!(context.lines().last(), Some(footer.as_str()));
}

// Descoberta do marcador e tolerância da entrada

#[test]
fn it_should_find_the_marker_from_a_subdirectory_cwd() {
    let project = Project::new("acme");
    let nested = project.dir().join("a").join("b").join("c");
    fs::create_dir_all(&nested).expect("nested directory is created");

    let output = run_hook(project.xdg(), &session_start(&nested).to_string());

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(json_line(&output.stdout)["hookSpecificOutput"].is_object());
}

#[test]
fn it_should_tolerate_extra_stdin_fields_and_an_unknown_source() {
    let project = Project::new("acme");
    let stdin = json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": project.dir(),
        "source": "future",
        "model": "x",
        "extra": { "a": 1 },
    });

    let output = run_hook(project.xdg(), &stdin.to_string());

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(json_line(&output.stdout)["hookSpecificOutput"].is_object());
}

// Erros

#[test]
fn it_should_exit_one_with_no_stdout_on_invalid_stdin() {
    let project = Project::new("acme");

    let output = run_hook(project.xdg(), "{");

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(output.stderr.matches('\n').count(), 1, "one error line");
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("input-invalid-json")
    );
}

#[test]
fn it_should_refuse_a_relative_cwd() {
    let project = Project::new("acme");
    let stdin = json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": "relative/project",
        "source": "startup",
    });

    let output = run_hook(project.xdg(), &stdin.to_string());

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(
        json_line(&output.stderr)["error"]["code"],
        json!("invalid-hook-input")
    );
}

#[test]
fn it_should_exit_one_with_no_stdout_on_an_invalid_marker() {
    let project = Project::new("acme");
    fs::write(
        project.dir().join(".omhl.toml"),
        "version = 1\ntenant = \"acme\"\nextra = 1\n",
    )
    .expect("marker is overwritten");

    let output = hook_for(&project);

    let error = json_line(&output.stderr)["error"].clone();
    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error["code"], json!("invalid-marker"));
    assert!(
        error["message"]
            .as_str()
            .expect("message is a string")
            .contains(".omhl.toml")
    );
}

// Idempotência e igualdade com o render

#[test]
fn it_should_print_identical_output_and_write_nothing_on_a_second_run() {
    let project = Project::new("acme");
    let total = |project: &Project| {
        json_line(&in_project(project, &["verify"]).stdout)["total_records"].clone()
    };
    let before = total(&project);

    let first = hook_for(&project);
    let second = hook_for(&project);
    let after = total(&project);

    assert!(context_of(&first).contains("### checks.run-tests\n"));
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(before, after);
}

#[test]
fn it_should_inject_the_same_text_as_the_render_action() {
    let project = Project::new("acme");
    append_use_wal(&project);

    let context = context_of(&hook_for(&project));
    let rendered = in_project(&project, &["render"]);

    assert_eq!(rendered.code, Some(0), "stderr: {}", rendered.stderr);
    assert_eq!(json_line(&rendered.stdout)["text"], json!(context));
}

// Aviso de extensions

fn warning_line(context: &str) -> &str {
    context
        .lines()
        .find(|line| line.starts_with("WARNING:"))
        .expect("the context has a WARNING line")
}

#[test]
fn it_should_append_a_note_when_extension_changes_are_pending() {
    let project = Project::new("acme");
    let baseline = context_of(&hook_for(&project));
    ExtTree::at(project.dir()).check("lint", LINT_TOML);

    let context = context_of(&hook_for(&project));

    let appended = context
        .strip_prefix(baseline.as_str())
        .expect("the rendered text comes first");
    assert_eq!(appended.trim_end(), format!("\n{PENDING_NOTE}"));
}

#[test]
fn it_should_list_the_orphaned_checks_in_a_note() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    let loaded = in_project(&project, &["load"]);
    fs::remove_file(project.dir().join(".omhl/checks/lint.toml")).expect("the file is deleted");

    let context = context_of(&hook_for(&project));

    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    assert!(context.contains(ORPHAN_NOTE), "context: {context}");
    assert!(!context.contains(PENDING_NOTE), "context: {context}");
}

#[test]
fn it_should_not_append_anything_when_extensions_are_synced() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);
    let loaded = in_project(&project, &["load"]);

    let context = context_of(&hook_for(&project));
    let rendered = in_project(&project, &["render"]);

    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    assert!(context.contains("### checks.lint\n"), "the check is loaded");
    assert_eq!(json_line(&rendered.stdout)["text"], json!(context));
    assert!(!context.contains("NOTE:"), "no note");
    assert!(!context.contains("WARNING:"), "no warning");
}

#[test]
fn it_should_warn_about_an_invalid_workflow_file_instead_of_swallowing_it() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).file(
        ".omhl/workflow.toml",
        "version = 1\n\n[specs]\ndirr = \"x\"\n",
    );

    let context = context_of(&hook_for(&project)).to_lowercase();

    assert!(
        context.contains("workflow") && context.contains("invalid"),
        "context: {context}"
    );
}

#[test]
fn it_should_not_write_to_the_database_when_extensions_are_pending() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("lint", LINT_TOML);

    let hooked = hook_for(&project);
    let listed = in_project(&project, &["list", "--type", "check"]);

    let ids: Vec<Value> = json_line(&listed.stdout)["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|item| item["id"].clone())
        .collect();
    assert_eq!(hooked.code, Some(0), "stderr: {}", hooked.stderr);
    assert_eq!(listed.code, Some(0), "stderr: {}", listed.stderr);
    assert!(
        !ids.contains(&json!("checks.lint")),
        "the check stays unloaded"
    );
}

#[test]
fn it_should_warn_with_codes_and_counts_but_never_paths() {
    let project = Project::new("acme");
    ExtTree::at(project.dir())
        .check("alpha-check", INVALID_TOML)
        .check("beta-check", INVALID_TOML)
        .symlink(".omhl/checks/gamma-link.toml", "alpha-check.toml");

    let context = context_of(&hook_for(&project));

    let line = warning_line(&context);
    assert!(line.contains("invalid-toml x2"), "counts the invalid files");
    assert!(line.contains("symlink x1"), "counts the symlink");
    for leaked in [".omhl/", "alpha-check", "beta-check", "gamma-link"] {
        assert!(!line.contains(leaked), "the warning leaks {leaked}");
    }
}

#[test]
fn it_should_keep_the_rendered_text_when_the_extension_check_fails() {
    let project = Project::new("acme");
    let baseline = context_of(&hook_for(&project));
    ExtTree::at(project.dir()).check("lint", INVALID_TOML);

    let context = context_of(&hook_for(&project));

    let appended = context
        .strip_prefix(baseline.as_str())
        .expect("the rendered text comes first");
    assert!(
        appended.starts_with("\nWARNING:"),
        "the warning follows a blank line"
    );
}

#[test]
fn it_should_keep_the_warning_line_within_480_characters() {
    let project = Project::new("acme");
    ExtTree::at(project.dir())
        .file(".omhl/checks/UPPER.toml", LINT_TOML)
        .check("broken", INVALID_TOML)
        .symlink(".omhl/checks/linked.toml", "broken.toml")
        .check("huge", &"#".repeat(65_537))
        .file(".omhl/checks/binary.toml", [0xff, 0xfe, 0xfd])
        .check(
            "nohow",
            "blocking = true\n\n[when]\nevent = \"before-commit\"\n",
        );

    let context = context_of(&hook_for(&project));

    assert!(warning_line(&context).chars().count() <= 480);
}

// spec-lock: o que independe de repositório git

fn pre_tool_use(cwd: &Path, tool_input: &Value) -> String {
    json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": cwd,
        "tool_name": "Write",
        "tool_input": tool_input,
    })
    .to_string()
}

/// Roda `omhl hook spec-lock` de uma pasta que não é a do projeto, para que o `cwd` só possa vir do stdin.
fn run_spec_lock(xdg: &Path, stdin: &str) -> Child {
    let elsewhere = TempDir::new().expect("other directory is created");

    omhl(&["hook", "spec-lock"])
        .cwd(elsewhere.path())
        .xdg(xdg)
        .stdin(stdin)
        .run()
}

#[test]
fn it_should_print_nothing_and_create_nothing_for_spec_lock_without_a_marker() {
    let bare = TempDir::new().expect("bare directory is created");
    let xdg = TempDir::new().expect("data directory is created");
    let spec = bare.path().join(".omhl/specs/wi-w1__login/parte-01.md");

    let output = run_spec_lock(
        xdg.path(),
        &pre_tool_use(bare.path(), &json!({ "file_path": spec, "content": "x" })),
    );

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert!(output.stderr.is_empty(), "stderr stays empty");
    assert_eq!(data_files(xdg.path()), 0, "no database file is created");
}

#[test]
fn it_should_release_spec_lock_with_an_error_line_on_invalid_stdin() {
    let project = Project::new("acme");

    let output = run_spec_lock(project.xdg(), "{");

    assert_released_with_code(&output, "input-invalid-json");
}

#[test]
fn it_should_release_spec_lock_with_an_error_line_on_a_relative_cwd() {
    let project = Project::new("acme");
    let stdin = pre_tool_use(
        Path::new("relative/project"),
        &json!({ "file_path": "/x/.omhl/specs/wi-w1__login/parte-01.md", "content": "x" }),
    );

    let output = run_spec_lock(project.xdg(), &stdin);

    assert_released_with_code(&output, "invalid-hook-input");
}

#[test]
fn it_should_release_spec_lock_with_an_error_line_when_file_path_is_missing() {
    let project = Project::new("acme");
    let stdin = pre_tool_use(project.dir(), &json!({ "content": "x" }));

    let output = run_spec_lock(project.xdg(), &stdin);

    assert_released_with_code(&output, "invalid-hook-input");
}

#[test]
fn it_should_release_spec_lock_with_an_error_line_when_file_path_is_relative() {
    let project = Project::new("acme");
    let stdin = pre_tool_use(
        project.dir(),
        &json!({ "file_path": ".omhl/specs/wi-w1__login/parte-01.md", "content": "x" }),
    );

    let output = run_spec_lock(project.xdg(), &stdin);

    assert_released_with_code(&output, "invalid-hook-input");
}

#[test]
fn it_should_stay_silent_for_spec_lock_on_a_tool_without_a_file_path() {
    let project = Project::new("acme");
    let stdin = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": project.dir(),
        "tool_name": "Bash",
        "tool_input": { "command": "ls" },
    });

    let output = run_spec_lock(project.xdg(), &stdin.to_string());

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert!(output.stderr.is_empty(), "stderr stays empty");
}

#[test]
fn it_should_tolerate_extra_stdin_fields_in_spec_lock() {
    let project = Project::new("acme");
    let stdin = json!({
        "session_id": "s1",
        "transcript_path": "/t.jsonl",
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "cwd": project.dir(),
        "tool_name": "Write",
        "tool_use_id": "u1",
        "tool_input": {
            "file_path": project.dir().join("src/main.rs"),
            "content": "x",
            "extra": { "a": 1 },
        },
        "future": true,
    });

    let output = run_spec_lock(project.xdg(), &stdin.to_string());

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert!(output.stderr.is_empty(), "stderr stays empty");
}

// Sem comando de descongelar

const UNFREEZE_WORDS: [&str; 4] = ["unfreeze", "unlock", "thaw", "defrost"];

fn help_of(args: &[&str]) -> String {
    let output = omhl(args).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

    output.stdout.to_lowercase()
}

#[test]
fn it_should_list_session_start_and_spec_lock_as_the_hook_events() {
    let help = help_of(&["hook", "--help"]);

    assert!(help.contains("session-start"), "help: {help}");
    assert!(help.contains("spec-lock"), "help: {help}");
}

#[test]
fn it_should_offer_no_way_to_unfreeze_a_spec_in_any_help_page() {
    let pages = [
        help_of(&["--help"]),
        help_of(&["hook", "--help"]),
        help_of(&["spec", "--help"]),
    ];

    for word in UNFREEZE_WORDS {
        for page in &pages {
            assert!(!page.contains(word), "a help page mentions {word}");
        }
    }
}

#[test]
fn it_should_refuse_an_unfreeze_subcommand_with_a_usage_error() {
    let codes: Vec<Option<i32>> = [
        &["hook", "unfreeze"][..],
        &["spec", "unfreeze", "work-items.w1"],
        &["unfreeze", "work-items.w1"],
    ]
    .iter()
    .map(|args| omhl(args).run().code)
    .collect();

    assert_eq!(codes, vec![Some(2); 3]);
}

#[test]
fn it_should_refuse_any_argument_to_spec_lock() {
    let output = omhl(&["hook", "spec-lock", "--unfreeze"]).run();

    assert_eq!(output.code, Some(2));
}

// Contexto com premissas e diretivas marcadas; premissa expira com o work item

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";

fn confirmation() -> Value {
    json!({ "by": "person:ana", "question": "Is it right?", "answer": "Yes." })
}

fn node_item(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

fn append_nodes(project: &Project, items: Vec<Value>) {
    let mut request = json!({});
    request["items"] = Value::Array(items);
    let appended = omhl(&["append"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(request.to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

/// Repositório git com o marcador commitado, para o `close` achar a árvore limpa.
fn git_project() -> Project {
    let project = Project::new("acme");
    for args in [
        &["init", "-b", "main"][..],
        &["add", "-A"],
        &["commit", "-m", "add marker"],
    ] {
        let output = git_in(project.dir(), args);
        assert!(output.status.success(), "git {args:?} failed");
    }

    project
}

fn append_open_work_item(project: &Project, id: &str) {
    let data = json!({ "title": "Login", "status": "open" });

    append_nodes(project, vec![node_item(id, "work-item", data)]);
}

fn close_work_item(project: &Project, id: &str) {
    let closed = in_project(project, &["close", id]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
}

fn premise_data(kind: &str, status: &str, work_item: &str) -> Value {
    let mut data = json!({
        "text": "the rule", "kind": kind, "status": status, "work_item": work_item,
    });
    if status == "confirmed" {
        data["confirmed_by"] = confirmation();
    }

    data
}

/// O contexto de uma sessão com o work item `W1` aberto e uma só premissa dele.
fn context_with_premise(data: Value) -> String {
    let project = git_project();
    append_open_work_item(&project, W1);
    append_nodes(&project, vec![node_item("premises.rule", "premise", data)]);

    context_of(&hook_for(&project))
}

/// As linhas do bloco `### <id>` do contexto, até a primeira linha em branco.
fn block_of<'c>(context: &'c str, id: &str) -> Vec<&'c str> {
    let header = format!("### {id}");

    context
        .lines()
        .skip_while(|line| *line != header)
        .take_while(|line| !line.is_empty())
        .collect()
}

/// O texto de `## <title>` até o próximo `## `; vazio se a seção não existe.
fn section_of<'c>(context: &'c str, title: &str) -> &'c str {
    let Some((_, from_title)) = context.split_once(&format!("\n## {title}\n")) else {
        return "";
    };

    from_title.split("\n## ").next().unwrap_or_default()
}

#[test]
fn it_should_show_an_unconfirmed_business_premise_as_pending_apart_from_the_binding_ones() {
    let context = context_with_premise(premise_data("business", "assumed", W1));

    assert!(block_of(&context, "premises.rule").contains(&"- mark: pending"));
    assert!(section_of(&context, "Pending premises").contains("### premises.rule\n"));
    assert!(!section_of(&context, "Premises").contains("premises.rule"));
}

#[test]
fn it_should_show_a_confirmed_business_premise_as_confirmed() {
    let context = context_with_premise(premise_data("business", "confirmed", W1));

    assert!(block_of(&context, "premises.rule").contains(&"- mark: confirmed"));
    assert!(section_of(&context, "Premises").contains("### premises.rule\n"));
}

#[test]
fn it_should_show_an_assumed_technical_premise_as_autonomous() {
    let context = context_with_premise(premise_data("technical", "assumed", W1));

    assert!(block_of(&context, "premises.rule").contains(&"- mark: autonomous"));
    assert!(section_of(&context, "Premises").contains("### premises.rule\n"));
}

#[test]
fn it_should_show_the_kind_of_a_premise() {
    let context = context_with_premise(premise_data("business", "assumed", W1));

    assert!(block_of(&context, "premises.rule").contains(&"- kind: business"));
}

#[test]
fn it_should_drop_the_premises_of_a_closed_work_item_from_the_context() {
    let project = git_project();
    append_open_work_item(&project, W1);
    append_nodes(
        &project,
        vec![node_item(
            "premises.rule",
            "premise",
            premise_data("technical", "assumed", W1),
        )],
    );
    close_work_item(&project, W1);

    let context = context_of(&hook_for(&project));

    assert!(!context.contains("premises.rule"), "context: {context}");
}

#[test]
fn it_should_keep_the_premises_of_an_open_work_item_when_another_one_is_closed() {
    let project = git_project();
    append_open_work_item(&project, W1);
    append_open_work_item(&project, W2);
    append_nodes(
        &project,
        vec![
            node_item(
                "premises.closed-rule",
                "premise",
                premise_data("technical", "assumed", W1),
            ),
            node_item(
                "premises.open-rule",
                "premise",
                premise_data("technical", "assumed", W2),
            ),
        ],
    );
    close_work_item(&project, W1);

    let context = context_of(&hook_for(&project));

    assert!(
        context.contains("### premises.open-rule\n"),
        "context: {context}"
    );
    assert!(
        !context.contains("premises.closed-rule"),
        "context: {context}"
    );
}

#[test]
fn it_should_keep_the_premises_of_a_closed_work_item_in_the_projection() {
    let project = git_project();
    append_open_work_item(&project, W1);
    append_nodes(
        &project,
        vec![node_item(
            "premises.rule",
            "premise",
            premise_data("technical", "assumed", W1),
        )],
    );
    close_work_item(&project, W1);

    let projected = in_project(&project, &["project"]);

    let file = fs::read_to_string(project.dir().join(".omhl/projection/work-items.w1.md"))
        .expect("the projection file exists");
    assert_eq!(projected.code, Some(0), "stderr: {}", projected.stderr);
    assert!(file.contains("### premises.rule\n"), "file: {file}");
}

#[test]
fn it_should_keep_a_premise_without_work_item_in_the_context_after_any_work_item_closes() {
    let project = git_project();
    append_open_work_item(&project, W1);
    let loose = json!({ "text": "the rule", "status": "assumed" });
    append_nodes(
        &project,
        vec![node_item("premises.loose", "premise", loose)],
    );
    close_work_item(&project, W1);

    let context = context_of(&hook_for(&project));

    assert!(
        context.contains("### premises.loose\n"),
        "context: {context}"
    );
    assert!(!block_of(&context, "premises.loose").contains(&"- work_item: work-items.w1"));
}

#[test]
fn it_should_mark_a_directive_with_confirmed_by_as_confirmed() {
    let project = Project::new("acme");
    let data = json!({ "text": "be brief", "kind": "business", "confirmed_by": confirmation() });
    append_nodes(
        &project,
        vec![node_item("guidelines.brief", "guideline", data)],
    );

    let context = context_of(&hook_for(&project));

    assert_eq!(
        block_of(&context, "guidelines.brief"),
        [
            "### guidelines.brief",
            "- confirmed: true",
            "- kind: business",
            "- text: be brief"
        ]
    );
}

#[test]
fn it_should_mark_a_loaded_extension_directive_as_confirmed() {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).guideline("tone", "kind = \"business\"\ntext = \"Be brief\"\n");
    let loaded = in_project(&project, &["load"]);

    let context = context_of(&hook_for(&project));

    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    assert!(block_of(&context, "guidelines.tone").contains(&"- confirmed: true"));
}

#[test]
fn it_should_keep_the_context_within_10000_characters_with_60_marked_directives() {
    let project = Project::new("acme");
    let text = "y".repeat(400);
    let directives: Vec<Value> = (0..60)
        .map(|n| {
            let data = json!({ "text": text, "kind": "business", "confirmed_by": confirmation() });
            node_item(&format!("guidelines.d{n:02}"), "guideline", data)
        })
        .collect();
    for batch in directives.chunks(30) {
        append_nodes(&project, batch.to_vec());
    }

    let context = context_of(&hook_for(&project));

    let shown = context
        .lines()
        .filter(|line| line.starts_with("### guidelines.d"))
        .count();
    let confirmed = context.matches("- confirmed: true\n").count();
    assert!(context.chars().count() <= CONTEXT_CEILING);
    assert!(shown > 0, "some directives fit");
    assert_eq!(confirmed, NATIVE_GUIDELINES.len() + shown);
}
