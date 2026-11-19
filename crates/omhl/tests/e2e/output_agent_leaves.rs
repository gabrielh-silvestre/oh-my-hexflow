use crate::support;
use omhl::adapters::text::{
    CommandKey, Layout, NEXT_LABEL, NOTHING_BLOCKED, Outcome, SECRET_WARNING, SYMBOL_ERROR,
    SYMBOL_OK, SYMBOL_WARNING, profile_for, render_failure, render_success,
};
use regex::Regex;
use serde_json::{Value, json};
use std::any::Any;
use std::fs;
use std::path::PathBuf;
use support::child::{Output, omhl as binary};
use support::cycle::{self, Program, State, check_file, check_repo, git};
use support::ext::ExtTree;
use support::inproc::{run_inproc, run_inproc_at};
use support::omhl::decision_data;
use support::output_agent_leaves::{
    fixture_problems, mask_durations, presence_problems, required_problems, table_problems,
};
use support::project::Project;
use support::query;
use support::report;
use support::run::Run;

// ---------------------------------------------------------------- fixtures e casos

struct Fixture {
    project: Project,
    project_dir: PathBuf,
    /// `Some(stdin)`: o comando roda pelo binário, com a pasta do projeto como cwd (o `check run` precisa de git e
    /// de processo próprio); `None`: roda em processo, com `--project-dir`.
    spawned_with: Option<String>,
    /// A cópia do binário que o comando grava (`hooks install`); `Some` roda sempre pelo binário.
    program: Option<Program>,
    /// Quem criou um fugido vivo segura o guarda até o `check sweep` rodar.
    sweep_guard: Option<Box<dyn Any>>,
}

impl Fixture {
    fn of(project: Project) -> Fixture {
        let project_dir = project.dir().to_path_buf();

        Fixture {
            project,
            project_dir,
            spawned_with: None,
            program: None,
            sweep_guard: None,
        }
    }

    fn spawned(project: Project, stdin: &str) -> Fixture {
        Fixture {
            spawned_with: Some(stdin.to_owned()),
            ..Fixture::of(project)
        }
    }

    fn from_state(state: State) -> Fixture {
        Fixture {
            spawned_with: Some(String::new()),
            program: state.program,
            ..Fixture::of(state.project)
        }
    }

    fn sweeping(project: Project, guard: cycle::SweepGuard) -> Fixture {
        Fixture {
            sweep_guard: Some(Box::new(guard)),
            ..Fixture::spawned(project, "")
        }
    }

    /// O binário com o ambiente do projeto; o `check run` nunca roda no processo do teste (varre os próprios filhos).
    fn spawn(&self, args: &[&str]) -> Output {
        let _sweeps = self.sweep_guard.is_none().then(cycle::check_run_guard);
        let command = binary(args)
            .cwd(&self.project_dir)
            .xdg(self.project.xdg())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(self.spawned_with.as_deref().unwrap_or_default());

        match &self.program {
            Some(program) => command.program(program.path()).run(),
            None => command.run(),
        }
    }

    fn run(&self, args: &[&str]) -> Run {
        if self.spawned_with.is_none() {
            return run_inproc_at(&self.project_dir, self.project.xdg(), args, "");
        }
        let output = self.spawn(args);

        Run {
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }

    /// O texto sem o que muda a cada fixture: as pastas temporárias, as durações e os números do `check sweep`.
    fn scrub(&self, text: &str) -> String {
        let mut places: Vec<(String, &str)> = vec![
            (self.project.xdg().display().to_string(), "<xdg>"),
            (self.project_dir.display().to_string(), "<repo>"),
        ];
        places.extend(
            self.program
                .iter()
                .map(|program| (program.dir().display().to_string(), "<bin>")),
        );
        let canonical: Vec<(String, &str)> = places
            .iter()
            .filter_map(|(path, label)| {
                fs::canonicalize(path)
                    .ok()
                    .map(|path| (path.display().to_string(), *label))
            })
            .collect();
        places.extend(canonical);
        places.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
        let placed = places.iter().fold(text.to_owned(), |text, (path, label)| {
            text.replace(path, label)
        });

        mask_volatile(&placed)
    }
}

fn append_decision(project: &Project, id: &str, text: &str) {
    let request = json!({
        "items": [{ "body": {
            "kind": "node", "id": id, "type": "decision",
            "data": decision_data(text, None),
        } }]
    });

    let appended = run_inproc(
        project,
        &["--format", "json", "append"],
        &request.to_string(),
    );

    assert_eq!(appended.stderr, "", "the fixture append failed");
}

/// Três decisões, uma cadeia fechada.
fn chain_of_three() -> Project {
    let project = Project::new("acme");
    append_decision(&project, "decisions.d1", "Chain marker one");
    append_decision(&project, "decisions.d2", "Chain marker two");
    append_decision(&project, "decisions.d3", "Chain marker three");

    project
}

/// Troca um caractere do texto do 2º record no arquivo do banco: o hash gravado deixa de bater.
fn tamper_second_record(project: &Project) {
    let path = project.db_path();
    let mut bytes = fs::read(&path).expect("the database is read");
    let (from, to) = (b"Chain marker two", b"Chain marker twp");
    let at = bytes
        .windows(from.len())
        .position(|window| window == from)
        .expect("the stored text is in the database file");
    bytes[at..at + to.len()].copy_from_slice(to);
    fs::write(&path, bytes).expect("the database is written");
}

fn verify_ok() -> Fixture {
    Fixture::of(chain_of_three())
}

fn verify_broken_chain() -> Fixture {
    let project = chain_of_three();
    tamper_second_record(&project);

    Fixture::of(project)
}

fn verify_no_marker() -> Fixture {
    let project = Project::new("acme");
    fs::remove_file(project.dir().join(".omhl.toml")).expect("the marker is removed");

    Fixture::of(project)
}

fn verify_invalid_project_dir() -> Fixture {
    let mut fixture = Fixture::of(Project::new("acme"));
    fixture.project_dir = fixture.project.dir().join("missing");

    fixture
}

fn verify_ok_facts(value: &Value) -> Vec<String> {
    vec![
        "chain ok".to_owned(),
        format!("{} records", value["total_records"]),
    ]
}

fn verify_broken_facts(value: &Value) -> Vec<String> {
    vec![
        "chain broken".to_owned(),
        format!("{} records", value["total_records"]),
        format!("{} breaks", value["total_breaks"]),
        format!("{NEXT_LABEL} "),
    ]
}

fn no_facts(_value: &Value) -> Vec<String> {
    Vec::new()
}

// ---------------------------------------------------------------- fixtures do `check run` (sempre pelo binário)

const CHECK_RUN: &[&str] = &["check", "run", "--event", "before-pr"];

const CHECK_RUN_ON_PUSH: &[&str] = &["check", "run", "--event", "before-pr", "--pre-push"];

/// Um push que só apaga uma ref: não há nada a atestar.
const DELETE_ONLY_PUSH: &str = "(delete) 0000000000000000000000000000000000000000 refs/heads/old bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n";

fn check_run_ok() -> Fixture {
    let toml = check_file("style", "format", "exit 0");

    Fixture::spawned(check_repo(&[("payroll-format", &toml)]), "")
}

/// O comando sai com 7 e escreve nas duas saídas: o check reprova e o `checks-failed` mostra o resumo.
fn check_run_blocked() -> Fixture {
    let toml = check_file(
        "test",
        "suite",
        "echo emitted-on-stdout; echo emitted-on-stderr >&2; exit 7",
    );

    Fixture::spawned(check_repo(&[("payroll-suite", &toml)]), "")
}

/// O arquivo commitado difere da definição guardada no banco: o check não roda.
fn check_run_stale() -> Fixture {
    let project = check_repo(&[("payroll-gate", &check_file("compile", "build", "exit 0"))]);
    ExtTree::at(project.dir()).check("payroll-gate", &check_file("compile", "build", "exit 3"));
    git(project.dir(), &["add", "-A"]);
    git(project.dir(), &["commit", "-q", "-m", "edit gate"]);

    Fixture::spawned(project, "")
}

fn check_run_nothing_to_attest() -> Fixture {
    let toml = check_file("style", "format", "exit 0");

    Fixture::spawned(check_repo(&[("payroll-format", &toml)]), DELETE_ONLY_PUSH)
}

/// HEAD solto contido em duas branches: não há branch do work item para resolver.
fn check_run_head_ambiguous() -> Fixture {
    let toml = check_file("style", "format", "exit 0");
    let project = check_repo(&[("payroll-format", &toml)]);
    git(project.dir(), &["branch", "payroll-copy"]);
    git(project.dir(), &["switch", "-q", "--detach"]);

    Fixture::spawned(project, "")
}

fn text_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("`{key}` is not a string in {value}"))
}

/// A duração do check reprovado como o texto a mostra: `Nms` abaixo de 1 s, `S.Ts` acima.
fn duration_text(duration_ms: u64) -> String {
    if duration_ms < 1000 {
        return format!("{duration_ms}ms");
    }

    format!("{:.1}s", duration_ms as f64 / 1000.0)
}

fn check_run_ok_facts(value: &Value) -> Vec<String> {
    let result = &value["results"][0];
    let id = text_at(&value["snapshot"], "id");

    vec![
        format!(
            "{SYMBOL_OK} {} · {} · recorded (git-commit {}",
            text_at(value, "event"),
            text_at(value, "work_item"),
            &id[..7]
        ),
        format!(
            "{SYMBOL_OK} {}  {}/{}  {}  {}ms",
            text_at(result, "check"),
            text_at(result, "concern"),
            text_at(result, "variant"),
            text_at(result, "outcome"),
            result["duration_ms"]
        ),
        format!("{NOTHING_BLOCKED} {NEXT_LABEL} go on with the push."),
    ]
}

