mod support;

use std::fs;
use std::path::{Path, PathBuf};

use omhl::guard::{ROLE_REFERENCE_COMMANDS, ROLE_REFERENCE_MATRIX};
use serde_json::{Value, json};
use support::child::{Output as Child, omhl};
use support::hook::assert_released_with_code;
use tempfile::TempDir;

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

// agent-role: regras decididas antes de ler a matriz do agente

const PORTED_AGENT: &str = "oh-my-hexflow:verifier";

/// Evento PreToolUse do Bash; sem `agent_type` é a sessão principal.
fn bash_call(command: &str, agent_type: Option<&str>) -> String {
    bash_call_with(&json!({ "command": command }), agent_type)
}

/// Como `bash_call`, com o `tool_input` inteiro à escolha (`description`, `timeout`...).
fn bash_call_with(tool_input: &Value, agent_type: Option<&str>) -> String {
    let mut event = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": "/tmp",
        "tool_name": "Bash",
        "tool_input": tool_input,
    });
    if let Some(agent_type) = agent_type {
        event["agent_type"] = json!(agent_type);
    }

    event.to_string()
}

fn run_agent_role_in(plugin_root: &Path, stdin: &str) -> Child {
    omhl(&["hook", "agent-role"])
        .env("CLAUDE_PLUGIN_ROOT", plugin_root)
        .stdin(stdin)
        .run()
}

/// Roda `omhl hook agent-role` com uma raiz de plugin vazia: nenhum arquivo de agente existe.
fn run_agent_role(command: &str, agent_type: Option<&str>) -> Child {
    let empty_root = TempDir::new().expect("plugin root is created");

    run_agent_role_in(empty_root.path(), &bash_call(command, agent_type))
}

fn assert_passed(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(
        output.stdout.is_empty(),
        "stdout stays empty: {}",
        output.stdout
    );
}

