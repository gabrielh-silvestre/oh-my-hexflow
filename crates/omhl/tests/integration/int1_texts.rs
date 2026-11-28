//! Os textos do plugin, os docs e o inventário do Worktrunk da parte 3 do `int1` (CA-045, CA-051,
//! CA-054 a CA-056, CA-062). Cada teste procura a frase nova numa linha, não o arquivo inteiro.

use crate::support;

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use support::repo_checks::repo_root;

const FLOW_SKILL: &str = "plugin/skills/directives-flow/SKILL.md";
const GATE_WORKTREE: &str = "plugin/skills/directives-flow/references/gate-worktree.md";
const REVIEW_SKILL: &str = "plugin/skills/review/SKILL.md";
const CLOSING: &str = "plugin/skills/review/references/fechamento.md";
const MERGE: &str = "plugin/skills/review/references/merge.md";
const HOOKS_INSTALL: &str = "plugin/skills/review/references/hooks-install.md";
const CHECK_RUN: &str = "plugin/skills/review/references/check-run.md";
const COMMIT_MARK: &str = "plugin/commands/omhl-commit-mark.md";

fn read(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

/// As páginas do fechamento juntas: a skill `review`, o `fechamento.md` e o `merge.md`.
fn closing_text() -> String {
    [REVIEW_SKILL, CLOSING, MERGE].map(read).join("\n")
}

/// Uma linha que contém todas as agulhas, sem olhar a caixa.
fn has_line_with(text: &str, needles: &[&str]) -> bool {
    text.lines().any(|line| {
        let line = line.to_lowercase();
        needles
            .iter()
            .all(|needle| line.contains(&needle.to_lowercase()))
    })
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).expect("the folder is read") {
        let path = entry.expect("the entry is read").path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }

    found
}