fn check_run_ok_absent(_value: &Value) -> Vec<String> {
    ["non-blocking", "timed out", "truncated", "exit "]
        .map(str::to_owned)
        .to_vec()
}

fn check_run_blocked_facts(value: &Value) -> Vec<String> {
    let context = &value["error"]["details"][0]["context"];
    let summary = &value["error"]["context"]["summary"];
    assert_eq!(
        summary["failed"], summary["blocking_failed"],
        "the fixture has no non-blocking failure"
    );

    vec![
        format!(
            "{SYMBOL_ERROR} BLOCKED · {} · {} · {} blocking failed, {} passed, {} not run",
            text_at(&value["error"]["context"], "event"),
            text_at(&value["error"]["context"], "work_item"),
            summary["blocking_failed"],
            summary["passed"],
            summary["not_run"]
        ),
        format!(
            "{SYMBOL_ERROR} {} · {}/{} · blocking · timeout {}s · exit {} · {}",
            text_at(&value["error"]["details"][0], "path"),
            text_at(context, "concern"),
            text_at(context, "variant"),
            context["timeout_secs"],
            context["exit_code"],
            duration_text(context["duration_ms"].as_u64().expect("a duration"))
        ),
        "command:".to_owned(),
        context["run"]
            .as_array()
            .expect("the run is a list")
            .iter()
            .map(|part| part.as_str().expect("a run part is text"))
            .collect::<Vec<_>>()
            .join(" "),
        format!(
            "how to read the output: {}",
            text_at(&value["error"]["details"][0], "message")
        ),
        "The output may contain secrets".to_owned(),
        format!("{NEXT_LABEL} read the output with the command above"),
    ]
}

fn check_run_blocked_absent(value: &Value) -> Vec<String> {
    vec![
        text_at(&value["error"], "code").to_owned(),
        text_at(&value["error"], "message").to_owned(),
        "non-blocking".to_owned(),
        "timed out".to_owned(),
        "truncated".to_owned(),
        "kind:".to_owned(),
    ]
}

fn check_run_stale_facts(value: &Value) -> Vec<String> {
    let error = &value["error"];
    let detail = &error["details"][0];

    vec![
        format!(
            "{SYMBOL_ERROR} {} · {}",
            text_at(error, "code"),
            text_at(error, "message")
        ),
        format!(
            "- {} at {}",
            text_at(detail, "code"),
            text_at(detail, "path")
        ),
        text_at(detail, "message").to_owned(),
        format!("check: {}", text_at(&detail["context"], "check")),
        format!("hint: {}", text_at(&detail["context"], "hint")),
    ]
}

/// O cabeçalho `BLOCKED` é o único lugar do evento e do work item; `kind` igual ao `code` não ganha linha.
fn check_run_stale_absent(value: &Value) -> Vec<String> {
    let context = &value["error"]["details"][0]["context"];

    vec![
        "BLOCKED".to_owned(),
        text_at(context, "event").to_owned(),
        text_at(context, "work_item").to_owned(),
        "kind:".to_owned(),
    ]
}

fn check_run_nothing_facts(value: &Value) -> Vec<String> {
    vec![format!(
        "{SYMBOL_OK} {} · nothing to attest in this push; no check ran",
        text_at(value, "event")
    )]
}

fn check_run_nothing_absent(_value: &Value) -> Vec<String> {
    ["recorded", "Nothing blocked", "Next:"]
        .map(str::to_owned)
        .to_vec()
}

fn check_run_ambiguous_facts(value: &Value) -> Vec<String> {
    let error = &value["error"];

    vec![
        format!(
            "{SYMBOL_ERROR} {} · {}",
            text_at(error, "code"),
            text_at(error, "message")
        ),
        format!("- {}", text_at(&error["details"][0], "code")),
        format!("hint: {}", text_at(&error["details"][0]["context"], "hint")),
    ]
}

/// `path` vazio some o ` at <path>`; `event` fica fora do texto; `kind` igual ao `code` não ganha linha.
fn check_run_ambiguous_absent(value: &Value) -> Vec<String> {
    let detail = &value["error"]["details"][0];

    vec![
        format!("{} at", text_at(detail, "code")),
        text_at(&detail["context"], "event").to_owned(),
        "kind:".to_owned(),
    ]
}

// ---------------------------------------------------------------- fixtures do `list` e do `get` (em processo)

fn query_state() -> Fixture {
    Fixture::of(query::query_project())
}

fn items_of(value: &Value) -> &Vec<Value> {
    value["items"].as_array().expect("the list has items")
}

/// `N nodes · seq A–B` e as palavras dos fatos `Derived` do nó (`shadowed`, `blocking`).
fn list_facts(value: &Value) -> Vec<String> {
    let items = items_of(value);
    let first = &items[0]["seq"];
    let last = &items[items.len() - 1]["seq"];

    vec![
        format!("{} nodes · seq {first}–{last}", items.len()),
        "shadowed".to_owned(),
        "blocking".to_owned(),
    ]
}

fn list_page_facts(value: &Value) -> Vec<String> {
    let items = items_of(value);
    let last = &items[items.len() - 1]["seq"];

    vec![
        format!("{} nodes · seq 1–{last}", items.len()),
        format!(
            "{NEXT_LABEL} omhl list --after-seq {} with the same filters",
            value["next_after_seq"]
        ),
    ]
}

fn list_last_page_absent(_value: &Value) -> Vec<String> {
    vec!["Next:".to_owned()]
}

/// O cabeçalho do `get` e os fatos `Derived` do nó; `data` busca cada fato pelo ponteiro JSON em `node.data`.
fn get_facts(value: &Value, data: &[(&str, &str)]) -> Vec<String> {
    let node = &value["node"];
    let mut facts = vec![
        format!(
            "{SYMBOL_OK} get · {} · {} · seq {}",
            text_at(node, "id"),
            text_at(node, "type"),
            node["seq"]
        ),
        format!("shadowed: {}", node["shadowed"]),
    ];
    for (label, pointer) in data {
        let found = node["data"]
            .pointer(pointer)
            .unwrap_or_else(|| panic!("`{pointer}` is not in {node}"));
        facts.push(format!("{label}: {found}"));
    }

    facts
}

fn get_header_facts(value: &Value) -> Vec<String> {
    get_facts(value, &[])
}

fn get_attestation_facts(value: &Value) -> Vec<String> {
    get_facts(
        value,
        &[
            ("ok", "/ok"),
            ("exit_code", "/exit_code"),
            ("duration_ms", "/duration_ms"),
            ("timed_out", "/timed_out"),
            ("truncated", "/truncated"),
        ],
    )
}

fn get_closed_work_item_facts(value: &Value) -> Vec<String> {
    get_facts(value, &[("gate.passed", "/gate/passed")])
}

fn get_check_facts(value: &Value) -> Vec<String> {
    get_facts(
        value,
        &[
            ("blocking", "/blocking"),
            ("expect[0].exit", "/expect/0/exit"),
        ],
    )
}

fn get_blocking_facts(value: &Value) -> Vec<String> {
    get_facts(value, &[("blocking", "/blocking")])
}

fn get_missing_node_facts(_value: &Value) -> Vec<String> {
    vec![format!("{SYMBOL_WARNING} get · node not found")]
}

/// O erro genérico do app: o cabeçalho `código · mensagem` e a linha do detalhe, que só repete o código se difere.
fn generic_error_facts(value: &Value) -> Vec<String> {
    let error = &value["error"];
    let detail = &error["details"][0];
    let detail_line = match detail["code"] == error["code"] {
        true => format!("- {}", text_at(detail, "path")),
        false => format!(
            "- {} · {}",
            text_at(detail, "path"),
            text_at(detail, "code")
        ),
    };

    vec![
        format!(
            "{SYMBOL_ERROR} {} · {}",
            text_at(error, "code"),
            text_at(error, "message")
        ),
        detail_line,
    ]
}

// ---------------------------------------------------------------- fixtures de `render` e `report` (em processo)

fn report_bare() -> Fixture {
    Fixture::of(report::bare_project())
}

fn report_answers_state() -> Fixture {
    Fixture::of(report::answers_project())
}

fn report_recurrences_state() -> Fixture {
    Fixture::of(report::recurrences_project())
}

fn report_retro_state() -> Fixture {
    Fixture::of(report::retro_project())
}

/// O produto do `render` é o texto do contexto: a primeira linha nomeia o tenant, sem símbolo de cabeçalho.
fn render_facts(_value: &Value) -> Vec<String> {
    vec!["# omhl context for tenant acme".to_owned()]
}

fn render_absent(_value: &Value) -> Vec<String> {
    vec!["✔ render".to_owned()]
}

fn answers_facts(_value: &Value) -> Vec<String> {
    vec![
        "✔ report answers · 4 gaps".to_owned(),
        "criterion".to_owned(),
    ]
}

fn answers_empty_facts(_value: &Value) -> Vec<String> {
    vec!["✔ report answers · none".to_owned()]
}

/// Os ids de uma lista do grupo, separados por vírgula como o texto os mostra.
fn list_field(group: &Value, key: &str) -> String {
    let ids: Vec<&str> = group[key]
        .as_array()
        .expect("the group has the list")
        .iter()
        .filter_map(Value::as_str)
        .collect();

    ids.join(", ")
}

