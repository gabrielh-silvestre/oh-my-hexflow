use crate::support;
use serde_json::json;
use support::child::{Output, omhl};
use support::cycle::WORK_ITEM;
use support::project::Project;
use tempfile::TempDir;

/// Os 23 comandos ativos (padrão `agent`): o caminho na CLI e uma chamada válida de cada um.
const ACTIVE: [(&str, &[&str]); 23] = [
    ("list", &["list"]),
    ("get", &["get", "work-items.payroll"]),
    ("append", &["append"]),
    (
        "attest",
        &[
            "attest",
            "checks.run-tests",
            WORK_ITEM,
            "--outcome",
            "passed",
        ],
    ),
    ("render", &["render"]),
    ("gate-pr", &["gate-pr", WORK_ITEM]),
    ("close", &["close", WORK_ITEM]),
    ("verify", &["verify"]),
    ("load", &["load"]),
    ("project", &["project"]),
    ("check run", &["check", "run", "--event", "before-pr"]),
    ("check sweep", &["check", "sweep"]),
    ("check retire", &["check", "retire", "checks.ledger-audit"]),
    (
        "guideline retire",
        &["guideline", "retire", "guidelines.brief-tone"],
    ),
    (
        "byproduct",
        &["byproduct", "checks.payroll-lines", WORK_ITEM],
    ),
    ("byproduct prune", &["byproduct", "prune"]),
    ("hooks install", &["hooks", "install"]),
    ("spec init", &["spec", "init", WORK_ITEM, "export-reports"]),
    ("spec validate", &["spec", "validate", WORK_ITEM]),
    ("report answers", &["report", "answers"]),
    ("report recurrences", &["report", "recurrences"]),
    ("report retro", &["report", "retro", WORK_ITEM]),
    ("report sessions", &["report", "sessions"]),
];

/// Os 6 comandos adiados (padrão `json`, só JSON).
const DEFERRED: [(&str, &[&str]); 6] = [
    ("hook session-start", &["hook", "session-start"]),
    ("hook spec-lock", &["hook", "spec-lock"]),
    ("hook agent-role", &["hook", "agent-role"]),
    ("hooks post-commit", &["hooks", "post-commit"]),
    (
        "gate-worktree",
        &["gate-worktree", "--branch", "feat/wi-payroll__x"],
    ),
    (
        "hook wt-post-start",
        &["hook", "wt-post-start", "--branch", "feat/wi-x__y"],
    ),
];

const INPUT_ERROR: &str = "{\"error\":{\"code\":\"input-invalid-json\",\"details\":[],\"message\":\"input is not valid JSON at line 1 column 0\"}}\n";

const NO_MARKER_ERROR: &str = "{\"error\":{\"code\":\"no-marker\",\"details\":[],\"message\":\"no .omhl.toml found in the project directory or any parent; a branch older than the marker needs a rebase onto main\"}}\n";

// ---------------------------------------------------------------- ajudantes

/// Uma pasta sem marcador e uma de dados: os comandos que precisam de projeto saem 1 com `no-marker`, sem tocar em nada.
struct Bare {
    dir: TempDir,
    xdg: TempDir,
}

impl Bare {
    fn new() -> Bare {
        Bare {
            dir: TempDir::new().expect("bare directory is created"),
            xdg: TempDir::new().expect("data directory is created"),
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        omhl(args).cwd(self.dir.path()).xdg(self.xdg.path()).run()
    }
}

/// O que um comando devolveu, para comparar duas chamadas.
fn outcome(output: &Output) -> (Option<i32>, &str, &str) {
    (output.code, &output.stdout, &output.stderr)
}

fn format_before<'a>(args: &[&'a str], format: &'a str) -> Vec<&'a str> {
    let mut full = vec!["--format", format];
    full.extend_from_slice(args);

    full
}

fn format_after<'a>(args: &[&'a str], format: &'a str) -> Vec<&'a str> {
    let mut full = args.to_vec();
    full.extend(["--format", format]);

    full
}