/// `arquivo:linha` de cada linha dos arquivos de `roots` que casa `pattern`.
fn lines_matching(roots: &[&str], pattern: &str) -> Vec<String> {
    let root = repo_root();
    let pattern = Regex::new(pattern).expect("the pattern compiles");
    roots
        .iter()
        .map(|relative| root.join(relative))
        .flat_map(|path| {
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .flat_map(|path| {
            let shown = path
                .strip_prefix(&root)
                .expect("under the root")
                .display()
                .to_string();
            fs::read_to_string(&path)
                .unwrap_or_default()
                .lines()
                .filter(|line| pattern.is_match(line))
                .map(|line| format!("{shown}: {line}"))
                .collect::<Vec<_>>()
        })
        .collect()
}

// CA-045 e CA-051: os ignorados e o limite da branch (CMP-035, CMP-038)

#[test]
fn it_should_tell_the_agent_to_write_ignored_files_only_in_the_main_checkout() {
    let text = closing_text();

    assert!(has_line_with(
        &text,
        &["`.ignore/`", "`.omhl/notes/`", "`.env`", "main checkout"]
    ));
}

#[test]
fn it_should_say_the_worktree_removal_deletes_ignored_files_without_asking() {
    let text = closing_text();

    assert!(has_line_with(
        &text,
        &["ignored", "git worktree remove", "without asking"]
    ));
}

#[test]
fn it_should_state_the_branch_left_after_a_squash_or_rebase_merge() {
    let text = closing_text();

    assert!(has_line_with(
        &text,
        &["`--squash`", "`--rebase`", "`git branch -d`"]
    ));
}

// CA-054: nem o binário nem o plugin nomeiam o Worktrunk; os docs falam do comando novo

#[test]
fn it_should_not_name_worktrunk_in_the_binary_sources_or_the_plugin() {
    let found = lines_matching(
        &["crates/omhl/src", "plugin"],
        r"(?i)worktrunk|wt switch|wt config|\bwt\b",
    );

    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn it_should_not_name_worktrunk_or_the_old_hook_in_the_docs_outside_the_adrs() {
    let readme_sentence = Regex::new("não depende").expect("valid");
    // montado em partes para este arquivo não casar a busca do nome antigo em `branch_created.rs`
    let pattern = [
        "(?i)worktrunk|wt switch|wt config",
        "|wt-post",
        "-start|Wt",
        "PostStart|wt_post",
        "_start",
    ]
    .concat();

    let found: Vec<String> = lines_matching(&["docs", "README.md"], &pattern)
        .into_iter()
        .filter(|line| !line.starts_with("docs/adr/"))
        .filter(|line| !(line.starts_with("README.md") && readme_sentence.is_match(line)))
        .collect();

    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn it_should_carry_the_worktrunk_configuration_of_this_repository_in_contributing() {
    let text = read("CONTRIBUTING.md");

    assert!(text.contains("omhl hook branch-created --branch"));
    assert!(text.contains("wt config approvals add"));
}

#[test]
fn it_should_name_branch_created_and_the_git_commands_in_the_directives_flow_doc() {
    let text = read("docs/directives-flow.md");

    assert!(text.contains("omhl hook branch-created"));
    assert!(text.contains("git worktree add -b"));
    assert!(text.contains("git switch -c"));
}

#[test]
fn it_should_cite_branch_created_in_the_branch_format_unmarked_line_of_the_directives_flow_doc() {
    let text = read("docs/directives-flow.md");

    assert!(has_line_with(
        &text,
        &["branch-format-unmarked", "omhl hook branch-created"]
    ));
}

#[test]
fn it_should_name_branch_created_in_the_mechanical_gates_doc() {
    let text = read("docs/mechanical-gates.md");

    assert!(text.contains("omhl hook branch-created"));
}

#[test]
fn it_should_not_start_wt_from_any_test() {
    let found = lines_matching(&["crates/omhl/tests"], r#"Command::new\("wt"\)"#);

    assert_eq!(found, Vec::<String>::new());
}

// CA-055: criar a branch só com o git (CMP-041)

#[test]
fn it_should_teach_the_worktree_creation_with_git_in_the_directives_flow_skill() {
    let text = read(FLOW_SKILL);

    assert!(has_line_with(
        &text,
        &["git worktree add -b", "worktree.dir"]
    ));
}

#[test]
fn it_should_teach_git_switch_c_as_the_other_way_in_the_directives_flow_skill() {
    let text = read(FLOW_SKILL);

    assert!(text.contains("git switch -c"));
}

#[test]
fn it_should_show_the_default_worktree_path_in_the_directives_flow_skill() {
    let text = read(FLOW_SKILL);

    assert!(text.contains(".worktrees/feat-wi-x__login"));
}

#[test]
fn it_should_say_the_plugin_keeps_the_worktree_folder_ignored() {
    let text = read(FLOW_SKILL);

    assert!(has_line_with(&text, &[".worktrees", "ignored"]));
}

#[test]
fn it_should_say_the_gate_the_mark_and_the_push_warning_see_only_these_commands() {
    let text = read(FLOW_SKILL);

    assert!(has_line_with(
        &text,
        &["creation gate", "birth mark", "push warning", "no other"]
    ));
}

#[test]
fn it_should_teach_the_git_creation_on_the_gate_worktree_page() {
    let text = read(GATE_WORKTREE);

    assert!(text.contains("git worktree add -b"));
}

#[test]
fn it_should_drop_the_cannot_prompt_for_approval_section_from_the_plugin() {
    let found = lines_matching(&["plugin"], "Cannot prompt for approval");

    assert_eq!(found, Vec::<String>::new());
}

// CA-056: o texto do fechamento (CMP-042)

#[test]
fn it_should_tell_the_merge_command_with_its_strategies() {
    let text = closing_text();

    assert!(has_line_with(
        &text,
        &["omhl merge <work-item>", "`--squash`", "`--rebase`"]
    ));
}

#[test]
fn it_should_forbid_merging_through_gh_or_the_mcp_and_say_the_hook_denies_it() {
    let text = closing_text();

    assert!(has_line_with(&text, &["`gh`", "MCP", "hook", "den"]));
}

#[test]
fn it_should_ask_the_owner_before_force_with_the_sentence_in_the_note() {
    let text = closing_text();

    assert!(has_line_with(&text, &["`--force`", "owner", "`--note`"]));
}

#[test]
fn it_should_tell_the_agent_to_run_the_cd_of_the_next_line_after_the_merge() {
    let text = closing_text();

    assert!(has_line_with(&text, &["`cd`", "`Next:`"]));
}

#[test]
fn it_should_close_with_omhl_close_when_the_integration_is_off() {
    let text = closing_text();

    assert!(has_line_with(
        &text,
        &["integration is off", "`omhl close`"]
    ));
}

#[test]
fn it_should_say_the_pull_request_base_is_set_by_instruction_when_the_integration_is_off() {
    let text = closing_text();

    assert!(has_line_with(&text, &["base", "by instruction"]));
}

#[test]
fn it_should_run_hooks_install_only_when_the_person_asks() {
    let text = closing_text();

    assert!(has_line_with(
        &text,
        &["`omhl hooks install`", "only when", "asks"]
    ));
}

#[test]
fn it_should_never_attest_ci_green_by_hand_with_the_integration_on() {
    let text = closing_text();

    assert!(has_line_with(&text, &["never", "`ci-green`", "by hand"]));
}

#[test]
fn it_should_say_merge_ignores_a_previous_ci_attestation() {
    let text = closing_text();

    assert!(has_line_with(
        &text,
        &["`omhl merge`", "ignores", "attestation"]
    ));
}

// CA-062 e EMD-037: as páginas de comando

#[test]
fn it_should_list_the_tidy_step_and_its_stages_on_the_merge_page() {
    let text = read(MERGE);

    let absent: Vec<&str> = [
        "`tidy`",
        "`worktree`",
        "`switch`",
        "`fast-forward`",
        "`branch`",
        "`not-needed`",
        "`skipped`",
    ]
    .into_iter()
    .filter(|anchor| !text.contains(anchor))
    .collect();
    assert_eq!(absent, Vec::<&str>::new());
}

#[test]
fn it_should_cite_branch_created_on_the_gate_worktree_page() {
    let text = read(GATE_WORKTREE);

    assert!(text.contains("branch-created"));
}

#[test]
fn it_should_cite_the_session_start_warning_on_the_hooks_install_page() {
    let text = read(HOOKS_INSTALL);

    assert!(has_line_with(&text, &["session", "warning"]));
}

#[test]
fn it_should_say_hooks_install_writes_five_scripts_with_post_merge() {
    let text = read(HOOKS_INSTALL);

    assert!(has_line_with(&text, &["five", "`post-merge`"]));
}

#[test]
fn it_should_say_on_the_check_run_page_that_a_local_merge_consumes_the_mark_but_squash_does_not() {
    let text = read(CHECK_RUN);

    assert!(has_line_with(
        &text,
        &["`post-merge`", "fast-forward", "`--squash`"]
    ));
}

#[test]
fn it_should_say_on_the_commit_mark_command_that_a_local_merge_consumes_the_mark_but_squash_does_not()
 {
    let text = read(COMMIT_MARK);

    assert!(has_line_with(
        &text,
        &["`post-merge`", "fast-forward", "`--squash`"]
    ));
}