fn recurrences_facts(value: &Value) -> Vec<String> {
    let group = &value["recurrences"][0];

    vec![
        "✔ report recurrences · min 3 · 1 group".to_owned(),
        format!("work items: {}", list_field(group, "work_items")),
        format!("premises: {}", list_field(group, "premises")),
    ]
}

fn recurrences_empty_facts(_value: &Value) -> Vec<String> {
    vec!["✔ report recurrences · min 3 · none".to_owned()]
}

fn retro_facts(_value: &Value) -> Vec<String> {
    vec![
        "✔ report retro · 3 divergences · 1 approved amendment · 1 pending amendment · 1 no-criterion gap · 1 open gap"
            .to_owned(),
        "Next: propose one improvement per item to the owner and change nothing without the answer"
            .to_owned(),
    ]
}

fn retro_empty_facts(_value: &Value) -> Vec<String> {
    vec!["✔ report retro · none".to_owned()]
}

fn retro_empty_absent(_value: &Value) -> Vec<String> {
    vec!["Next:".to_owned()]
}

/// O erro sem detalhes (`no-marker`, `details: []`): só o cabeçalho `código · mensagem`.
fn error_header_facts(value: &Value) -> Vec<String> {
    let error = &value["error"];

    vec![format!(
        "{SYMBOL_ERROR} {} · {}",
        text_at(error, "code"),
        text_at(error, "message")
    )]
}

// ---------------------------------------------------------------- fixtures e fatos dos comandos simples do ciclo

fn project_first() -> Fixture {
    Fixture::of(cycle::project_first())
}

fn project_mixed() -> Fixture {
    Fixture::of(cycle::project_mixed())
}

fn project_blocked() -> Fixture {
    Fixture::of(cycle::project_blocked())
}

fn sweep_empty() -> Fixture {
    Fixture::spawned(Project::new("acme"), "")
}

fn sweep_with_stray() -> Fixture {
    let (project, guard) = cycle::sweep_stray();

    Fixture::sweeping(project, guard)
}

fn retire_check_orphaned() -> Fixture {
    Fixture::of(cycle::retire_check())
}

fn retire_guidelines() -> Fixture {
    Fixture::of(cycle::retire_guidelines())
}

fn without_stored_extensions() -> Fixture {
    Fixture::of(Project::new("acme"))
}

fn hooks_clean() -> Fixture {
    Fixture::from_state(cycle::hooks_clean())
}

fn hooks_in_build_dir() -> Fixture {
    Fixture::from_state(cycle::hooks_in_build_dir())
}

fn hooks_foreign() -> Fixture {
    Fixture::from_state(cycle::hooks_foreign())
}

fn spec_repo() -> Fixture {
    Fixture::of(cycle::spec_repo())
}

fn byproduct_state() -> Fixture {
    Fixture::of(cycle::byproduct_repo())
}

/// O tamanho de uma lista do `Value`, como o texto o conta.
fn size_of(value: &Value, key: &str) -> usize {
    value[key].as_array().map_or(0, Vec::len)
}

fn joined_names(value: &Value, key: &str) -> String {
    value[key]
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default()
        .join(", ")
}

/// `N written · N unchanged · N removed` e a linha de cada lista que tem nomes; os `unchanged` só entram na contagem.
fn project_facts(value: &Value) -> Vec<String> {
    let mut facts = vec![format!(
        "✔ project · {} · {} written · {} unchanged · {} removed",
        text_at(value, "dir"),
        size_of(value, "written"),
        size_of(value, "unchanged"),
        size_of(value, "removed"),
    )];
    for key in ["written", "removed"] {
        if size_of(value, key) > 0 {
            facts.push(format!("- {key} · {}", joined_names(value, key)));
        }
    }

    facts
}

