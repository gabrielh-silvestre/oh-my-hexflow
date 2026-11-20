use crate::support;

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::CommandFactory;
use omhl::adapters::cli::Cli;
use omhl::guard::ROLE_REFERENCE_COMMANDS;
use serde_json::{Value, json};
use support::child::{Output as Child, omhl};
use support::hook::{
    assert_released_with_code, bash_call, bash_call_with, decision_of, json_line, run_agent_role_in,
};
use tempfile::TempDir;

// agent-role: regras decididas antes de ler a matriz do agente

const PORTED_AGENT: &str = "oh-my-hexflow:verifier";

/// Roda `omhl hook agent-role` com a raiz das fixtures, onde os sete papéis têm `agents/<papel>.md` legível:
/// uma negação vem da regra que o teste mira, nunca de um arquivo de agente ausente.
fn run_agent_role(command: &str, agent_type: Option<&str>) -> Child {
    run_agent_role_in(&fixtures_root(), &bash_call(command, agent_type))
}

fn assert_passed(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(
        output.stdout.is_empty(),
        "stdout stays empty: {}",
        output.stdout
    );
}

fn assert_denied(output: &Child, context: &str) {
    assert_eq!(output.code, Some(0), "{context}: stderr: {}", output.stderr);
    assert!(
        !output.stdout.is_empty(),
        "{context}: expected a deny, stdout is empty"
    );
    let decision = &json_line(&output.stdout)["hookSpecificOutput"];
    assert_eq!(decision["hookEventName"], json!("PreToolUse"));
    assert_eq!(decision["permissionDecision"], json!("deny"));
    assert!(
        decision["permissionDecisionReason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "{context}: the deny carries a reason"
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

const ROLE_ASSIGNMENT_DENIAL: &str = "OMHL_ROLE is set by the plugin hook only";

#[test]
fn it_should_deny_omhl_role_as_a_command_prefix_from_a_ported_agent() {
    let output = run_agent_role("OMHL_ROLE=omhl:verifier omhl list", Some(PORTED_AGENT));

    assert_denied_mentioning(&output, ROLE_ASSIGNMENT_DENIAL);
}

#[test]
fn it_should_deny_omhl_role_as_a_command_prefix_from_the_main_session() {
    let output = run_agent_role("OMHL_ROLE=omhl:verifier omhl list", None);

    assert_denied_mentioning(&output, ROLE_ASSIGNMENT_DENIAL);
}

#[test]
fn it_should_deny_env_omhl_role_omhl_verifier_omhl_append() {
    let output = run_agent_role(
        "env OMHL_ROLE=omhl:verifier omhl append",
        Some(PORTED_AGENT),
    );

    assert_denied_mentioning(&output, ROLE_ASSIGNMENT_DENIAL);
}

#[test]
fn it_should_deny_export_omhl_role() {
    let output = run_agent_role("export OMHL_ROLE=omhl:verifier", None);

    assert_denied_mentioning(&output, ROLE_ASSIGNMENT_DENIAL);
}

#[test]
fn it_should_deny_every_builtin_that_assigns_omhl_role() {
    for command in ["declare", "typeset", "readonly", "local"] {
        let command = format!("{command} OMHL_ROLE=omhl:critic");
        let output = run_agent_role(&command, None);

        assert_reason_starts_with_code(&output, "role-assignment", &command);
    }
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
fn it_should_deny_omhl_combined_with_another_command_by_any_separator() {
    for command in [
        "omhl list | head",
        "omhl list && echo done",
        "omhl list || echo failed",
        "omhl list ; echo done",
        "omhl list & echo done",
        "omhl append\nomhl append",
        "echo $(omhl list)",
        "echo `omhl list`",
        "(omhl list)",
        "{ omhl list; }",
        "{ omhl list\n}",
        "diff <(omhl list) /dev/null",
        "tee >(omhl list) </dev/null",
        "omhl append <<'EOF'\n{}\nEOF\necho done",
    ] {
        assert_denied_with_code(command, "compound-command");
    }
}

#[test]
fn it_should_deny_when_the_command_has_unbalanced_quotes_and_contains_omhl() {
    let output = run_as("verifier", "omhl list \"unterminated");

    assert_denied_mentioning(&output, "unclosed quote");
}

// Programas que embrulham

#[test]
fn it_should_pass_a_wrapper_whose_real_program_is_not_omhl() {
    let output = run_agent_role("timeout 30 grep omhl crates", Some(PORTED_AGENT));

    assert_passed(&output);
}

#[test]
fn it_should_deny_a_wrapper_option_the_hook_cannot_skip_before_omhl() {
    for command in [
        "nice --frobnicate omhl list",
        "env -S \"omhl list\"",
        "nice --adjustment=5 omhl list",
        "env -i omhl list",
        "env - omhl list",
        "env --ignore-environment omhl list",
        "env -S omhl list",
        "exec -c omhl list",
    ] {
        assert_denied_with_code(command, "wrapper-unreadable");
    }
}

#[test]
fn it_should_pass_a_wrapper_with_an_unknown_option_when_no_word_is_omhl() {
    let output = run_agent_role("nice --frobnicate grep foo crates", Some(PORTED_AGENT));

    assert_passed(&output);
}

#[test]
fn it_should_pass_a_ported_agent_on_a_read_error_when_the_command_has_no_omhl() {
    let output = run_agent_role("echo \"unterminated", Some(PORTED_AGENT));

    assert_passed(&output);
}

// Papel fora dos sete

#[test]
fn it_should_deny_omhl_from_a_plugin_agent_whose_role_is_outside_the_seven() {
    // Existe `agents/janitor.md` com `commands: [list]`: só a lista fechada de papéis nega.
    let output = run_as("janitor", "omhl list");

    assert_denied_mentioning(&output, "no omhl role matrix");
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

    assert_denied_mentioning(&output, "no omhl role matrix");
}

// Ramos de borda do `agent_type` e da raiz do plugin

#[test]
fn it_should_deny_a_plugin_agent_type_with_an_empty_or_capitalised_role() {
    for agent_type in ["oh-my-hexflow:", "oh-my-hexflow:Verifier"] {
        let output = run_agent_role("omhl list", Some(agent_type));

        assert_reason_starts_with_code(&output, "role-unknown", agent_type);
    }
}

#[test]
fn it_should_pass_an_agent_type_outside_the_plugin_prefix_even_when_the_role_name_matches() {
    let output = run_agent_role("omhl list", Some("other:verifier"));

    assert_passed(&output);
}

#[test]
fn it_should_pass_an_agent_type_that_is_not_a_string() {
    let mut event: Value =
        serde_json::from_str(&bash_call("omhl list", None)).expect("the event is JSON");
    event["agent_type"] = json!(7);

    let output = run_agent_role_in(&fixtures_root(), &event.to_string());

    assert_passed(&output);
}

#[test]
fn it_should_deny_a_ported_agent_when_the_plugin_root_is_not_set() {
    let stdin = bash_call("omhl list", Some(PORTED_AGENT));

    let output = omhl(&["hook", "agent-role"]).stdin(&stdin).run();

    assert_denied_mentioning(&output, "CLAUDE_PLUGIN_ROOT");
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
// Os agentes de `tests/fixtures/agents/` são FIXTURES, não os agentes do plugin.

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
    ("hook session-start", "omhl hook session-start"),
    ("hook spec-lock", "omhl hook spec-lock"),
    ("hook agent-role", "omhl hook agent-role"),
    (
        "hook wt-post-start",
        "omhl hook wt-post-start --branch feat/wi-x__y",
    ),
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
fn it_should_deny_every_hook_event_to_every_role() {
    let cases = cases_for(&samples_of(&[
        "hook session-start",
        "hook spec-lock",
        "hook agent-role",
    ]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_deny_hook_wt_post_start_for_every_role() {
    let cases = cases_for(&samples_of(&["hook wt-post-start"]));

    cases.iter().for_each(assert_case);
}

#[test]
fn it_should_deny_a_leaf_the_cli_does_not_have_to_every_role() {
    let cases = cases_for(&[("frobnicate", "omhl frobnicate")]);

    cases.iter().for_each(assert_case);
}

// Listas à mão contra o clap

/// As folhas reais da CLI: `<grupo> <folha>` dentro de um grupo, o nome sozinho fora dele.
/// O `help` implícito do clap fica de fora: a matriz o trata como folha só de leitura.
fn cli_leaves() -> BTreeSet<String> {
    let cli = Cli::command();
    let mut leaves = BTreeSet::new();
    for command in cli.get_subcommands().filter(|c| c.get_name() != "help") {
        if !command.has_subcommands() {
            leaves.insert(command.get_name().to_owned());
        }
        for leaf in command.get_subcommands().filter(|c| c.get_name() != "help") {
            leaves.insert(format!("{} {}", command.get_name(), leaf.get_name()));
        }
    }

    leaves
}

#[test]
fn it_should_have_a_sample_for_every_leaf_of_the_cli() {
    let sampled: BTreeSet<String> = LEAF_SAMPLES
        .iter()
        .map(|(leaf, _)| (*leaf).to_owned())
        .filter(|leaf| leaf != "help")
        .collect();

    assert_eq!(sampled, cli_leaves());
}

// Forma da resposta e reescrita

#[test]
fn it_should_answer_allow_with_updated_input_for_a_command_inside_the_matrix() {
    let output = run_as("verifier", "omhl list");

    let decision = decision_of(&output);

    assert_eq!(decision["permissionDecision"], json!("allow"));
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

/// `time` é palavra reservada do bash: o prefixo vem depois dela (e do `-p`), senão vira o `/usr/bin/time`.
#[test]
fn it_should_apply_the_matrix_to_time_omhl_with_the_prefix_after_the_keyword() {
    let allowed = run_as("verifier", "time omhl list");
    let denied = run_as("verifier", &format!("time {ATTEST}"));

    assert_eq!(
        decision_of(&allowed)["updatedInput"]["command"],
        json!("time OMHL_ROLE=omhl:verifier omhl list")
    );
    assert_denied_listing(&denied, "verifier", &format!("time {ATTEST}"));
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
    let empty_root = TempDir::new().expect("plugin root is created");

    let output = run_agent_role_in(
        empty_root.path(),
        &bash_call("omhl list", Some(PORTED_AGENT)),
    );

    assert_denied_mentioning(&output, "cannot read the agent file");
}

#[test]
fn it_should_deny_when_the_agent_file_is_unreadable() {
    let root = TempDir::new().expect("plugin root is created");
    // Um diretório no lugar do arquivo: existe, mas não dá para ler como texto.
    fs::create_dir_all(root.path().join("agents/verifier.md")).expect("directory is created");

    let output = run_agent_role_in(root.path(), &bash_call("omhl list", Some(PORTED_AGENT)));

    assert_denied_mentioning(&output, "cannot read the agent file");
}

/// O formato de cada matriz inválida é provado em `workflow/role_allow.rs`; aqui só o contrato do hook.
#[test]
fn it_should_deny_when_the_omhl_allow_yaml_is_malformed() {
    let content = agent_file("omhl-allow:\n  commands: [list\n  append: {\n", "Body.");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_denied_mentioning(&output, "without a valid omhl-allow");
}

#[test]
fn it_should_read_an_agent_file_with_crlf_line_endings() {
    let content = agent_file("omhl-allow:\n  commands: [list]\n", "Body.").replace('\n', "\r\n");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_allowed_with_prefix(&output, "verifier", "omhl list");
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
fn it_should_allow_a_command_when_the_append_block_is_well_formed() {
    let content = append_matrix_with("nodes", "[premise]");

    let output = run_verifier_with_agent_file(&content, "omhl list");

    assert_allowed_with_prefix(&output, "verifier", "omhl list");
}

// Prefixo: só vale quando o `omhl` é provadamente o comando que o recebe

fn assert_denied_mentioning(output: &Child, fragment: &str) {
    assert_denied(output, fragment);
    let decision = decision_of(output);
    let reason = decision["permissionDecisionReason"]
        .as_str()
        .unwrap_or_default();

    assert!(
        reason.contains(fragment),
        "the reason mentions {fragment}: {reason}"
    );
}

/// Nega com um motivo que começa por `<code>: `; `context` (o comando ou o `agent_type`) vai em toda mensagem de falha.
fn assert_reason_starts_with_code(output: &Child, code: &str, context: &str) {
    assert_denied(output, context);
    let decision = decision_of(output);
    let reason = decision["permissionDecisionReason"]
        .as_str()
        .unwrap_or_default();

    assert!(
        reason.starts_with(&format!("{code}: ")),
        "{context}: the reason starts with `{code}: `: {reason}"
    );
}

fn assert_denied_with_code(command: &str, code: &str) {
    assert_reason_starts_with_code(&run_as("verifier", command), code, command);
}

fn assert_denied_from_any_origin_with_code(command: &str, code: &str) {
    assert_denied_with_code(command, code);
    assert_reason_starts_with_code(&run_agent_role(command, None), code, command);
}

fn assert_denied_from_any_origin(command: &str) {
    assert_denied(&run_as("verifier", command), command);
    assert_denied(&run_agent_role(command, None), command);
}

#[test]
fn it_should_start_every_denial_reason_with_its_code() {
    let empty_root = TempDir::new().expect("plugin root is created");
    let unreadable_matrix = run_agent_role_in(
        empty_root.path(),
        &bash_call("omhl list", Some(PORTED_AGENT)),
    );
    let denials = [
        (
            "role-assignment",
            "OMHL_ROLE=x omhl list",
            run_agent_role("OMHL_ROLE=x omhl list", None),
        ),
        (
            "role-unset",
            "env -u OMHL_ROLE omhl list",
            run_agent_role("env -u OMHL_ROLE omhl list", None),
        ),
        (
            "heredoc-substitution",
            "omhl append <<EOF\n$(id)\nEOF\n",
            run_as("verifier", "omhl append <<EOF\n$(id)\nEOF\n"),
        ),
        (
            "command-unreadable",
            "omhl list \"unclosed",
            run_as("verifier", "omhl list \"unclosed"),
        ),
        (
            "role-unknown",
            "omhl list as janitor",
            run_as("janitor", "omhl list"),
        ),
        (
            "wrapper-unreadable",
            "env -i omhl list",
            run_as("verifier", "env -i omhl list"),
        ),
        (
            "compound-command",
            "omhl list | head",
            run_as("verifier", "omhl list | head"),
        ),
        ("command-outside-matrix", ATTEST, run_as("verifier", ATTEST)),
        (
            "role-prefix-unreachable",
            "! omhl list",
            run_as("verifier", "! omhl list"),
        ),
        (
            "role-matrix-unreadable",
            "omhl list without agent file",
            unreadable_matrix,
        ),
    ];

    for (code, context, output) in &denials {
        assert_reason_starts_with_code(output, code, context);
    }
}

// Nada além de espaço e tab antes do primeiro comando

#[test]
fn it_should_deny_a_comment_line_before_the_omhl_call() {
    let command = "# record the decision\nomhl append <<'EOF'\n{\"items\":[]}\nEOF\n";

    assert_denied_with_code(command, "role-prefix-unreachable");
}

#[test]
fn it_should_deny_a_blank_line_before_the_omhl_call() {
    assert_denied_with_code("\nomhl list", "role-prefix-unreachable");
}

#[test]
fn it_should_deny_a_redirection_only_line_before_the_omhl_call() {
    assert_denied_with_code(">out\nomhl list", "role-prefix-unreachable");
}

#[test]
fn it_should_deny_a_heredoc_line_before_the_omhl_call() {
    assert_denied_with_code("<<X\nbody\nX\nomhl list", "role-prefix-unreachable");
}

#[test]
fn it_should_deny_a_line_continuation_before_the_omhl_call() {
    assert_denied_with_code("\\\nomhl list", "role-prefix-unreachable");
}

#[test]
fn it_should_allow_leading_spaces_and_tabs_before_the_omhl_call() {
    assert_verifier_allowed("  \t omhl list");
}

#[test]
fn it_should_not_hide_the_matrix_denial_behind_the_start_rule() {
    let command = format!("# why\n{ATTEST}");

    let output = run_as("verifier", &command);

    assert_denied_listing(&output, "verifier", &command);
}

// `env` que tira o papel

#[test]
fn it_should_deny_env_u_omhl_role() {
    assert_denied_from_any_origin_with_code("env -u OMHL_ROLE omhl list", "role-unset");
}

#[test]
fn it_should_deny_env_with_u_glued_to_omhl_role() {
    assert_denied_from_any_origin_with_code("env -uOMHL_ROLE omhl list", "role-unset");
}

#[test]
fn it_should_deny_env_unset_equals_omhl_role() {
    assert_denied_from_any_origin_with_code("env --unset=OMHL_ROLE omhl list", "role-unset");
}

#[test]
fn it_should_deny_env_unset_separated_omhl_role() {
    assert_denied_from_any_origin_with_code("env --unset OMHL_ROLE omhl list", "role-unset");
}

// `OMHL_ROLE+=`

#[test]
fn it_should_deny_omhl_role_plus_equals_as_a_command_prefix() {
    assert_denied_from_any_origin("OMHL_ROLE+=omhl:verifier omhl list");
}

#[test]
fn it_should_deny_env_omhl_role_plus_equals() {
    assert_denied_from_any_origin("env OMHL_ROLE+=omhl:verifier omhl list");
}

#[test]
fn it_should_deny_export_omhl_role_plus_equals() {
    assert_denied_from_any_origin("export OMHL_ROLE+=omhl:verifier");
}

// Palavras reservadas que o prefixo não alcança

#[test]
fn it_should_deny_a_negated_omhl_call() {
    assert_denied_with_code("! omhl list", "role-prefix-unreachable");
}

#[test]
fn it_should_deny_omhl_behind_every_control_keyword() {
    for keyword in ["if", "then", "elif", "else", "while", "until", "do"] {
        assert_denied_with_code(&format!("{keyword} omhl list"), "role-prefix-unreachable");
    }
}

#[test]
fn it_should_put_the_prefix_after_time_and_its_p_option() {
    let output = run_as("verifier", "time -p omhl list");

    assert_eq!(
        decision_of(&output)["updatedInput"]["command"],
        json!("time -p OMHL_ROLE=omhl:verifier omhl list")
    );
}

#[test]
fn it_should_put_the_prefix_after_time_when_the_call_has_leading_spaces() {
    let output = run_as("verifier", "  time omhl list");

    assert_eq!(
        decision_of(&output)["updatedInput"]["command"],
        json!("  time OMHL_ROLE=omhl:verifier omhl list")
    );
}

// `${...}` e `$((...))` são parte da palavra

#[test]
fn it_should_allow_omhl_with_a_braced_parameter_inside_double_quotes() {
    assert_verifier_allowed("omhl list --db \"${X}/state.db\"");
}

#[test]
fn it_should_allow_omhl_reached_through_a_braced_parameter_path() {
    assert_verifier_allowed("${HOME}/.local/bin/omhl list");
}

#[test]
fn it_should_allow_omhl_with_an_arithmetic_expansion_argument() {
    assert_verifier_allowed("omhl list --limit $((1+2))");
}

#[test]
fn it_should_allow_omhl_with_a_parenthesised_arithmetic_expansion() {
    assert_verifier_allowed("omhl list --limit $(( (1+2)*3 ))");
}

#[test]
fn it_should_not_read_a_hash_inside_a_braced_parameter_as_a_comment() {
    let output = run_as("verifier", "echo ${#x}; omhl list");

    assert_denied_mentioning(&output, "only command");
}

#[test]
fn it_should_not_read_a_hash_right_after_a_substitution_as_a_comment() {
    let output = run_as("verifier", "echo $(true)#; omhl list");

    assert_denied_mentioning(&output, "only command");
}

// Heredoc sem aspas no terminador executa `$(...)` e crase

#[test]
fn it_should_deny_an_unquoted_heredoc_with_a_command_substitution_in_the_body() {
    let command = "omhl append <<EOF\n{\"text\":\"on $(date)\"}\nEOF\n";

    let output = run_as("verifier", command);

    assert_denied_mentioning(&output, "<<'EOF'");
}

#[test]
fn it_should_deny_an_unquoted_heredoc_with_a_backtick_in_the_body() {
    let command = "omhl append <<EOF\n{\"text\":\"use `omhl list` to see\"}\nEOF\n";

    let output = run_as("verifier", command);

    assert_denied_mentioning(&output, "<<'EOF'");
}

#[test]
fn it_should_deny_a_dash_heredoc_with_a_substitution_in_the_body() {
    let command = "omhl append <<-EOF\n\t$(date)\n\tEOF\n";

    let output = run_as("verifier", command);

    assert_denied_mentioning(&output, "<<'EOF'");
}

#[test]
fn it_should_deny_an_unquoted_heredoc_that_hides_an_omhl_substitution_in_another_command() {
    let output = run_as("architect", "cat <<EOF\n$(omhl list)\nEOF\n");

    assert_denied_mentioning(&output, "<<'EOF'");
}

#[test]
fn it_should_allow_an_unquoted_heredoc_without_substitution() {
    assert_verifier_allowed("omhl append <<EOF\n{\"items\":[]}\nEOF\n");
}

#[test]
fn it_should_allow_a_quoted_heredoc_with_a_substitution_in_the_body() {
    assert_verifier_allowed("omhl append <<'EOF'\n{\"text\":\"$(date) `x`\"}\nEOF\n");
}

#[test]
fn it_should_pass_an_unquoted_heredoc_with_a_substitution_from_the_main_session() {
    let output = run_agent_role("cat <<EOF\n$(date)\nEOF\n", None);

    assert_passed(&output);
}

// `command -v` consulta, não executa

#[test]
fn it_should_pass_command_v_omhl() {
    let output = run_as("verifier", "command -v omhl");

    assert_passed(&output);
}

#[test]
fn it_should_pass_command_capital_v_omhl() {
    let output = run_as("verifier", "command -V omhl");

    assert_passed(&output);
}

// `$'omhl'` e `$"omhl"` são o programa `omhl`

#[test]
fn it_should_read_an_ansi_c_quoted_program_as_omhl() {
    assert_verifier_allowed("$'omhl' list");
}

#[test]
fn it_should_read_a_locale_quoted_program_as_omhl() {
    assert_verifier_allowed("$\"omhl\" list");
}

#[test]
fn it_should_apply_the_matrix_to_an_ansi_c_quoted_program() {
    let command = format!("$'omhl' {}", &ATTEST["omhl ".len()..]);

    let output = run_as("verifier", &command);

    assert_denied_listing(&output, "verifier", &command);
}

// O texto de `${...}` e `$((...))` fica na palavra, então o hook vê o que o bash vai expandir

#[test]
fn it_should_deny_a_parameter_expansion_as_the_leaf_of_an_omhl_command() {
    let command = "omhl ${X:-attest} list";

    let output = run_as("verifier", command);

    assert_denied_listing(&output, "verifier", command);
}

#[test]
fn it_should_deny_an_arithmetic_expansion_as_the_leaf_of_an_omhl_command() {
    let command = "omhl $((0)) list";

    let output = run_as("verifier", command);

    assert_denied_listing(&output, "verifier", command);
}

#[test]
fn it_should_deny_an_expansion_in_the_option_position_of_a_wrapper_before_omhl() {
    let command = "env ${X:--i} omhl append <<'EOF'\n{\"items\":[]}\nEOF\n";

    let output = run_as("verifier", command);

    assert_denied_mentioning(&output, "wrapper option");
}

#[test]
fn it_should_deny_an_expansion_in_the_program_position_when_a_word_mentions_omhl() {
    for command in [
        "$X omhl list",
        "\"${X}\" omhl list",
        "$((0)) omhl list",
        "omhl$X list",
        "nice $N omhl list",
    ] {
        let output = run_as("verifier", command);

        assert_denied_mentioning(&output, "wrapper option");
    }
}

const HIDDEN_OMHL_COMMANDS: [&str; 7] = [
    "$(command -v omhl) append <<'EOF'\n{\"items\":[]}\nEOF\n",
    "\"$(which omhl)\" list",
    "`which omhl` list",
    "env -i $(which omhl) list",
    "timeout $(echo 5) omhl list",
    "P=$(command -v omhl); $P list",
    "X=omhl; $X list",
];

#[test]
fn it_should_deny_omhl_hidden_behind_a_substitution_or_a_variable_in_the_program_position() {
    for command in HIDDEN_OMHL_COMMANDS {
        let output = run_as("verifier", command);

        assert_denied_mentioning(&output, "only command of the call");
    }
}

#[test]
fn it_should_pass_omhl_hidden_behind_a_substitution_or_a_variable_from_the_main_session() {
    for command in HIDDEN_OMHL_COMMANDS {
        let output = run_agent_role(command, None);

        assert_passed(&output);
    }
}

#[test]
fn it_should_pass_an_expansion_in_the_program_position_when_no_word_mentions_omhl() {
    for command in ["$EDITOR notes.md", "echo $(date)"] {
        let output = run_as("verifier", command);

        assert_passed(&output);
    }
}

#[test]
fn it_should_pass_a_quoted_heredoc_that_holds_an_omhl_substitution_inside_a_substitution() {
    let output = run_as("verifier", "echo \"$(cat <<'EOF'\n$(omhl list)\nEOF\n)\"");

    assert_passed(&output);
}

#[test]
fn it_should_pass_an_expansion_in_the_program_position_from_the_main_session() {
    let output = run_agent_role("$X omhl list", None);

    assert_passed(&output);
}

#[test]
fn it_should_let_a_redirection_target_consume_a_parameter_expansion() {
    // Consumido o alvo, a folha é `report answers` (liberada); se ele virasse argumento, seria `report ${X:-/dev/null}` (negada).
    assert_for_every_role("report answers", "omhl report >${X:-/dev/null} answers");
}

#[test]
fn it_should_allow_an_expansion_inside_a_later_argument_value() {
    for command in [
        "omhl --db \"${X}\" list",
        "omhl list --where a=$B",
        "omhl list $((1 + 1))",
        "timeout ${T} omhl list",
        "FOO=${X} omhl list",
    ] {
        assert_verifier_allowed(command);
    }
}

// Limites do leitor: profundidade e tamanho viram erro de leitura, nunca estouro de pilha

fn deeply_nested(opening: &str, closing: &str) -> String {
    format!(
        "omhl list {}{}",
        opening.repeat(10_000),
        closing.repeat(10_000)
    )
}

#[test]
fn it_should_deny_a_deeply_nested_command_with_omhl_from_a_ported_agent() {
    for (opening, closing) in [("$(", ")"), ("${a:-", "}"), ("(", ")")] {
        let output = run_as("verifier", &deeply_nested(opening, closing));

        assert_denied_mentioning(&output, "too deeply");
    }
}

#[test]
fn it_should_pass_a_deeply_nested_command_with_omhl_from_the_main_session() {
    let output = run_agent_role(&deeply_nested("$(", ")"), None);

    assert_passed(&output);
}

#[test]
fn it_should_pass_a_deeply_nested_command_without_omhl_from_a_ported_agent() {
    let command = deeply_nested("$(", ")").replace("omhl", "echo");

    let output = run_as("verifier", &command);

    assert_passed(&output);
}

#[test]
fn it_should_deny_a_command_over_the_length_limit_with_omhl_from_a_ported_agent() {
    let command = format!("omhl list # {}", "a".repeat(300 * 1024));

    let output = run_as("verifier", &command);

    assert_denied_mentioning(&output, "too long");
}

#[test]
fn it_should_pass_a_command_over_the_length_limit_from_the_main_session() {
    let command = format!("omhl list # {}", "a".repeat(300 * 1024));

    let output = run_agent_role(&command, None);

    assert_passed(&output);
}

// `time [-p] --` leva o prefixo para depois do `--`

#[test]
fn it_should_put_the_prefix_after_the_double_dash_of_time() {
    for (command, rewritten) in [
        (
            "time -- omhl list",
            "time -- OMHL_ROLE=omhl:verifier omhl list",
        ),
        (
            "time -p -- omhl list",
            "time -p -- OMHL_ROLE=omhl:verifier omhl list",
        ),
    ] {
        let output = run_as("verifier", command);

        assert_eq!(
            decision_of(&output)["updatedInput"]["command"],
            json!(rewritten)
        );
    }
}

// Execução: o comando reescrito roda de verdade com `OMHL_ROLE` no `omhl`

/// Um embrulho de mentira: pula as opções (`-oL`, `-n 5`) e o `proxy` do `rtk` e executa o resto.
const WRAPPER_FAKE: &str = "#!/bin/sh\nwhile [ \"${1#-}\" != \"$1\" ]; do\n  case $1 in -n) shift ;; esac\n  shift\ndone\n[ \"$1\" = proxy ] && shift\nexec \"$@\"\n";

fn install_script(dir: &Path, name: &str, body: &str) {
    let script = dir.join(name);
    fs::write(&script, body).expect("script is written");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("script is executable");
}

fn is_on_path(program: &str) -> bool {
    let path = std::env::var("PATH").expect("PATH is set");

    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

/// O Bash da ferramenta roda no shell do usuário (bash ou zsh); `-f` poupa o zsh do `~/.zshenv` de quem roda o teste.
fn tool_shells() -> Vec<(&'static str, &'static [&'static str])> {
    let mut shells: Vec<(&str, &[&str])> = vec![("bash", &["-c"])];
    if is_on_path("zsh") {
        shells.push(("zsh", &["-f", "-c"]));
    } else {
        assert!(
            std::env::var_os("CI").is_none(),
            "zsh must be installed in CI: the tool shell may be zsh"
        );
    }

    shells
}

const BASH_ONLY: [&str; 3] = [
    "time -p omhl list",
    "time -- omhl list",
    "time -p -- omhl list",
];

#[test]
fn it_should_run_every_allowed_rewritten_command_with_omhl_role_in_the_omhl_environment() {
    let bin = TempDir::new().expect("bin directory is created");
    install_script(
        bin.path(),
        "omhl",
        "#!/bin/sh\nprintf '%s\\n' \"$OMHL_ROLE\"\n",
    );
    // O `rtk` real fica de fora; os outros embrulhos só ganham um falso se a máquina não os tem.
    install_script(bin.path(), "rtk", WRAPPER_FAKE);
    for wrapper in ["nice", "nohup", "stdbuf", "xargs"] {
        if !is_on_path(wrapper) {
            install_script(bin.path(), wrapper, WRAPPER_FAKE);
        }
    }
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").expect("PATH is set")
    );

    let shells = tool_shells();

    for command in [
        "omhl list",
        APPEND_HEREDOC,
        "timeout 30 omhl list",
        "env omhl list",
        "time omhl list",
        "time -p omhl list",
        "time -- omhl list",
        "time -p -- omhl list",
        "exec omhl list",
        "command omhl list",
        "nice -n 5 omhl list",
        "nohup omhl list",
        "stdbuf -oL omhl list",
        "xargs omhl list",
        "rtk proxy omhl list",
        "  omhl list",
        "omhl list 2>&1",
    ] {
        let rewritten = decision_of(&run_as("verifier", command))["updatedInput"]["command"]
            .as_str()
            .expect("the hook allowed and rewrote the command")
            .to_owned();

        for (shell, flags) in &shells {
            // O `time` do zsh é palavra reservada sem opções: `time -p` só existe em bash.
            if *shell == "zsh" && BASH_ONLY.contains(&command) {
                continue;
            }
            let ran = Command::new(shell)
                .args(*flags)
                .arg(&rewritten)
                .env("PATH", &path)
                .env_remove("OMHL_ROLE")
                .env_remove("BASH_ENV")
                .env_remove("ENV")
                .output()
                .expect("the shell runs the rewritten command");

            assert_eq!(
                String::from_utf8_lossy(&ran.stdout).trim(),
                "omhl:verifier",
                "{shell}: {command:?} became {rewritten:?}"
            );
        }
    }
}

// `OMHL_ROLE` no bash e no zsh: o que o hook nega e o que ele libera

/// Cada tentativa de mudar o papel que o hook deve negar, com o código da negação.
const ROLE_BYPASS_ATTEMPTS: [(&str, &str); 14] = [
    ("export -p OMHL_ROLE=x", "role-assignment"),
    ("export -p OMHL_ROLE", "role-assignment"),
    ("readonly -p OMHL_ROLE", "role-assignment"),
    ("readonly -p OMHL_ROLE=x", "role-assignment"),
    ("declare -x OMHL_ROLE=x", "role-assignment"),
    ("declare OMHL_ROLE=x", "role-assignment"),
    ("declare -p OMHL_ROLE=x", "role-assignment"),
    ("declare -- -p OMHL_ROLE=x", "role-assignment"),
    ("declare -- -p OMHL_ROLE", "role-assignment"),
    ("env -iu OMHL_ROLE omhl list", "role-unset"),
    ("env -iuOMHL_ROLE omhl list", "role-unset"),
    ("env -i -u OMHL_ROLE omhl list", "role-unset"),
    ("env --uns=OMHL_ROLE omhl list", "role-unset"),
    ("OMHL_ROLE=x omhl list", "role-assignment"),
];

/// Citam o nome nu de `OMHL_ROLE` sem mudar o papel.
const RELEASED_ROLE_MENTIONS: [&str; 3] = [
    "env -i grep OMHL_ROLE file",
    "declare -p OMHL_ROLE",
    "typeset -p OMHL_ROLE",
];

/// O papel que o próprio shell enxerga depois de rodar o comando, com `omhl:verifier` no ambiente de partida.
fn role_after_running_in(shell: &str, flags: &[&str], command: &str) -> String {
    let probe = format!("{command}\nprintf 'role=%s\\n' \"${{OMHL_ROLE-unset}}\"");
    let ran = Command::new(shell)
        .args(flags)
        .arg(&probe)
        .env("OMHL_ROLE", "omhl:verifier")
        .env_remove("BASH_ENV")
        .env_remove("ENV")
        .output()
        .expect("the shell runs the command");

    String::from_utf8_lossy(&ran.stdout)
        .lines()
        .last()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn it_should_deny_every_attempt_to_change_the_role_from_any_origin() {
    for (command, code) in ROLE_BYPASS_ATTEMPTS {
        assert_denied_from_any_origin_with_code(command, code);
    }
}

#[test]
fn it_should_release_a_bare_role_name_that_only_a_search_or_a_print_reads() {
    for command in RELEASED_ROLE_MENTIONS {
        assert_passed(&run_agent_role(command, None));
        assert_passed(&run_agent_role(command, Some(PORTED_AGENT)));
    }
}

#[test]
fn it_should_keep_the_role_in_bash_and_zsh_after_a_released_bare_role_name() {
    for command in RELEASED_ROLE_MENTIONS {
        for (shell, flags) in tool_shells() {
            let role = role_after_running_in(shell, flags, command);

            assert_eq!(role, "role=omhl:verifier", "{shell}: {command}");
        }
    }
}