fn assert_denied(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(
        !output.stdout.is_empty(),
        "expected a deny, stdout is empty"
    );
    let decision = &json_line(&output.stdout)["hookSpecificOutput"];
    assert_eq!(decision["hookEventName"], json!("PreToolUse"));
    assert_eq!(decision["permissionDecision"], json!("deny"));
    assert!(
        decision["permissionDecisionReason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "the deny carries a reason"
    );
}

// Passa sem mexer

#[test]
fn it_should_pass_a_bash_call_without_agent_type_untouched() {
    let output = run_agent_role("omhl list", None);

    assert_passed(&output);
}

#[test]
fn it_should_pass_a_bash_call_from_explore_or_another_non_ported_agent_untouched() {
    let output = run_agent_role("omhl list", Some("Explore"));

    assert_passed(&output);
}

#[test]
fn it_should_pass_a_command_without_omhl_from_a_ported_agent() {
    let output = run_agent_role("ls -la crates", Some(PORTED_AGENT));

    assert_passed(&output);
}

#[test]
fn it_should_pass_grep_omhl_piped_to_head_from_a_ported_agent() {
    let output = run_agent_role("grep -rn omhl crates | head", Some(PORTED_AGENT));

    assert_passed(&output);
}

// OMHL_ROLE: só a palavra de atribuição

#[test]
fn it_should_deny_omhl_role_as_a_command_prefix_from_a_ported_agent() {
    let output = run_agent_role("OMHL_ROLE=omhl:verifier omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_role_as_a_command_prefix_from_the_main_session() {
    let output = run_agent_role("OMHL_ROLE=omhl:verifier omhl list", None);

    assert_denied(&output);
}

#[test]
fn it_should_deny_env_omhl_role_omhl_verifier_omhl_append() {
    let output = run_agent_role(
        "env OMHL_ROLE=omhl:verifier omhl append",
        Some(PORTED_AGENT),
    );

    assert_denied(&output);
}

#[test]
fn it_should_deny_export_omhl_role() {
    let output = run_agent_role("export OMHL_ROLE=omhl:verifier", None);

    assert_denied(&output);
}

#[test]
fn it_should_pass_grep_omhl_role_from_the_main_session() {
    let output = run_agent_role("grep OMHL_ROLE crates", None);

    assert_passed(&output);
}

#[test]
fn it_should_pass_grep_omhl_role_from_a_ported_agent() {
    let output = run_agent_role("grep OMHL_ROLE crates", Some(PORTED_AGENT));

    assert_passed(&output);
}

#[test]
fn it_should_pass_a_git_commit_message_mentioning_omhl_role() {
    let output = run_agent_role(
        "git commit -m \"document OMHL_ROLE handling\"",
        Some(PORTED_AGENT),
    );

    assert_passed(&output);
}

#[test]
fn it_should_not_treat_omhl_role_inside_a_heredoc_body_as_a_manual_prefix() {
    let output = run_agent_role("cat <<'EOF'\nOMHL_ROLE=omhl:verifier\nEOF\n", None);

    assert_passed(&output);
}

#[test]
fn it_should_not_deny_the_main_session_on_a_read_error_even_when_the_command_contains_omhl() {
    let output = run_agent_role("omhl list \"unterminated", None);

    assert_passed(&output);
}

// Comando composto

#[test]
fn it_should_deny_omhl_piped_to_another_command() {
    let output = run_agent_role("omhl list | head", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_chained_with_and() {
    let output = run_agent_role("omhl list && echo done", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_chained_with_or() {
    let output = run_agent_role("omhl list || echo failed", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_chained_with_a_semicolon() {
    let output = run_agent_role("omhl list ; echo done", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_chained_with_a_single_ampersand() {
    let output = run_agent_role("omhl list & echo done", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_two_omhl_appends_on_consecutive_lines() {
    let output = run_agent_role("omhl append\nomhl append", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_inside_a_command_substitution() {
    let output = run_agent_role("echo $(omhl list)", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_inside_backticks() {
    let output = run_agent_role("echo `omhl list`", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_inside_a_subshell() {
    let output = run_agent_role("(omhl list)", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_inside_a_brace_group() {
    let output = run_agent_role("{ omhl list; }", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_inside_an_input_process_substitution() {
    let output = run_agent_role("diff <(omhl list) /dev/null", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_inside_an_output_process_substitution() {
    let output = run_agent_role("tee >(omhl list) </dev/null", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_omhl_followed_by_a_command_on_the_next_line_after_a_heredoc_terminator() {
    let output = run_agent_role(
        "omhl append <<'EOF'\n{}\nEOF\necho done",
        Some(PORTED_AGENT),
    );

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_the_command_has_unbalanced_quotes_and_contains_omhl() {
    let output = run_agent_role("omhl list \"unterminated", Some(PORTED_AGENT));

    assert_denied(&output);
}

// Programas que embrulham

#[test]
fn it_should_pass_a_wrapper_whose_real_program_is_not_omhl() {
    let output = run_agent_role("timeout 30 grep omhl crates", Some(PORTED_AGENT));

    assert_passed(&output);
}

#[test]
fn it_should_deny_a_wrapper_with_an_option_it_cannot_skip_when_a_word_is_omhl() {
    let output = run_agent_role("nice --frobnicate omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_a_quoted_env_split_string_that_hides_omhl() {
    let output = run_agent_role("env -S \"omhl list\"", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_nice_long_adjustment_option_before_omhl() {
    let output = run_agent_role("nice --adjustment=5 omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_pass_a_wrapper_with_an_unknown_option_when_no_word_is_omhl() {
    let output = run_agent_role("nice --frobnicate grep foo crates", Some(PORTED_AGENT));

    assert_passed(&output);
}

#[test]
fn it_should_deny_omhl_after_an_opening_brace_without_a_semicolon() {
    let output = run_agent_role("{ omhl list\n}", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_pass_a_ported_agent_on_a_read_error_when_the_command_has_no_omhl() {
    let output = run_agent_role("echo \"unterminated", Some(PORTED_AGENT));

    assert_passed(&output);
}

#[test]
fn it_should_deny_env_dash_i_omhl_from_a_ported_agent() {
    let output = run_agent_role("env -i omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_env_dash_omhl_from_a_ported_agent() {
    let output = run_agent_role("env - omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_env_ignore_environment_omhl_from_a_ported_agent() {
    let output = run_agent_role("env --ignore-environment omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_env_dash_s_omhl_from_a_ported_agent() {
    let output = run_agent_role("env -S omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_exec_dash_c_omhl_from_a_ported_agent() {
    let output = run_agent_role("exec -c omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

// Papel fora dos sete

#[test]
fn it_should_deny_omhl_from_a_plugin_agent_whose_role_is_outside_the_seven() {
    let output = run_agent_role("omhl list", Some("oh-my-hexflow:janitor"));

    assert_denied(&output);
}

#[test]
fn it_should_reject_a_role_containing_a_path_separator_before_opening_any_file() {
    let root = TempDir::new().expect("plugin root is created");
    fs::create_dir(root.path().join("agents")).expect("agents directory is created");
    // Se o hook abrisse o arquivo, a matriz abaixo liberaria o `list`.
    fs::write(
        root.path().join("secret.md"),
        "---\nomhl-allow:\n  commands: [list]\n---\n",
    )
    .expect("traversal target is written");
    let stdin = bash_call("omhl list", Some("oh-my-hexflow:../secret"));

    let output = run_agent_role_in(root.path(), &stdin);

    assert_denied(&output);
}

// Robustez

#[test]
fn it_should_exit_zero_with_a_stderr_line_on_invalid_stdin_for_agent_role() {
    let empty_root = TempDir::new().expect("plugin root is created");

    let output = run_agent_role_in(empty_root.path(), "{");

    assert_released_with_code(&output, "input-invalid-json");
}

// agent-role: matriz do agente e reescrita do comando
//
// Os agentes de `tests/fixtures/agents/` são FIXTURES; os reais chegam na S4.

const APPEND_HEREDOC: &str = "omhl append <<'EOF'\n{\"items\":[]}\nEOF\n";
const ATTEST: &str = "omhl attest checks.run-tests work-items.w1 --outcome pass";

/// Uma chamada de exemplo por folha da CLI; a chave é a folha como aparece em `commands`.
const LEAF_SAMPLES: &[(&str, &str)] = &[
    ("list", "omhl list --type decision"),
    ("get", "omhl get work-items.w1"),
    ("render", "omhl render"),
    ("verify", "omhl verify"),
    ("byproduct", "omhl byproduct checks.run-tests work-items.w1"),
    (
        "report answers",
        "omhl report answers --work-item work-items.w1",
    ),
    ("report recurrences", "omhl report recurrences --min 2"),
    ("report retro", "omhl report retro work-items.w1"),
    ("help", "omhl help"),
    ("append", APPEND_HEREDOC),
    ("spec init", "omhl spec init work-items.w1 my-slug"),
    ("spec validate", "omhl spec validate work-items.w1"),
    ("attest", ATTEST),
    ("load", "omhl load --dry-run"),
    ("close", "omhl close work-items.w1"),
    ("gate-pr", "omhl gate-pr work-items.w1"),
    ("gate-worktree", "omhl gate-worktree --branch feat/x"),
    ("project", "omhl project"),
    ("guideline retire", "omhl guideline retire guidelines.g1"),
    ("check run", "omhl check run --event before-commit"),
    ("check sweep", "omhl check sweep"),
    ("check retire", "omhl check retire checks.lint"),
    ("hooks install", "omhl hooks install"),
    ("hooks post-commit", "omhl hooks post-commit"),
    ("mcp", "omhl mcp"),
    ("hook session-start", "omhl hook session-start"),
    ("hook spec-lock", "omhl hook spec-lock"),
    ("hook agent-role", "omhl hook agent-role"),
];

/// Um comando de um papel e o veredito que a matriz de referência dá a ele.
struct Case {
    role: &'static str,
    command: &'static str,
    allowed: bool,
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// `omhl:analyst` -> `analyst`.
fn bare_role(reference_role: &str) -> &str {
    reference_role
        .strip_prefix("omhl:")
        .expect("reference roles carry the omhl: prefix")
}

fn allowed_commands(role: &str) -> &'static [&'static str] {
    ROLE_REFERENCE_COMMANDS
        .iter()
        .find(|(reference, _)| bare_role(reference) == role)
        .map(|(_, commands)| *commands)
        .expect("the role is one of the seven")
}

/// Cada par (papel, comando de exemplo da folha), com o veredito dado por `commands` do papel.
fn cases_for(samples: &[(&'static str, &'static str)]) -> Vec<Case> {
    let mut cases = Vec::new();
    for (reference_role, commands) in ROLE_REFERENCE_COMMANDS {
        for &(leaf, command) in samples {
            cases.push(Case {
                role: bare_role(reference_role),
                command,
                allowed: commands.contains(&leaf),
            });
        }
    }

    cases
}

fn samples_of(leaves: &[&str]) -> Vec<(&'static str, &'static str)> {
    leaves
        .iter()
        .map(|leaf| {
            LEAF_SAMPLES
                .iter()
                .find(|(sample_leaf, _)| sample_leaf == leaf)
                .copied()
                .expect("the leaf has a sample")
        })
        .collect()
}

/// Roda o hook como o subagente `oh-my-hexflow:<role>`, com os agentes de fixture.
fn run_as(role: &str, command: &str) -> Child {
    let agent_type = format!("oh-my-hexflow:{role}");

    run_agent_role_in(&fixtures_root(), &bash_call(command, Some(&agent_type)))
}

/// O `hookSpecificOutput` da resposta; falha se o hook ficou calado.
fn decision_of(output: &Child) -> Value {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(
        !output.stdout.is_empty(),
        "expected a decision, stdout is empty"
    );

    json_line(&output.stdout)["hookSpecificOutput"].clone()
}

fn assert_allowed_with_prefix(output: &Child, role: &str, command: &str) {
    let decision = decision_of(output);

    assert_eq!(
        decision["permissionDecision"],
        json!("allow"),
        "{role}: {command}"
    );
    assert_eq!(
        decision["updatedInput"]["command"],
        json!(format!("OMHL_ROLE=omhl:{role} {command}")),
        "{role}: {command}"
    );
}

/// Nega com um motivo que lista o que o papel pode rodar (a primeira folha dele).
fn assert_denied_listing(output: &Child, role: &str, command: &str) {
    let decision = decision_of(output);
    let reason = decision["permissionDecisionReason"]
        .as_str()
        .unwrap_or_default();

    assert_eq!(
        decision["permissionDecision"],
        json!("deny"),
        "{role}: {command}"
    );
    assert!(
        reason.contains(allowed_commands(role)[0]),
        "{role}: {command}: the reason lists what the role may run: {reason}"
    );
}

fn assert_case(case: &Case) {
    let output = run_as(case.role, case.command);

    if case.allowed {
        assert_allowed_with_prefix(&output, case.role, case.command);
    } else {
        assert_denied_listing(&output, case.role, case.command);
    }
}

/// Roda o comando como cada um dos sete papéis; o veredito vem de `commands` do papel para `leaf`.
fn assert_for_every_role(leaf: &'static str, command: &'static str) {
    cases_for(&[(leaf, command)]).iter().for_each(assert_case);
}

fn assert_verifier_allowed(command: &str) {
    let output = run_as("verifier", command);

    assert_allowed_with_prefix(&output, "verifier", command);
}

// Matriz: todos os comandos de cada papel

#[test]
fn it_should_allow_every_command_inside_the_matrix_of_each_of_the_seven_roles() {
    let cases = cases_for(LEAF_SAMPLES);

    cases
        .iter()
        .filter(|case| case.allowed)
        .for_each(assert_case);
}

#[test]
fn it_should_deny_every_command_outside_the_matrix_of_each_of_the_seven_roles() {
    let cases = cases_for(LEAF_SAMPLES);

    cases
        .iter()
        .filter(|case| !case.allowed)
        .for_each(assert_case);
}

// Matriz: uma linha da tabela folha a folha por teste

#[test]
fn it_should_allow_omhl_list_get_and_render_to_every_role() {
    let cases = cases_for(&samples_of(&["list", "get", "render"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_allow_omhl_verify_and_byproduct_to_every_role() {
    let cases = cases_for(&samples_of(&["verify", "byproduct"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_allow_omhl_report_answers_recurrences_and_retro_to_every_role() {
    let cases = cases_for(&samples_of(&[
        "report answers",
        "report recurrences",
        "report retro",
    ]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_allow_omhl_help_to_every_role() {
    let cases = cases_for(&samples_of(&["help"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_allow_omhl_append_to_every_role_but_the_planner() {
    let cases = cases_for(&samples_of(&["append"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_allow_omhl_spec_init_only_to_the_planner() {
    let cases = cases_for(&samples_of(&["spec init"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_allow_omhl_spec_validate_only_to_the_planner_the_critic_and_the_verifier() {
    let cases = cases_for(&samples_of(&["spec validate"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_deny_omhl_attest_load_close_gate_pr_and_gate_worktree_to_every_role() {
    let cases = cases_for(&samples_of(&[
        "attest",
        "load",
        "close",
        "gate-pr",
        "gate-worktree",
    ]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_deny_omhl_project_and_guideline_retire_to_every_role() {
    let cases = cases_for(&samples_of(&["project", "guideline retire"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_deny_omhl_check_run_sweep_and_retire_to_every_role() {
    let cases = cases_for(&samples_of(&["check run", "check sweep", "check retire"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_deny_omhl_hooks_install_and_post_commit_to_every_role() {
    let cases = cases_for(&samples_of(&["hooks install", "hooks post-commit"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_deny_omhl_mcp_and_every_hook_event_to_every_role() {
    let cases = cases_for(&samples_of(&[
        "mcp",
        "hook session-start",
        "hook spec-lock",
        "hook agent-role",
    ]));

    cases.iter().for_each(assert_case);
}

// Forma da resposta e reescrita

#[test]
fn it_should_answer_allow_with_updated_input_for_a_command_inside_the_matrix() {
    let output = run_as("verifier", "omhl list");

    let decision = decision_of(&output);

    assert_eq!(decision["permissionDecision"], json!("allow"));
    assert!(decision["updatedInput"].is_object());
}

#[test]
fn it_should_answer_without_updated_input_for_a_command_outside_the_matrix() {
    let output = run_as("planner", "omhl append");

    let decision = decision_of(&output);

    assert_eq!(decision["permissionDecision"], json!("deny"));
    assert!(decision.get("updatedInput").is_none());
}

#[test]
fn it_should_rewrite_the_command_with_the_omhl_role_prefix() {
    let output = run_as("critic", "omhl get work-items.w1");

    assert_allowed_with_prefix(&output, "critic", "omhl get work-items.w1");
}

#[test]
fn it_should_keep_description_timeout_and_run_in_background_in_the_rewritten_input() {
    let stdin = bash_call_with(
        &json!({
            "command": "omhl list",
            "description": "List the nodes",
            "timeout": 5000,
            "run_in_background": false,
        }),
        Some(PORTED_AGENT),
    );

    let output = run_agent_role_in(&fixtures_root(), &stdin);

    assert_eq!(
        decision_of(&output)["updatedInput"],
        json!({
            "command": "OMHL_ROLE=omhl:verifier omhl list",
            "description": "List the nodes",
            "timeout": 5000,
            "run_in_background": false,
        })
    );
}

#[test]
fn it_should_not_add_description_timeout_or_run_in_background_when_the_call_had_none() {
    let output = run_as("verifier", "omhl list");

    assert_eq!(
        decision_of(&output)["updatedInput"],
        json!({ "command": "OMHL_ROLE=omhl:verifier omhl list" })
    );
}

#[test]
fn it_should_answer_allow_with_only_permission_decision_updated_input_and_the_reason() {
    let output = run_as("verifier", "omhl list");

    let decision = decision_of(&output);

    let answer = json_line(&output.stdout);
    let mut top_level: Vec<&String> = answer.as_object().expect("an object").keys().collect();
    top_level.sort();
    let mut decision_keys: Vec<&String> = decision.as_object().expect("an object").keys().collect();
    decision_keys.sort();
    assert_eq!(top_level, ["hookSpecificOutput"]);
    assert_eq!(
        decision_keys,
        [
            "hookEventName",
            "permissionDecision",
            "permissionDecisionReason",
            "updatedInput"
        ]
    );
}

// Subcomando depois das opções globais

#[test]
fn it_should_resolve_the_subcommand_after_the_project_dir_option() {
    assert_verifier_allowed("omhl --project-dir . append");
}

#[test]
fn it_should_resolve_the_subcommand_after_the_db_option() {
    assert_verifier_allowed("omhl --db x list");
}

#[test]
fn it_should_resolve_the_subcommand_after_the_db_option_written_with_an_equals_sign() {
    assert_verifier_allowed("omhl --db=x list");
}

#[test]
fn it_should_resolve_the_subcommand_after_the_project_dir_option_written_with_an_equals_sign() {
    assert_verifier_allowed("omhl --project-dir=. list");
}

#[test]
fn it_should_resolve_the_subcommand_after_both_global_options() {
    assert_verifier_allowed("omhl --db x --project-dir . list");
}

#[test]
fn it_should_deny_a_leaf_outside_the_matrix_after_a_global_option() {
    let command = format!("omhl --db x {}", &ATTEST["omhl ".len()..]);

    let output = run_as("verifier", &command);

    assert_denied_listing(&output, "verifier", &command);
}

// Redirecionamento (não é separador)

#[test]
fn it_should_accept_omhl_list_2_redirected_to_1() {
    assert_verifier_allowed("omhl list 2>&1");
}

#[test]
fn it_should_accept_omhl_list_with_stderr_sent_to_dev_null() {
    assert_verifier_allowed("omhl list 2>/dev/null");
}

#[test]
fn it_should_accept_omhl_list_with_stdout_sent_to_a_file() {
    assert_verifier_allowed("omhl list >out");
}

#[test]
fn it_should_accept_omhl_list_with_stdout_appended_to_a_file() {
    assert_verifier_allowed("omhl list >>out");
}

#[test]
fn it_should_accept_omhl_list_with_stdin_read_from_a_file() {
    assert_verifier_allowed("omhl list <in");
}

#[test]
fn it_should_accept_omhl_list_with_both_streams_sent_to_a_file() {
    assert_verifier_allowed("omhl list &> out");
}

#[test]
fn it_should_accept_omhl_append_fed_by_a_here_string() {
    assert_verifier_allowed("omhl append <<< '{\"items\":[]}'");
}

#[test]
fn it_should_accept_a_heredoc_command_ending_with_a_trailing_newline() {
    assert_verifier_allowed(APPEND_HEREDOC);
}

// Comando composto: o que o heredoc esconde

#[test]
fn it_should_accept_omhl_append_fed_by_a_heredoc_whose_body_contains_pipe_and_and_and_a_quote() {
    let command = "omhl append <<'EOF'\n{\"note\":\"a | b && it's done\"}\nEOF\n";

    assert_verifier_allowed(command);
}

// Programas que embrulham o `omhl`

/// A matriz vale através do embrulho: `omhl list` passa com o prefixo e `omhl attest` é negado.
fn assert_wrapper_applies_matrix(wrapper: &str) {
    let inside = format!("{wrapper} omhl list");
    let outside = format!("{wrapper} {ATTEST}");

    let allowed = run_as("verifier", &inside);
    let denied = run_as("verifier", &outside);

    assert_allowed_with_prefix(&allowed, "verifier", &inside);
    assert_denied_listing(&denied, "verifier", &outside);
}

#[test]
fn it_should_deny_env_omhl_append_from_a_ported_agent_when_the_role_has_no_append() {
    let output = run_as("planner", "env omhl append");

    assert_denied_listing(&output, "planner", "env omhl append");
}

#[test]
fn it_should_allow_env_omhl_append_with_the_prefix_when_the_role_has_append() {
    assert_verifier_allowed("env omhl append");
}

#[test]
fn it_should_apply_the_matrix_to_timeout_30_omhl() {
    assert_wrapper_applies_matrix("timeout 30");
}

#[test]
fn it_should_apply_the_matrix_to_time_omhl() {
    assert_wrapper_applies_matrix("time");
}

#[test]
fn it_should_apply_the_matrix_to_nice_n_5_omhl() {
    assert_wrapper_applies_matrix("nice -n 5");
}

#[test]
fn it_should_apply_the_matrix_to_nohup_omhl() {
    assert_wrapper_applies_matrix("nohup");
}

#[test]
fn it_should_apply_the_matrix_to_command_omhl() {
    assert_wrapper_applies_matrix("command");
}

#[test]
fn it_should_apply_the_matrix_to_exec_omhl() {
    assert_wrapper_applies_matrix("exec");
}

#[test]
fn it_should_apply_the_matrix_to_stdbuf_o_l_omhl() {
    assert_wrapper_applies_matrix("stdbuf -oL");
}

#[test]
fn it_should_apply_the_matrix_to_xargs_omhl() {
    assert_wrapper_applies_matrix("xargs");
}

#[test]
fn it_should_apply_the_matrix_to_rtk_proxy_omhl() {
    assert_wrapper_applies_matrix("rtk proxy");
}

#[test]
fn it_should_apply_the_matrix_to_a_nested_wrapper_chain() {
    assert_wrapper_applies_matrix("rtk proxy env timeout 30");
}

// Folhas só de leitura

#[test]
fn it_should_allow_omhl_dash_dash_help_to_every_role() {
    assert_for_every_role("help", "omhl --help");
}

#[test]
fn it_should_allow_omhl_dash_h_to_every_role() {
    assert_for_every_role("help", "omhl -h");
}

#[test]
fn it_should_allow_omhl_dash_capital_v_to_every_role() {
    assert_for_every_role("help", "omhl -V");
}

#[test]
fn it_should_allow_omhl_dash_dash_version_to_every_role() {
    assert_for_every_role("help", "omhl --version");
}

#[test]
fn it_should_allow_omhl_spec_help_to_every_role() {
    assert_for_every_role("help", "omhl spec help");
}

#[test]
fn it_should_allow_omhl_check_help_run_to_every_role() {
    assert_for_every_role("help", "omhl check help run");
}

#[test]
fn it_should_allow_omhl_project_dir_dot_help_to_every_role() {
    assert_for_every_role("help", "omhl --project-dir . --help");
}

#[test]
fn it_should_allow_omhl_version_after_a_global_option_to_every_role() {
    assert_for_every_role("help", "omhl --db x --version");
}

#[test]
fn it_should_resolve_omhl_spec_init_help_to_the_spec_init_leaf() {
    assert_for_every_role("spec init", "omhl spec init --help");
}

#[test]
fn it_should_resolve_omhl_attest_help_to_the_attest_leaf() {
    assert_for_every_role("attest", "omhl attest --help");
}

// Matriz em YAML

/// Arquivo de agente do verifier com o trecho dado no frontmatter e o corpo dado.
fn agent_file(frontmatter_extra: &str, body: &str) -> String {
    format!(
        "---\nname: verifier\ndescription: Fixture agent\ntools: Bash\n{frontmatter_extra}---\n\n{body}\n"
    )
}

/// Roda o verifier numa raiz de plugin cujo `agents/verifier.md` tem o conteúdo dado.
fn run_verifier_with_agent_file(content: &str, command: &str) -> Child {
    let root = TempDir::new().expect("plugin root is created");
    fs::create_dir(root.path().join("agents")).expect("agents directory is created");
    fs::write(root.path().join("agents/verifier.md"), content).expect("agent file is written");

    run_agent_role_in(root.path(), &bash_call(command, Some(PORTED_AGENT)))
}

#[test]
fn it_should_deny_when_the_agent_file_is_missing() {
    let output = run_agent_role("omhl list", Some(PORTED_AGENT));

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_the_agent_file_is_unreadable() {
    let root = TempDir::new().expect("plugin root is created");
    // Um diretório no lugar do arquivo: existe, mas não dá para ler como texto.
    fs::create_dir_all(root.path().join("agents/verifier.md")).expect("directory is created");

    let output = run_agent_role_in(root.path(), &bash_call("omhl list", Some(PORTED_AGENT)));

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_the_omhl_allow_yaml_is_malformed() {
    let content = agent_file("omhl-allow:\n  commands: [list\n  append: {\n", "Body.");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_the_omhl_allow_key_is_missing() {
    let content = agent_file("", "Body.");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_the_agent_file_has_no_frontmatter() {
    let output = run_verifier_with_agent_file("Just a body, no frontmatter.\n", "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_omhl_allow_appears_only_in_the_body_of_the_agent_file() {
    let content = agent_file("", "omhl-allow:\n  commands: [list]");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_omhl_allow_is_not_a_mapping() {
    let content = agent_file("omhl-allow: list\n", "Body.");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_omhl_allow_has_no_commands_key() {
    let content = agent_file("omhl-allow:\n  other: [list]\n", "Body.");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_commands_is_not_a_list() {
    let content = agent_file("omhl-allow:\n  commands: list\n", "Body.");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_append_is_listed_in_commands_but_the_append_key_is_missing() {
    let content = agent_file("omhl-allow:\n  commands: [append, list]\n", "Body.");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_the_append_key_is_present_but_append_is_not_listed_in_commands() {
    let content = agent_file(
        "omhl-allow:\n  commands: [list]\n  append:\n    nodes: []\n    relations: []\n    retire: []\n",
        "Body.",
    );

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_a_commands_item_is_not_a_string() {
    let content = agent_file("omhl-allow:\n  commands: [list, 5]\n", "Body.");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

/// Matriz do verifier com `append`, onde o valor de `key` (`nodes`, `relations` ou `retire`) é o dado; as outras listas são válidas.
fn append_matrix_with(key: &str, value: &str) -> String {
    let lists: String = ["nodes", "relations", "retire"]
        .iter()
        .map(|list| format!("    {list}: {}\n", if *list == key { value } else { "[]" }))
        .collect();

    agent_file(
        &format!("omhl-allow:\n  commands: [append, list]\n  append:\n{lists}"),
        "Body.",
    )
}

#[test]
fn it_should_deny_when_an_append_list_is_not_a_list() {
    for key in ["nodes", "relations", "retire"] {
        let content = append_matrix_with(key, "premise");

        let output = run_verifier_with_agent_file(&content, "omhl list");

        assert_denied(&output);
    }
}

#[test]
fn it_should_deny_when_an_append_list_item_is_not_a_string() {
    for key in ["nodes", "relations", "retire"] {
        let content = append_matrix_with(key, "[premise, 5]");

        let output = run_verifier_with_agent_file(&content, "omhl list");

        assert_denied(&output);
    }
}

#[test]
fn it_should_deny_when_omhl_allow_has_an_unknown_key() {
    let content = agent_file(
        "omhl-allow:\n  commands: [list]\n  extra: [list]\n",
        "Body.",
    );

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_deny_when_the_append_block_has_an_unknown_key() {
    let content = agent_file(
        "omhl-allow:\n  commands: [append, list]\n  append:\n    nodes: []\n    relations: []\n    retire: []\n    extra: []\n",
        "Body.",
    );

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied(&output);
}

#[test]
fn it_should_allow_a_command_when_the_append_block_is_well_formed() {
    let content = append_matrix_with("nodes", "[premise]");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_allowed_with_prefix(&output, "verifier", "omhl list");
}

// Fixtures iguais à matriz de referência

/// Os itens de uma lista em linha (`key: [a, "b c"]`) do arquivo, ordenados; vazio se a chave não existe.
fn flow_list(file: &str, key: &str) -> Vec<String> {
    let marker = format!("{key}: [");
    let Some(line) = file
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(&marker))
    else {
        return Vec::new();
    };
    let mut items: Vec<String> = line[marker.len()..]
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_owned())
        .filter(|item| !item.is_empty())
        .collect();
    items.sort();

    items
}

fn sorted(items: &[&str]) -> Vec<String> {
    let mut items: Vec<String> = items.iter().map(|item| (*item).to_owned()).collect();
    items.sort();

    items
}

#[test]
fn it_should_keep_every_agent_fixture_equal_to_the_reference_role_matrix() {
    for (reference_role, commands) in ROLE_REFERENCE_COMMANDS {
        let role = bare_role(reference_role);
        let file = fs::read_to_string(fixtures_root().join(format!("agents/{role}.md")))
            .expect("the fixture exists");
        let (_, nodes, relations, retire) = ROLE_REFERENCE_MATRIX
            .iter()
            .find(|(matrix_role, ..)| *matrix_role == reference_role)
            .expect("the role is in the reference matrix");

        assert_eq!(flow_list(&file, "commands"), sorted(commands), "{role}");
        assert_eq!(
            file.contains("\n  append:\n"),
            commands.contains(&"append"),
            "{role}: append key"
        );
        assert_eq!(flow_list(&file, "nodes"), sorted(nodes), "{role}");
        assert_eq!(flow_list(&file, "relations"), sorted(relations), "{role}");
        assert_eq!(flow_list(&file, "retire"), sorted(retire), "{role}");
    }
}