/// Os nomes `unchanged` e as listas vazias não ganham linha.
fn project_absent(value: &Value) -> Vec<String> {
    let mut absent = vec!["- unchanged".to_owned()];
    absent.extend(
        value["unchanged"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    absent.extend(
        ["written", "removed"]
            .into_iter()
            .filter(|key| size_of(value, key) == 0)
            .map(|key| format!("- {key}")),
    );

    absent
}

fn sweep_facts(value: &Value) -> Vec<String> {
    let mut facts = vec![format!(
        "✔ check sweep · {} swept · {} unreadable",
        size_of(value, "swept"),
        value["unreadable"]
    )];
    facts.extend(
        value["swept"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|process| {
                format!(
                    "- {} · pid {} · owner {}",
                    text_at(process, "comm"),
                    process["pid"],
                    process["owner_pid"]
                )
            }),
    );

    facts
}

fn sweep_absent(_value: &Value) -> Vec<String> {
    ["scan timed out", "Next:"].map(str::to_owned).to_vec()
}

fn check_retire_facts(value: &Value) -> Vec<String> {
    vec![format!(
        "✔ check retire · retired {}",
        text_at(value, "retired")
    )]
}

fn guideline_retire_facts(value: &Value) -> Vec<String> {
    let retired = text_at(value, "retired");

    match value["superseded_by"].as_str() {
        Some(successor) => vec![format!(
            "✔ guideline retire · retired {retired} · superseded by {successor}"
        )],
        None => vec![format!("✔ guideline retire · retired {retired}")],
    }
}

fn guideline_retire_alone_absent(_value: &Value) -> Vec<String> {
    vec!["superseded".to_owned()]
}

/// O erro genérico e o `Next:` da `next_step` do detalhe, que o texto fecha com ponto.
fn error_with_next_step_facts(value: &Value) -> Vec<String> {
    let mut facts = generic_error_facts(value);
    facts.push(format!(
        "Next: {}",
        text_at(&value["error"]["details"][0], "next_step")
    ));

    facts
}

fn hooks_install_facts(value: &Value) -> Vec<String> {
    let warnings = size_of(value, "warnings");
    let scripts = counted(size_of(value, "scripts"), "script");
    let header = match warnings {
        0 => format!("✔ hooks install · {scripts}"),
        count => format!(
            "⚠ hooks install · {scripts} · {}",
            counted(count, "warning")
        ),
    };
    let mut facts = vec![
        header,
        format!("hooks path: {}", text_at(value, "hooks_path")),
        format!("omhl: {}", text_at(value, "omhl")),
        format!("scripts: {}", joined_names(value, "scripts")),
    ];
    for warning in value["warnings"].as_array().into_iter().flatten() {
        facts.push(format!(
            "- warning · {} · {}",
            text_at(warning, "code"),
            text_at(warning, "path")
        ));
        facts.push(format!("Next: {}", text_at(warning, "hint")));
    }

    facts
}

fn hooks_install_clean_absent(_value: &Value) -> Vec<String> {
    ["warning", "Next:"].map(str::to_owned).to_vec()
}

fn hooks_existing_facts(value: &Value) -> Vec<String> {
    let mut facts = error_header_facts(value);
    facts.push("- hooks-existing".to_owned());

    facts
}

fn spec_init_facts(value: &Value) -> Vec<String> {
    let work_item = text_at(value, "work_item");

    vec![
        format!("✔ spec init · {work_item} · {}", text_at(value, "slug")),
        format!("dir: {}", text_at(value, "dir")),
        format!("path: {}", text_at(value, "path")),
        format!(
            "Next: fill in the five sections of the part file, then run omhl spec validate {work_item}"
        ),
    ]
}

fn byproduct_facts(value: &Value) -> Vec<String> {
    let mut facts = vec![
        format!(
            "✔ byproduct · {} · {} · {} · {}",
            text_at(value, "check"),
            text_at(value, "work_item"),
            text_at(value, "outcome"),
            counted(size_of(value, "lines"), "line")
        ),
        SECRET_WARNING.to_owned(),
    ];
    if let Some(cut) = value["truncated_lines"].as_u64() {
        facts.push(format!(
            "⚠ {} cut short",
            counted(usize::try_from(cut).expect("a small count"), "line")
        ));
    }

    facts
}

fn byproduct_whole_absent(_value: &Value) -> Vec<String> {
    vec!["cut short".to_owned()]
}

/// `none` quando vazio; senão "N nouns", com o singular em 1.
fn counted(count: usize, noun: &str) -> String {
    match count {
        0 => "none".to_owned(),
        1 => format!("1 {noun}"),
        total => format!("{total} {noun}s"),
    }
}

// ---------------------------------------------------------------- fixtures e fatos de `spec validate` e `load`

fn spec_validate_one_part() -> Fixture {
    Fixture::of(cycle::spec_one_part())
}

fn spec_validate_with_amendments() -> Fixture {
    Fixture::of(cycle::spec_with_amendments())
}

fn spec_validate_broken() -> Fixture {
    Fixture::of(cycle::spec_broken())
}

fn load_full() -> Fixture {
    Fixture::of(cycle::load_full())
}

fn load_fresh() -> Fixture {
    Fixture::of(cycle::load_fresh())
}

fn load_invalid() -> Fixture {
    Fixture::of(cycle::load_invalid())
}

/// Os escalares de texto de uma lista do `Value`, na ordem em que vêm.
fn strings_of(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn objects_of<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value[key].as_array().into_iter().flatten()
}

/// `⚠` quando uma emenda traz violação, `✔` senão; uma linha por parte e por emenda, e a violação logo abaixo da emenda.
fn spec_validate_facts(value: &Value) -> Vec<String> {
    let violations: usize = objects_of(value, "amendments")
        .map(|amendment| size_of(amendment, "violations"))
        .sum();
    let symbol = match violations {
        0 => SYMBOL_OK,
        _ => SYMBOL_WARNING,
    };
    let mut facts = vec![format!(
        "{symbol} spec validate · {} · {}",
        plural(size_of(value, "parts"), "part"),
        plural(size_of(value, "amendments"), "amendment"),
    )];
    facts.extend(
        strings_of(value, "parts")
            .into_iter()
            .map(|name| format!("- part · {name}")),
    );
    for amendment in objects_of(value, "amendments") {
        facts.push(format!(
            "- amendment · {} · {}",
            text_at(amendment, "name"),
            text_at(amendment, "state")
        ));
        facts.extend(objects_of(amendment, "violations").map(|violation| {
            format!(
                "- {} · {}",
                text_at(violation, "path"),
                text_at(violation, "code")
            )
        }));
    }

    facts
}

fn spec_validate_absent(_value: &Value) -> Vec<String> {
    ["Next:", "ok ·"].map(str::to_owned).to_vec()
}

fn plural(count: usize, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        total => format!("{total} {noun}s"),
    }
}

/// O cabeçalho conta cada grupo (os de lista vazia que não são os três primeiros ficam de fora) e o `⚠` vem do aviso.
fn load_facts(value: &Value) -> Vec<String> {
    let mode = match value["dry_run"] == json!(true) {
        true => "dry run",
        false => "loaded",
    };
    let symbol = match size_of(value, "warnings") {
        0 => SYMBOL_OK,
        _ => SYMBOL_WARNING,
    };
    let mut header = format!(
        "{symbol} {mode} · {} created · {} revised · {} unchanged",
        size_of(value, "created"),
        size_of(value, "revised"),
        size_of(value, "unchanged"),
    );
    if size_of(value, "skipped") > 0 {
        header.push_str(&format!(" · {} skipped", size_of(value, "skipped")));
    }
    for (key, noun) in [
        ("relations_created", "relation"),
        ("orphans", "orphan"),
        ("orphan_relations", "orphan relation"),
        ("warnings", "warning"),
    ] {
        if size_of(value, key) > 0 {
            header.push_str(&format!(" · {}", plural(size_of(value, key), noun)));
        }
    }
    let mut facts = vec![header];
    for (key, label) in [
        ("created", "created"),
        ("revised", "revised"),
        ("orphans", "orphan"),
    ] {
        if size_of(value, key) > 0 {
            facts.push(format!("- {label} · {}", joined_names(value, key)));
        }
    }
    facts.extend(objects_of(value, "skipped").map(|entry| {
        format!(
            "- skipped · {} · {}",
            text_at(entry, "id"),
            text_at(entry, "code")
        )
    }));
    for (key, label) in [
        ("relations_created", "relation"),
        ("orphan_relations", "orphan relation"),
    ] {
        facts.extend(objects_of(value, key).map(|relation| {
            format!(
                "- {label} · {} after {}",
                text_at(relation, "from"),
                text_at(relation, "to")
            )
        }));
    }
    facts.extend(objects_of(value, "warnings").map(|warning| {
        format!(
            "- warning · {} · {} · {}",
            text_at(warning, "path"),
            text_at(warning, "code"),
            text_at(warning, "message")
        )
    }));

    facts
}

/// Os ids que não mudaram (a menos que outro grupo cite o id), o tenant e as listas vazias não ganham linha;
/// sem aviso o texto não diz `warning`.
fn load_absent(value: &Value) -> Vec<String> {
    let mut others = value.clone();
    others
        .as_object_mut()
        .expect("the load report is an object")
        .remove("unchanged");
    let cited = others.to_string();
    let mut absent: Vec<String> = strings_of(value, "unchanged")
        .into_iter()
        .filter(|id| !cited.contains(id.as_str()))
        .collect();
    absent.push("- unchanged".to_owned());
    absent.push(text_at(value, "tenant").to_owned());
    absent.extend(
        ["created", "revised", "orphans", "skipped"]
            .into_iter()
            .filter(|key| size_of(value, key) == 0)
            .map(|key| format!("- {key}")),
    );
    if size_of(value, "warnings") == 0 {
        absent.push("- warning".to_owned());
    }

    absent
}

// ---------------------------------------------------------------- fixtures e fatos de `append` e `attest`

/// O pedido vai no stdin: o `append` roda sempre pelo binário.
fn append_to_empty_db(request: &str) -> Fixture {
    Fixture::spawned(Project::new("acme"), request)
}

fn append_one() -> Fixture {
    append_to_empty_db(cycle::APPEND_ONE)
}

fn append_two() -> Fixture {
    append_to_empty_db(cycle::APPEND_TWO)
}

/// O mesmo pedido com `key` já gravado: o comando do caso o repete e recebe os mesmos records.
fn append_replayed() -> Fixture {
    let project = Project::new("acme");
    cycle::appended(&project, cycle::APPEND_KEYED);

    Fixture::spawned(project, cycle::APPEND_KEYED)
}

fn append_not_json() -> Fixture {
    append_to_empty_db(cycle::APPEND_NOT_JSON)
}

fn append_missing_title() -> Fixture {
    append_to_empty_db(cycle::APPEND_MISSING_TITLE)
}

fn attest_state() -> Fixture {
    Fixture::spawned(cycle::attest_repo(), "")
}

fn append_facts(value: &Value) -> Vec<String> {
    let records = value["records"].as_array().expect("records is a list");
    let mut header = format!("{SYMBOL_OK} append · {}", counted(records.len(), "record"));
    if value["replayed"] == true {
        header.push_str(" · replayed");
    }
    let lines = records
        .iter()
        .map(|record| format!("- {} · seq {}", text_at(record, "id"), record["seq"]));

    std::iter::once(header).chain(lines).collect()
}

/// A cabeça da cadeia é ruído declarado; `replayed` só aparece na palavra, quando é `true`.
fn append_fresh_absent(value: &Value) -> Vec<String> {
    vec![
        text_at(&value["chain_head"], "hash").to_owned(),
        "replayed".to_owned(),
    ]
}

fn append_replayed_absent(value: &Value) -> Vec<String> {
    vec![text_at(&value["chain_head"], "hash").to_owned()]
}

/// `✔` só em `passed`; o registro dos outros desfechos também foi gravado, mas é um aviso (`⚠`).
fn attest_facts(value: &Value) -> Vec<String> {
    let attestation = &value["attestation"];
    let symbol = match attestation["outcome"] == "passed" {
        true => SYMBOL_OK,
        false => SYMBOL_WARNING,
    };
    let snapshot = text_at(&attestation["snapshot"], "id");
    let mut facts = vec![format!(
        "{symbol} attest · {} · {} · {} · snapshot {}",
        text_at(attestation, "check"),
        text_at(attestation, "work_item"),
        text_at(attestation, "outcome"),
        &snapshot[..7]
    )];
    if let Some(note) = attestation["note"].as_str() {
        facts.push(format!("note: {note}"));
    }

    facts
}

/// O id da attestation e a cabeça da cadeia só saem no JSON; o sucesso não sugere próximo passo.
fn attest_absent(value: &Value) -> Vec<String> {
    let mut absent = vec![
        text_at(&value["attestation"], "id").to_owned(),
        text_at(&value["chain_head"], "hash").to_owned(),
        NEXT_LABEL.to_owned(),
    ];
    if value["attestation"]["note"].is_null() {
        absent.push("note:".to_owned());
    }

    absent
}

// ---------------------------------------------------------------- fixtures e fatos de `gate-pr` e `close`

fn gate_state(project: fn() -> Project) -> Fixture {
    Fixture::spawned(project(), "")
}

fn gate_approved() -> Fixture {
    gate_state(cycle::gate_approved)
}

fn gate_unattested() -> Fixture {
    gate_state(cycle::gate_unattested)
}

fn gate_failed() -> Fixture {
    gate_state(cycle::gate_failed)
}

fn gate_dirty() -> Fixture {
    gate_state(cycle::gate_dirty)
}

fn close_with_retro() -> Fixture {
    gate_state(cycle::close_with_retro)
}

fn questions_of(report: &Value) -> &Vec<Value> {
    report["questions"].as_array().expect("questions is a list")
}

fn question_passed(question: &Value) -> bool {
    question["passed"] == true
}

fn failing_ids(question: &Value) -> Vec<&str> {
    question["failing"]
        .as_array()
        .expect("failing is a list")
        .iter()
        .filter_map(Value::as_str)
        .collect()
}

/// O `next_step` da primeira pergunta que reprova e traz um: o `gate-pr` o promove a `Next:`.
fn first_failed_step(report: &Value) -> Option<&str> {
    questions_of(report)
        .iter()
        .filter(|question| !question_passed(question))
        .find_map(|question| question["next_step"].as_str())
}

/// `<gate> · passed|not passed · N questions[ · F failed]`.
fn gate_summary_text(report: &Value) -> String {
    let questions = questions_of(report);
    let failed = questions
        .iter()
        .filter(|question| !question_passed(question))
        .count();
    let verdict = match report["passed"] == true {
        true => "passed",
        false => "not passed",
    };
    let mut summary = format!(
        "{} · {verdict} · {}",
        text_at(report, "gate"),
        counted(questions.len(), "question")
    );
    if failed > 0 {
        summary.push_str(&format!(" · {failed} failed"));
    }

    summary
}

/// Uma linha por pergunta, com o `kind` só quando difere do `label`; o `next_step` vira campo, menos o promovido a `Next:`.
fn question_facts(report: &Value, promoted: Option<&str>) -> Vec<String> {
    let mut facts = Vec::new();
    for question in questions_of(report) {
        let symbol = match question_passed(question) {
            true => SYMBOL_OK,
            false => SYMBOL_ERROR,
        };
        let label = text_at(question, "label");
        let mut parts = vec![label.to_owned()];
        if text_at(question, "kind") != label {
            parts.push(text_at(question, "kind").to_owned());
        }
        parts.push(format!("matched {}", question["matched"]));
        let failing = failing_ids(question);
        if !failing.is_empty() {
            parts.push(format!("failing {}", failing.join(", ")));
        }
        if question["omitted"].as_u64().is_some_and(|count| count > 0) {
            parts.push(format!("{} omitted", question["omitted"]));
        }
        facts.push(format!("{symbol} {}", parts.join(" · ")));
        if let Some(step) = question["next_step"]
            .as_str()
            .filter(|s| promoted != Some(s))
        {
            facts.push(format!("next_step: {step}"));
        }
    }

    facts
}

fn gate_pr_facts(value: &Value) -> Vec<String> {
    let symbol = match value["passed"] == true {
        true => SYMBOL_OK,
        false => SYMBOL_ERROR,
    };
    let promoted = first_failed_step(value);
    let mut facts = vec![format!("{symbol} gate-pr · {}", gate_summary_text(value))];
    facts.extend(question_facts(value, promoted));
    facts.extend(promoted.map(|step| format!("{NEXT_LABEL} {step}")));

    facts
}

/// O que o texto de um gate omite: a cabeça da cadeia e as partes que só existem quando há o que mostrar.
fn gate_absent_facts(report: &Value) -> Vec<String> {
    let questions = questions_of(report);
    let mut absent = vec![text_at(&report["chain_head"], "hash").to_owned()];
    if !questions.iter().any(|q| q["next_step"].is_string()) {
        absent.push("next_step".to_owned());
    }
    if !questions.iter().any(|q| !failing_ids(q).is_empty()) {
        absent.push("failing".to_owned());
    }
    if questions.iter().all(|q| q["omitted"] == 0) {
        absent.push("omitted".to_owned());
    }
    if report["passed"] != true {
        absent.push(" · passed".to_owned());
    }

    absent
}

fn gate_pr_absent(value: &Value) -> Vec<String> {
    let mut absent = gate_absent_facts(value);
    if first_failed_step(value).is_none() {
        absent.push(NEXT_LABEL.to_owned());
    }

    absent
}

/// O texto do resumo do retro: contagens por parte, na ordem do renderizador.
fn retro_summary_text(retro: &Value) -> String {
    let divergences: usize = retro["divergences"]
        .as_array()
        .expect("divergences is a list")
        .iter()
        .map(|group| group["ids"].as_array().expect("ids is a list").len())
        .sum();
    let parts: Vec<String> = [
        (divergences, "divergence"),
        (
            retro["amendments"]["approved"]
                .as_array()
                .map_or(0, Vec::len),
            "approved amendment",
        ),
        (
            retro["amendments"]["pending"]
                .as_array()
                .map_or(0, Vec::len),
            "pending amendment",
        ),
        (
            retro["no_criterion"].as_array().map_or(0, Vec::len),
            "no-criterion gap",
        ),
        (
            retro["open_gaps"].as_array().map_or(0, Vec::len),
            "open gap",
        ),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, noun)| counted(count, noun))
    .collect();

    parts.join(" · ")
}