fn help_of(path: &str) -> String {
    let mut args: Vec<&str> = path.split(' ').collect();
    args.push("--help");

    let output = omhl(&args).run();

    assert_eq!(output.code, Some(0), "{path}: stderr: {}", output.stderr);
    output.stdout
}

/// A lista de caminhos que o `omhl --help` mostra sob `  <formato>: `.
fn listed_under(root_help: &str, format: &str) -> Vec<String> {
    let prefix = format!("  {format}: ");
    let mut listed: Vec<String> = root_help
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .map(|rest| rest.split(", ").map(str::to_owned).collect())
        .unwrap_or_default();
    listed.sort();

    listed
}

fn sorted_paths(table: &[(&str, &[&str])]) -> Vec<String> {
    let mut paths: Vec<String> = table.iter().map(|(path, _)| (*path).to_owned()).collect();
    paths.sort();

    paths
}

// ---------------------------------------------------------------- `--format` em cada comando ativo (CA-8)

#[test]
fn it_should_accept_agent_human_and_json_before_the_subcommand_of_each_active_command() {
    let bare = Bare::new();

    let refused: Vec<String> = ACTIVE
        .iter()
        .flat_map(|(path, args)| ["agent", "human", "json"].map(|format| (*path, *args, format)))
        .filter(|(_, args, format)| bare.run(&format_before(args, format)).code == Some(2))
        .map(|(path, _, format)| format!("{path} --format {format}"))
        .collect();

    assert_eq!(refused, Vec::<String>::new());
}

#[test]
fn it_should_accept_agent_human_and_json_after_the_subcommand_of_each_active_command() {
    let bare = Bare::new();

    let refused: Vec<String> = ACTIVE
        .iter()
        .flat_map(|(path, args)| ["agent", "human", "json"].map(|format| (*path, *args, format)))
        .filter(|(_, args, format)| bare.run(&format_after(args, format)).code == Some(2))
        .map(|(path, _, format)| format!("{path} --format {format}"))
        .collect();

    assert_eq!(refused, Vec::<String>::new());
}

#[test]
fn it_should_exit_two_for_an_unknown_format_before_the_subcommand_of_each_active_command() {
    let bare = Bare::new();

    let accepted: Vec<&str> = ACTIVE
        .iter()
        .filter(|(_, args)| {
            let output = bare.run(&format_before(args, "xml"));

            output.code != Some(2) || !output.stdout.is_empty()
        })
        .map(|(path, _)| *path)
        .collect();

    assert_eq!(accepted, Vec::<&str>::new());
}

#[test]
fn it_should_exit_two_for_an_unknown_format_after_the_subcommand_of_each_active_command() {
    let bare = Bare::new();

    let accepted: Vec<&str> = ACTIVE
        .iter()
        .filter(|(_, args)| {
            let output = bare.run(&format_after(args, "xml"));

            output.code != Some(2) || !output.stdout.is_empty()
        })
        .map(|(path, _)| *path)
        .collect();

    assert_eq!(accepted, Vec::<&str>::new());
}

#[test]
fn it_should_let_the_format_after_the_subcommand_win_over_the_one_before_in_each_active_command() {
    let bare = Bare::new();

    let differing: Vec<&str> = ACTIVE
        .iter()
        .filter(|(_, args)| {
            let mut both = format_before(args, "json");
            both.extend(["--format", "agent"]);
            let only_after = bare.run(&format_after(args, "agent"));

            outcome(&bare.run(&both)) != outcome(&only_after)
        })
        .map(|(path, _)| *path)
        .collect();

    assert_eq!(differing, Vec::<&str>::new());
}