fn retro_facts_of_close(retro: &Value) -> Vec<String> {
    let mut facts = vec![format!("{SYMBOL_OK} retro · {}", retro_summary_text(retro))];
    for group in retro["divergences"]
        .as_array()
        .expect("divergences is a list")
    {
        let ids: Vec<&str> = group["ids"]
            .as_array()
            .expect("ids is a list")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        facts.push(format!(
            "- divergence · {} · {} · {}",
            text_at(group, "kind"),
            text_at(group, "resolution"),
            ids.join(", ")
        ));
    }
    for answer in retro["no_criterion"]
        .as_array()
        .expect("no_criterion is a list")
    {
        facts.push(format!(
            "- no criterion · {} · answered by {}",
            text_at(answer, "gap"),
            text_at(answer, "answered_by")
        ));
    }
    facts.push(format!(
        "{NEXT_LABEL} propose one improvement per item to the owner and change nothing without the answer"
    ));

    facts
}

fn close_facts(value: &Value) -> Vec<String> {
    let report = &value["report"];
    let symbol = match report["passed"] == true {
        true => SYMBOL_OK,
        false => SYMBOL_WARNING,
    };
    let records = value["outcome"]["records"]
        .as_array()
        .expect("records is a list");
    let mut facts = vec![format!("{symbol} close · {}", gate_summary_text(report))];
    facts.extend(question_facts(report, None));
    facts.push(format!(
        "{SYMBOL_OK} closed · {}",
        counted(records.len(), "record")
    ));
    facts.extend(
        records
            .iter()
            .map(|record| format!("- {} · seq {}", text_at(record, "id"), record["seq"])),
    );
    if !value["retro"].is_null() {
        facts.extend(retro_facts_of_close(&value["retro"]));
    }

    facts
}

/// O gate, o `replayed` do append e, sem retro, o bloco e o `Next:`; as duas cabeças da cadeia só saem no JSON.
fn close_absent(value: &Value) -> Vec<String> {
    let mut absent = gate_absent_facts(&value["report"]);
    absent.push(text_at(&value["outcome"]["chain_head"], "hash").to_owned());
    absent.push("replayed".to_owned());
    if value["retro"].is_null() {
        absent.push("retro".to_owned());
        absent.push(NEXT_LABEL.to_owned());
    }

    absent
}

/// Um estado de fixture de um comando.
struct Case {
    name: &'static str,
    key: CommandKey,
    args: &'static [&'static str],
    outcome: Outcome,
    /// O `error.code` literal esperado no `--format json` (só nos casos de erro).
    error_code: Option<&'static str>,
    fixture: fn() -> Fixture,
    /// Os fatos `Derived` que o texto mostra nos dois layouts, tirados do `Value`.
    derived: fn(&Value) -> Vec<String>,
    /// O que o texto omite nos dois layouts (o cabeçalho que substitui `code · message`, a linha `kind:`).
    absent: fn(&Value) -> Vec<String>,
}

const CASES: [Case; 81] = [
    Case {
        name: "verify ok",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: verify_ok,
        derived: verify_ok_facts,
        absent: no_facts,
    },
    Case {
        name: "verify broken chain",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: verify_broken_chain,
        derived: verify_broken_facts,
        absent: no_facts,
    },
    Case {
        name: "verify invalid project dir",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Failure,
        error_code: Some("invalid-project-dir"),
        fixture: verify_invalid_project_dir,
        derived: no_facts,
        absent: no_facts,
    },
    Case {
        name: "verify no marker",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Failure,
        error_code: Some("no-marker"),
        fixture: verify_no_marker,
        derived: no_facts,
        absent: no_facts,
    },
    Case {
        name: "check run ok",
        key: CommandKey::CheckRun,
        args: CHECK_RUN,
        outcome: Outcome::Success,
        error_code: None,
        fixture: check_run_ok,
        derived: check_run_ok_facts,
        absent: check_run_ok_absent,
    },
    Case {
        name: "check run checks-failed",
        key: CommandKey::CheckRun,
        args: CHECK_RUN,
        outcome: Outcome::Failure,
        error_code: Some("checks-failed"),
        fixture: check_run_blocked,
        derived: check_run_blocked_facts,
        absent: check_run_blocked_absent,
    },
    Case {
        name: "check run check-not-run",
        key: CommandKey::CheckRun,
        args: CHECK_RUN,
        outcome: Outcome::Failure,
        error_code: Some("check-not-run"),
        fixture: check_run_stale,
        derived: check_run_stale_facts,
        absent: check_run_stale_absent,
    },
    Case {
        name: "check run pre-push with nothing to attest",
        key: CommandKey::CheckRun,
        args: CHECK_RUN_ON_PUSH,
        outcome: Outcome::Success,
        error_code: None,
        fixture: check_run_nothing_to_attest,
        derived: check_run_nothing_facts,
        absent: check_run_nothing_absent,
    },
    Case {
        name: "check run head-ambiguous",
        key: CommandKey::CheckRun,
        args: CHECK_RUN,
        outcome: Outcome::Failure,
        error_code: Some("head-ambiguous"),
        fixture: check_run_head_ambiguous,
        derived: check_run_ambiguous_facts,
        absent: check_run_ambiguous_absent,
    },
    Case {
        name: "list with the nine types and a shadowed check",
        key: CommandKey::List,
        args: &["list"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: list_facts,
        absent: list_last_page_absent,
    },
    Case {
        name: "list page with a next page",
        key: CommandKey::List,
        args: &["list", "--limit", "3"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: list_page_facts,
        absent: no_facts,
    },
    Case {
        name: "get decision",
        key: CommandKey::Get,
        args: &["get", query::DECISION],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get attestation",
        key: CommandKey::Get,
        args: &["get", query::ATTESTATION],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_attestation_facts,
        absent: no_facts,
    },
    Case {
        name: "get premise",
        key: CommandKey::Get,
        args: &["get", query::PREMISE],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get closed work item with gate",
        key: CommandKey::Get,
        args: &["get", query::CLOSED_WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_closed_work_item_facts,
        absent: no_facts,
    },
    Case {
        name: "get project check",
        key: CommandKey::Get,
        args: &["get", query::CHECK],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_check_facts,
        absent: no_facts,
    },
    Case {
        name: "get shadowed check",
        key: CommandKey::Get,
        args: &["get", query::SHADOWED_CHECK],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_blocking_facts,
        absent: no_facts,
    },
    Case {
        name: "get divergence",
        key: CommandKey::Get,
        args: &["get", query::DIVERGENCE],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get pull request",
        key: CommandKey::Get,
        args: &["get", query::PULL_REQUEST],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get gap",
        key: CommandKey::Get,
        args: &["get", query::GAP],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get guideline",
        key: CommandKey::Get,
        args: &["get", query::GUIDELINE],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get missing node",
        key: CommandKey::Get,
        args: &["get", "checks.absent-probe"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_missing_node_facts,
        absent: no_facts,
    },
    Case {
        name: "get invalid id",
        key: CommandKey::Get,
        args: &["get", "Bad_Id"],
        outcome: Outcome::Failure,
        error_code: Some("INVALID_INPUT"),
        fixture: query_state,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "list limit zero",
        key: CommandKey::List,
        args: &["list", "--limit", "0"],
        outcome: Outcome::Failure,
        error_code: Some("invalid-arguments"),
        fixture: query_state,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "render native only",
        key: CommandKey::Render,
        args: &["render"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: report_bare,
        derived: render_facts,
        absent: render_absent,
    },
    Case {
        name: "render with nodes",
        key: CommandKey::Render,
        args: &["render"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: render_facts,
        absent: render_absent,
    },
    Case {
        name: "render no marker",
        key: CommandKey::Render,
        args: &["render"],
        outcome: Outcome::Failure,
        error_code: Some("no-marker"),
        fixture: verify_no_marker,
        derived: error_header_facts,
        absent: no_facts,
    },
    Case {
        name: "report answers empty",
        key: CommandKey::ReportAnswers,
        args: &["report", "answers"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: report_bare,
        derived: answers_empty_facts,
        absent: no_facts,
    },
    Case {
        name: "report answers by state",
        key: CommandKey::ReportAnswers,
        args: &["report", "answers"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: report_answers_state,
        derived: answers_facts,
        absent: no_facts,
    },
    Case {
        name: "report answers unknown work item",
        key: CommandKey::ReportAnswers,
        args: &["report", "answers", "--work-item", report::ABSENT_WORK_ITEM],
        outcome: Outcome::Failure,
        error_code: Some("work-item-not-found"),
        fixture: report_bare,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "report recurrences empty",
        key: CommandKey::ReportRecurrences,
        args: &["report", "recurrences"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: report_bare,
        derived: recurrences_empty_facts,
        absent: no_facts,
    },
    Case {
        name: "report recurrences one group",
        key: CommandKey::ReportRecurrences,
        args: &["report", "recurrences"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: report_recurrences_state,
        derived: recurrences_facts,
        absent: no_facts,
    },
    Case {
        name: "report recurrences no marker",
        key: CommandKey::ReportRecurrences,
        args: &["report", "recurrences"],
        outcome: Outcome::Failure,
        error_code: Some("no-marker"),
        fixture: verify_no_marker,
        derived: error_header_facts,
        absent: no_facts,
    },
    Case {
        name: "report retro null",
        key: CommandKey::ReportRetro,
        args: &["report", "retro", report::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: report_bare,
        derived: retro_empty_facts,
        absent: retro_empty_absent,
    },
    Case {
        name: "report retro full",
        key: CommandKey::ReportRetro,
        args: &["report", "retro", report::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: report_retro_state,
        derived: retro_facts,
        absent: no_facts,
    },
    Case {
        name: "report retro unknown work item",
        key: CommandKey::ReportRetro,
        args: &["report", "retro", report::ABSENT_WORK_ITEM],
        outcome: Outcome::Failure,
        error_code: Some("work-item-not-found"),
        fixture: report_bare,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "project writes the new files",
        key: CommandKey::Project,
        args: &["project"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: project_first,
        derived: project_facts,
        absent: project_absent,
    },
    Case {
        name: "project writes, keeps and removes files",
        key: CommandKey::Project,
        args: &["project"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: project_mixed,
        derived: project_facts,
        absent: project_absent,
    },
    Case {
        name: "project not-generated file",
        key: CommandKey::Project,
        args: &["project"],
        outcome: Outcome::Failure,
        error_code: Some("projection-failed"),
        fixture: project_blocked,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "check sweep with nothing to sweep",
        key: CommandKey::CheckSweep,
        args: &["check", "sweep"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: sweep_empty,
        derived: sweep_facts,
        absent: sweep_absent,
    },
    Case {
        name: "check sweep with a swept stray",
        key: CommandKey::CheckSweep,
        args: &["check", "sweep"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: sweep_with_stray,
        derived: sweep_facts,
        absent: sweep_absent,
    },
    Case {
        name: "check sweep no marker",
        key: CommandKey::CheckSweep,
        args: &["check", "sweep"],
        outcome: Outcome::Failure,
        error_code: Some("no-marker"),
        fixture: verify_no_marker,
        derived: error_header_facts,
        absent: no_facts,
    },
    Case {
        name: "check retire an orphaned check",
        key: CommandKey::CheckRetire,
        args: &["check", "retire", cycle::ORPHAN_CHECK],
        outcome: Outcome::Success,
        error_code: None,
        fixture: retire_check_orphaned,
        derived: check_retire_facts,
        absent: no_facts,
    },
    Case {
        name: "check retire check-not-found",
        key: CommandKey::CheckRetire,
        args: &["check", "retire", "checks.ghost-one"],
        outcome: Outcome::Failure,
        error_code: Some("check-not-found"),
        fixture: without_stored_extensions,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "guideline retire without a successor",
        key: CommandKey::GuidelineRetire,
        args: &["guideline", "retire", cycle::ORPHAN_GUIDELINE],
        outcome: Outcome::Success,
        error_code: None,
        fixture: retire_guidelines,
        derived: guideline_retire_facts,
        absent: guideline_retire_alone_absent,
    },
    Case {
        name: "guideline retire with a successor",
        key: CommandKey::GuidelineRetire,
        args: &[
            "guideline",
            "retire",
            cycle::ORPHAN_GUIDELINE,
            "--by",
            cycle::SUCCESSOR_GUIDELINE,
        ],
        outcome: Outcome::Success,
        error_code: None,
        fixture: retire_guidelines,
        derived: guideline_retire_facts,
        absent: no_facts,
    },
    Case {
        name: "guideline retire guideline-not-found",
        key: CommandKey::GuidelineRetire,
        args: &["guideline", "retire", "guidelines.ghost-one"],
        outcome: Outcome::Failure,
        error_code: Some("guideline-not-found"),
        fixture: without_stored_extensions,
        derived: error_with_next_step_facts,
        absent: no_facts,
    },
    Case {
        name: "hooks install without warnings",
        key: CommandKey::HooksInstall,
        args: &["hooks", "install"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: hooks_clean,
        derived: hooks_install_facts,
        absent: hooks_install_clean_absent,
    },
    Case {
        name: "hooks install with a build-dir warning",
        key: CommandKey::HooksInstall,
        args: &["hooks", "install"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: hooks_in_build_dir,
        derived: hooks_install_facts,
        absent: no_facts,
    },
    Case {
        name: "hooks install hooks-existing",
        key: CommandKey::HooksInstall,
        args: &["hooks", "install"],
        outcome: Outcome::Failure,
        error_code: Some("hooks-existing"),
        fixture: hooks_foreign,
        derived: hooks_existing_facts,
        absent: no_facts,
    },
    Case {
        name: "spec init creates the first part",
        key: CommandKey::SpecInit,
        args: &["spec", "init", cycle::WORK_ITEM, cycle::SPEC_SLUG],
        outcome: Outcome::Success,
        error_code: None,
        fixture: spec_repo,
        derived: spec_init_facts,
        absent: no_facts,
    },
    Case {
        name: "spec init spec-slug-invalid",
        key: CommandKey::SpecInit,
        args: &["spec", "init", cycle::WORK_ITEM, "Bad_Slug"],
        outcome: Outcome::Failure,
        error_code: Some("spec-slug-invalid"),
        fixture: spec_repo,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "byproduct shows the stored lines",
        key: CommandKey::Byproduct,
        args: &["byproduct", cycle::CHECK_WITH_LINES, cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: byproduct_state,
        derived: byproduct_facts,
        absent: byproduct_whole_absent,
    },
    Case {
        name: "byproduct cuts a line over the limit",
        key: CommandKey::Byproduct,
        args: &["byproduct", cycle::CHECK_WITH_WIDE_LINE, cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: byproduct_state,
        derived: byproduct_facts,
        absent: no_facts,
    },
    Case {
        name: "byproduct without output lines",
        key: CommandKey::Byproduct,
        args: &["byproduct", cycle::CHECK_WITHOUT_OUTPUT, cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: byproduct_state,
        derived: byproduct_facts,
        absent: byproduct_whole_absent,
    },
    Case {
        name: "byproduct check-not-found",
        key: CommandKey::Byproduct,
        args: &["byproduct", "checks.ghost-one", cycle::WORK_ITEM],
        outcome: Outcome::Failure,
        error_code: Some("check-not-found"),
        fixture: without_stored_extensions,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "append writes one record",
        key: CommandKey::Append,
        args: &["append"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: append_one,
        derived: append_facts,
        absent: append_fresh_absent,
    },
    Case {
        name: "append writes several records",
        key: CommandKey::Append,
        args: &["append"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: append_two,
        derived: append_facts,
        absent: append_fresh_absent,
    },
    Case {
        name: "append replays a request that already wrote",
        key: CommandKey::Append,
        args: &["append"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: append_replayed,
        derived: append_facts,
        absent: append_replayed_absent,
    },
    Case {
        name: "append input-invalid-json",
        key: CommandKey::Append,
        args: &["append"],
        outcome: Outcome::Failure,
        error_code: Some("input-invalid-json"),
        fixture: append_not_json,
        derived: error_header_facts,
        absent: no_facts,
    },
    Case {
        name: "append INVALID_RECORD",
        key: CommandKey::Append,
        args: &["append"],
        outcome: Outcome::Failure,
        error_code: Some("INVALID_RECORD"),
        fixture: append_missing_title,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "attest passed with a note",
        key: CommandKey::Attest,
        args: &[
            "attest",
            cycle::MANUAL_CHECK,
            cycle::WORK_ITEM,
            "--outcome",
            "passed",
            "--note",
            "Totals match the bank export",
        ],
        outcome: Outcome::Success,
        error_code: None,
        fixture: attest_state,
        derived: attest_facts,
        absent: attest_absent,
    },
    Case {
        name: "attest passed_with_warning",
        key: CommandKey::Attest,
        args: &[
            "attest",
            cycle::MANUAL_CHECK,
            cycle::WORK_ITEM,
            "--outcome",
            "passed_with_warning",
        ],
        outcome: Outcome::Success,
        error_code: None,
        fixture: attest_state,
        derived: attest_facts,
        absent: attest_absent,
    },
    Case {
        name: "attest failed",
        key: CommandKey::Attest,
        args: &[
            "attest",
            cycle::MANUAL_CHECK,
            cycle::WORK_ITEM,
            "--outcome",
            "failed",
        ],
        outcome: Outcome::Success,
        error_code: None,
        fixture: attest_state,
        derived: attest_facts,
        absent: attest_absent,
    },
    Case {
        name: "attest check-not-found",
        key: CommandKey::Attest,
        args: &[
            "attest",
            "checks.ghost-one",
            cycle::WORK_ITEM,
            "--outcome",
            "passed",
        ],
        outcome: Outcome::Failure,
        error_code: Some("check-not-found"),
        fixture: attest_state,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "gate-pr passed",
        key: CommandKey::GatePr,
        args: &["gate-pr", cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: gate_approved,
        derived: gate_pr_facts,
        absent: gate_pr_absent,
    },
    Case {
        name: "gate-pr failed without next step",
        key: CommandKey::GatePr,
        args: &["gate-pr", cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: gate_unattested,
        derived: gate_pr_facts,
        absent: gate_pr_absent,
    },
    Case {
        name: "gate-pr failed with two anchor questions",
        key: CommandKey::GatePr,
        args: &["gate-pr", cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: gate_failed,
        derived: gate_pr_facts,
        absent: gate_pr_absent,
    },
    Case {
        name: "gate-pr snapshot-dirty",
        key: CommandKey::GatePr,
        args: &["gate-pr", cycle::WORK_ITEM],
        outcome: Outcome::Failure,
        error_code: Some("snapshot-dirty"),
        fixture: gate_dirty,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "close with a passed gate and no retro",
        key: CommandKey::Close,
        args: &["close", cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: gate_approved,
        derived: close_facts,
        absent: close_absent,
    },
    Case {
        name: "close with a failed gate and no retro",
        key: CommandKey::Close,
        args: &["close", cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: gate_failed,
        derived: close_facts,
        absent: close_absent,
    },
    Case {
        name: "close with a passed gate and a retro",
        key: CommandKey::Close,
        args: &["close", cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: close_with_retro,
        derived: close_facts,
        absent: close_absent,
    },
    Case {
        name: "close snapshot-dirty",
        key: CommandKey::Close,
        args: &["close", cycle::WORK_ITEM],
        outcome: Outcome::Failure,
        error_code: Some("snapshot-dirty"),
        fixture: gate_dirty,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "spec validate with a single part",
        key: CommandKey::SpecValidate,
        args: &["spec", "validate", cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: spec_validate_one_part,
        derived: spec_validate_facts,
        absent: spec_validate_absent,
    },
    Case {
        name: "spec validate with an approved and a pending amendment",
        key: CommandKey::SpecValidate,
        args: &["spec", "validate", cycle::WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: spec_validate_with_amendments,
        derived: spec_validate_facts,
        absent: spec_validate_absent,
    },
    Case {
        name: "spec validate spec-invalid",
        key: CommandKey::SpecValidate,
        args: &["spec", "validate", cycle::WORK_ITEM],
        outcome: Outcome::Failure,
        error_code: Some("spec-invalid"),
        fixture: spec_validate_broken,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "spec validate spec-not-found",
        key: CommandKey::SpecValidate,
        args: &["spec", "validate", cycle::WORK_ITEM],
        outcome: Outcome::Failure,
        error_code: Some("spec-not-found"),
        fixture: spec_repo,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "load reports every group and a warning",
        key: CommandKey::Load,
        args: &["load"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: load_full,
        derived: load_facts,
        absent: load_absent,
    },
    Case {
        name: "load dry run",
        key: CommandKey::Load,
        args: &["load", "--dry-run"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: load_fresh,
        derived: load_facts,
        absent: load_absent,
    },
    Case {
        name: "load creates the new files without warnings",
        key: CommandKey::Load,
        args: &["load"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: load_fresh,
        derived: load_facts,
        absent: load_absent,
    },
    Case {
        name: "load extensions-invalid",
        key: CommandKey::Load,
        args: &["load"],
        outcome: Outcome::Failure,
        error_code: Some("extensions-invalid"),
        fixture: load_invalid,
        derived: error_with_next_step_facts,
        absent: no_facts,
    },
];

fn command(case: &Case, format: &str) -> Vec<String> {
    ["--format", format]
        .into_iter()
        .chain(case.args.iter().copied())
        .map(str::to_owned)
        .collect()
}

fn run_case(case: &Case, fixture: &Fixture, format: &str) -> Run {
    let args = command(case, format);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    fixture.run(&args)
}

/// O `Value` do comando em `--format json`: o stdout do sucesso ou o `{"error": ...}` do stderr.
fn json_of(case: &Case, fixture: &Fixture) -> Value {
    let run = run_case(case, fixture, "json");
    let (stream, other) = match case.outcome {
        Outcome::Success => (run.stdout, run.stderr),
        Outcome::Failure => (run.stderr, run.stdout),
    };

    assert_eq!(other, "", "{}: the other stream is not empty", case.name);
    serde_json::from_str(stream.trim_end())
        .unwrap_or_else(|error| panic!("{}: the output is not JSON ({error}): {stream}", case.name))
}

fn text_of(case: &Case, value: &Value, layout: Layout) -> String {
    match case.outcome {
        Outcome::Success => render_success(case.key, value, layout),
        Outcome::Failure => render_failure(case.key, value, layout),
    }
}

fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Agent => "agent",
        Layout::Human => "human",
    }
}

/// O `Value` de cada caso, na ordem de `CASES`: calculado uma vez e lido por todos os testes que não mudam a fixture,
/// porque cada cálculo roda o comando real num filho e a varredura repetida estoura o prazo do filho sob carga.
#[expect(
    clippy::disallowed_types,
    reason = "memory-exception: the tests of one binary run on separate threads and share the values; alternatives discarded: recomputing per test (about 12 sweeps overrun the 30 s child deadline under load), merging the read-only tests into one (breaks one behavior per test), passing the values by reference (the test harness has no shared fixture argument)"
)]
static CASE_VALUES: std::sync::LazyLock<Vec<Value>> = std::sync::LazyLock::new(|| {
    CASES
        .iter()
        .map(|case| json_of(case, &(case.fixture)()))
        .collect()
});

/// Roda a checagem em todos os casos e reprova uma vez, com os problemas de todos: um caso vermelho não esconde os outros.
fn assert_cases_clean(check: impl Fn(&Case, &Value) -> Vec<String>) {
    let problems: Vec<String> = CASES
        .iter()
        .zip(CASE_VALUES.iter())
        .flat_map(|(case, value)| {
            check(case, value)
                .into_iter()
                .map(|problem| format!("{}: {problem}", case.name))
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(problems.is_empty(), "{problems:#?}");
}

// ---------------------------------------------------------------- testes de cobertura das folhas (CA-2)

#[test]
fn it_should_declare_every_scalar_of_each_real_value_in_the_profile_table() {
    assert_cases_clean(|case, value| table_problems(profile_for(case.key), case.outcome, value));
}

#[test]
fn it_should_find_every_required_profile_key_in_each_real_value() {
    assert_cases_clean(|case, value| required_problems(profile_for(case.key), case.outcome, value));
}

#[test]
fn it_should_build_each_fixture_so_that_its_free_literals_are_distinguishable() {
    assert_cases_clean(|case, value| fixture_problems(profile_for(case.key), case.outcome, value));
}

fn assert_leaves_in_text(layout: Layout) {
    assert_cases_clean(|case, value| {
        let text = text_of(case, value, layout);

        presence_problems(profile_for(case.key), case.outcome, value, &text)
    });
}

#[test]
fn it_should_show_every_leaf_of_the_real_value_in_the_agent_text() {
    assert_leaves_in_text(Layout::Agent);
}

#[test]
fn it_should_show_every_leaf_of_the_real_value_in_the_human_text() {
    assert_leaves_in_text(Layout::Human);
}

/// O `human` alinha os rótulos com espaços extras: os fatos se comparam com cada sequência de espaços reduzida a um.
fn squeeze(text: &str) -> String {
    text.split(' ')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn assert_derived_facts_in_text(layout: Layout) {
    assert_cases_clean(|case, value| {
        let text = squeeze(&text_of(case, value, layout));

        (case.derived)(value)
            .into_iter()
            .filter(|fact| !text.contains(&squeeze(fact)))
            .map(|fact| format!("`{fact}` missing from:\n{text}"))
            .collect()
    });
}

#[test]
fn it_should_show_every_derived_fact_in_the_agent_text() {
    assert_derived_facts_in_text(Layout::Agent);
}

#[test]
fn it_should_show_every_derived_fact_in_the_human_text() {
    assert_derived_facts_in_text(Layout::Human);
}

fn assert_omitted_facts_absent_from_text(layout: Layout) {
    assert_cases_clean(|case, value| {
        let text = squeeze(&text_of(case, value, layout));

        (case.absent)(value)
            .into_iter()
            .filter(|fact| text.contains(&squeeze(fact)))
            .map(|fact| format!("`{fact}` must not be in:\n{text}"))
            .collect()
    });
}

#[test]
fn it_should_omit_the_facts_the_agent_text_replaces_or_hides() {
    assert_omitted_facts_absent_from_text(Layout::Agent);
}

#[test]
fn it_should_omit_the_facts_the_human_text_replaces_or_hides() {
    assert_omitted_facts_absent_from_text(Layout::Human);
}

#[test]
fn it_should_not_suggest_a_next_step_when_the_chain_is_ok() {
    let case = &CASES[0];

    let text = text_of(case, &CASE_VALUES[0], Layout::Agent);

    assert!(!text.contains("Next:"), "{text}");
}

/// Troca por marcadores o que muda a cada execução: durações, pids, as contagens do `check sweep` e os ids de record.
fn mask_volatile(text: &str) -> String {
    let pids = Regex::new(r"\b(pid|owner) \d+\b").expect("the pid pattern compiles");
    let counts = Regex::new(r"\b\d+ (swept|unreadable)\b").expect("the count pattern compiles");
    let record_ids = Regex::new(r"\b[a-z0-9-]+:[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}\b")
        .expect("the record id pattern compiles");
    let timed = mask_durations(text);
    let masked = pids.replace_all(&timed, "$1 <pid>");
    let counted = counts.replace_all(&masked, "<n> $1");

    record_ids.replace_all(&counted, "<record-id>").into_owned()
}

/// O `report` fecha o texto com uma quebra de linha; o `render` não: o `text` do contexto já termina nos seus bytes.
fn line_break(case: &Case) -> &'static str {
    match (case.key, case.outcome) {
        (CommandKey::Render, Outcome::Success) => "",
        _ => "\n",
    }
}

/// O comando muda a fixture (grava, aposenta, cria): rodá-lo de novo na mesma fixture não repete o `Value`.
fn changes_the_fixture(case: &Case) -> bool {
    matches!(
        case.key,
        CommandKey::Project
            | CommandKey::CheckSweep
            | CommandKey::CheckRetire
            | CommandKey::GuidelineRetire
            | CommandKey::SpecInit
            | CommandKey::Append
            | CommandKey::Close
            | CommandKey::Load
    ) && case.outcome == Outcome::Success
}

fn assert_command_prints_the_renderer_output(layout: Layout) {
    for case in &CASES {
        let fixture = (case.fixture)();
        let value = json_of(case, &fixture);
        // As pastas e os pids saem do texto, como no runner de exemplos: a 2ª fixture tem as suas.
        let expected = fixture.scrub(&format!(
            "{}{}",
            text_of(case, &value, layout),
            line_break(case)
        ));

        let printing = match changes_the_fixture(case) {
            true => {
                drop(fixture);
                (case.fixture)()
            }
            false => fixture,
        };
        let run = run_case(case, &printing, layout_name(layout));

        let (printed, silent) = match case.outcome {
            Outcome::Success => (run.stdout, run.stderr),
            Outcome::Failure => (run.stderr, run.stdout),
        };
        assert_eq!(printing.scrub(&printed), expected, "{}", case.name);
        assert_eq!(silent, "", "{}", case.name);
    }
}

#[test]
fn it_should_print_the_agent_render_of_the_json_value_for_each_command() {
    assert_command_prints_the_renderer_output(Layout::Agent);
}

#[test]
fn it_should_print_the_human_render_of_the_json_value_for_each_command() {
    assert_command_prints_the_renderer_output(Layout::Human);
}

// ---------------------------------------------------------------- `error.code` congelado (CA-7 v)

#[test]
fn it_should_answer_the_expected_error_code_in_the_json_of_each_error_case() {
    for (case, value) in CASES
        .iter()
        .zip(CASE_VALUES.iter())
        .filter(|(case, _)| case.outcome == Outcome::Failure)
    {
        assert_eq!(
            value["error"]["code"],
            json!(case.error_code.expect("an error case names its code")),
            "{}",
            case.name
        );
    }
}

// ---------------------------------------------------------------- CA-1: o `verify` sem `--format`, com `human` e com `json`

fn binary_verify(fixture: &Fixture, format: Option<&str>) -> Output {
    let dir = fixture
        .project_dir
        .to_str()
        .expect("the project directory is UTF-8");
    let mut args = vec!["--project-dir", dir];
    if let Some(format) = format {
        args.extend(["--format", format]);
    }
    args.push("verify");

    binary(&args).xdg(fixture.project.xdg()).run()
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

fn assert_text_with_symbol(text: &str, symbols: &[&str]) {
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    assert!(
        symbols
            .iter()
            .any(|symbol| first_line(text).starts_with(symbol)),
        "{text}"
    );
}

#[test]
fn it_should_print_agent_text_for_verify_success_when_no_format_is_given() {
    let output = binary_verify(&verify_ok(), None);

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_agent_text_for_verify_error_when_no_format_is_given() {
    let output = binary_verify(&verify_no_marker(), None);

    assert_eq!(output.code, Some(1));
    assert_text_with_symbol(&output.stderr, &["✘"]);
}

#[test]
fn it_should_print_human_text_for_verify_success() {
    let output = binary_verify(&verify_ok(), Some("human"));

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_human_text_for_verify_error() {
    let output = binary_verify(&verify_no_marker(), Some("human"));

    assert_eq!(output.code, Some(1));
    assert_text_with_symbol(&output.stderr, &["✘"]);
}

#[test]
fn it_should_print_a_single_json_line_for_verify_success_with_format_json() {
    let output = binary_verify(&verify_ok(), Some("json"));

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stdout.trim_end()).expect("the line is JSON")["ok"],
        json!(true)
    );
}

#[test]
fn it_should_print_a_single_json_line_for_verify_error_with_format_json() {
    let output = binary_verify(&verify_no_marker(), Some("json"));

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stderr.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stderr.trim_end()).expect("the line is JSON")["error"]
            ["code"],
        json!("no-marker")
    );
}

#[test]
fn it_should_exit_zero_with_a_broken_chain_symbol_when_the_verify_chain_is_broken() {
    let output = binary_verify(&verify_broken_chain(), None);

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✘"]);
}

// ---------------------------------------------------------------- CA-1: o `check run` sem `--format`, com `human` e com `json`

fn binary_check_run(fixture: &Fixture, format: Option<&str>) -> Output {
    let mut args = Vec::new();
    if let Some(format) = format {
        args.extend(["--format", format]);
    }
    args.extend(CHECK_RUN);

    fixture.spawn(&args)
}

#[test]
fn it_should_print_agent_text_for_check_run_success_when_no_format_is_given() {
    let output = binary_check_run(&check_run_ok(), None);

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_agent_text_for_check_run_error_when_no_format_is_given() {
    let output = binary_check_run(&check_run_blocked(), None);

    assert_eq!(output.code, Some(1));
    assert_text_with_symbol(&output.stderr, &["✘"]);
    assert_eq!(output.stdout, "");
}

#[test]
fn it_should_print_agent_text_for_a_check_run_with_nothing_to_attest_when_no_format_is_given() {
    let fixture = check_run_nothing_to_attest();

    let output = fixture.spawn(CHECK_RUN_ON_PUSH);

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_human_text_for_check_run_success() {
    let output = binary_check_run(&check_run_ok(), Some("human"));

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_human_text_for_check_run_error() {
    let output = binary_check_run(&check_run_blocked(), Some("human"));

    assert_eq!(output.code, Some(1));
    assert_text_with_symbol(&output.stderr, &["✘"]);
    assert_eq!(output.stdout, "");
}

#[test]
fn it_should_print_a_single_json_line_for_check_run_success_with_format_json() {
    let output = binary_check_run(&check_run_ok(), Some("json"));

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stdout.trim_end()).expect("the line is JSON")["event"],
        json!("before-pr")
    );
}

#[test]
fn it_should_print_a_single_json_line_for_check_run_error_with_format_json() {
    let output = binary_check_run(&check_run_blocked(), Some("json"));

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stderr.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stderr.trim_end()).expect("the line is JSON")["error"]
            ["code"],
        json!("checks-failed")
    );
}