#[test]
fn it_should_print_json_when_the_format_after_the_subcommand_is_json_and_the_one_before_is_agent() {
    let bare = Bare::new();

    let differing: Vec<&str> = ACTIVE
        .iter()
        .filter(|(_, args)| {
            let mut both = format_before(args, "agent");
            both.extend(["--format", "json"]);
            let only_after = bare.run(&format_after(args, "json"));

            outcome(&bare.run(&both)) != outcome(&only_after)
        })
        .map(|(path, _)| *path)
        .collect();

    assert_eq!(differing, Vec::<&str>::new());
}

#[test]
fn it_should_exit_two_for_format_text_on_check_run() {
    let bare = Bare::new();

    let output = bare.run(&["check", "run", "--event", "before-pr", "--format", "text"]);

    assert_eq!(output.code, Some(2), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");
}

// ---------------------------------------------------------------- adiados com `--format json` (CA-3)

/// Os dois testes de um adiado: sem a flag e com `--format json` a saída é a mesma, e é o JSON de hoje.
macro_rules! deferred_json {
    ($command:ident, $index:expr, $code:expr, $stdout:expr, $stderr:expr) => {
        mod $command {
            use super::*;

            #[test]
            fn it_should_print_the_same_with_and_without_format_json() {
                let bare = Bare::new();
                let args = DEFERRED[$index].1;

                let default = bare.run(args);
                let explicit = bare.run(&format_after(args, "json"));

                assert_eq!(outcome(&default), outcome(&explicit));
            }

            #[test]
            fn it_should_print_the_json_it_printed_before_the_format_flag_existed() {
                let bare = Bare::new();

                let output = bare.run(&format_after(DEFERRED[$index].1, "json"));

                assert_eq!(outcome(&output), (Some($code), $stdout, $stderr));
            }
        }
    };
}

deferred_json!(hook_session_start, 0, 1, "", INPUT_ERROR);

deferred_json!(hook_spec_lock, 1, 0, "", INPUT_ERROR);

deferred_json!(hook_agent_role, 2, 0, "", INPUT_ERROR);

deferred_json!(hooks_post_commit, 3, 1, "", NO_MARKER_ERROR);

deferred_json!(gate_worktree, 4, 0, "", "");

deferred_json!(hook_wt_post_start, 5, 0, "", "");

#[test]
fn it_should_print_the_session_context_with_format_json_as_without_the_flag() {
    let project = Project::new("acme");
    let stdin = json!({ "cwd": project.dir() }).to_string();
    let hook = |args: &[&str]| {
        omhl(args)
            .cwd(project.dir())
            .xdg(project.xdg())
            .stdin(stdin.clone())
            .run()
    };

    let default = hook(&["hook", "session-start"]);
    let explicit = hook(&["hook", "session-start", "--format", "json"]);

    assert_eq!(default.code, Some(0), "stderr: {}", default.stderr);
    assert_eq!(outcome(&default), outcome(&explicit));
}

// ---------------------------------------------------------------- help (CA-8)

#[test]
fn it_should_list_the_twenty_three_agent_commands_in_the_root_help() {
    let help = omhl(&["--help"]).run().stdout;

    assert_eq!(listed_under(&help, "agent"), sorted_paths(&ACTIVE));
}

#[test]
fn it_should_list_the_six_json_commands_in_the_root_help() {
    let help = omhl(&["--help"]).run().stdout;

    assert_eq!(listed_under(&help, "json"), sorted_paths(&DEFERRED));
}

#[test]
fn it_should_cite_the_agent_default_in_the_help_of_each_active_command() {
    let missing: Vec<&str> = ACTIVE
        .iter()
        .filter(|(path, _)| !help_of(path).contains("Default output format: agent."))
        .map(|(path, _)| *path)
        .collect();

    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn it_should_cite_the_json_default_in_the_help_of_each_deferred_command() {
    let missing: Vec<&str> = DEFERRED
        .iter()
        .filter(|(path, _)| !help_of(path).contains("Default output format: json."))
        .map(|(path, _)| *path)
        .collect();

    assert_eq!(missing, Vec::<&str>::new());
}
