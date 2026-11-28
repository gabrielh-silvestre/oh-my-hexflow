//! CA-043, CA-044, CA-046 a CA-048: a arrumação do clone sobre um `git` de registro, que anota o
//! diretório de cada chamada e o argv e responde pelo começo do argv.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use super::{StageResult, TidyStage, tidy};
use crate::probe::{GitOutput, ProbeError, TidyGit};

const MAIN: &str = "/repo";
const LINKED: &str = "/repo/.worktrees/feat-wi-w1__x";
const BRANCH: &str = "feat/wi-w1__x";
const INTEGRATION: &str = "main";

/// Um `git` falso: grava `(dir, argv)` e devolve a primeira resposta cujo prefixo casa o argv.
struct RecordingGit {
    sent: Sender<(PathBuf, Vec<String>)>,
    seen: Receiver<(PathBuf, Vec<String>)>,
    answers: Vec<(Vec<&'static str>, GitOutput)>,
}

fn ok(stdout: &str) -> GitOutput {
    GitOutput {
        code: 0,
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

fn failed(stderr: &str) -> GitOutput {
    GitOutput {
        code: 128,
        stdout: String::new(),
        stderr: stderr.to_owned(),
    }
}

impl RecordingGit {
    fn new(porcelain: &str) -> RecordingGit {
        let (sent, seen) = mpsc::channel();
        RecordingGit {
            sent,
            seen,
            answers: vec![(vec!["worktree", "list"], ok(porcelain))],
        }
    }

    fn answer(mut self, prefix: &[&'static str], output: GitOutput) -> RecordingGit {
        self.answers.insert(0, (prefix.to_vec(), output));
        self
    }

    /// Lê o registro e o devolve ao canal, para a leitura seguinte ver as mesmas chamadas.
    fn calls(&self) -> Vec<(PathBuf, Vec<String>)> {
        let calls: Vec<(PathBuf, Vec<String>)> = self.seen.try_iter().collect();
        for call in &calls {
            self.sent.send(call.clone()).expect("the receiver is alive");
        }

        calls
    }

    fn argvs(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .map(|(_, args)| args.join(" "))
            .collect()
    }

    /// A posição da primeira chamada cujo argv começa por `prefix`.
    fn position(&self, prefix: &str) -> Option<usize> {
        self.argvs()
            .iter()
            .position(|argv| argv.starts_with(prefix))
    }
}

impl TidyGit for RecordingGit {
    fn git(&self, dir: &Path, args: &[&str]) -> Result<GitOutput, ProbeError> {
        self.sent
            .send((
                dir.to_path_buf(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
            ))
            .expect("the receiver is alive");
        let answer = self
            .answers
            .iter()
            .find(|(prefix, _)| args.starts_with(prefix))
            .map(|(_, output)| output.clone())
            .unwrap_or_else(|| ok(""));

        Ok(answer)
    }
}

fn entry(path: &str, branch: &str, extra: &str) -> String {
    format!(
        "worktree {path}\nHEAD 1111111111111111111111111111111111111111\nbranch refs/heads/{branch}\n{extra}\n"
    )
}

/// O checkout principal na integração e a worktree do work item ligada.
fn with_linked_worktree() -> String {
    format!(
        "{}{}",
        entry(MAIN, INTEGRATION, ""),
        entry(LINKED, BRANCH, "")
    )
}

/// O trabalho feito no checkout principal, sem worktree.
fn in_main_checkout() -> String {
    entry(MAIN, BRANCH, "")
}

fn run(git: &RecordingGit) -> Vec<(TidyStage, StageResult)> {
    tidy(git, Path::new(MAIN), BRANCH, INTEGRATION, false)
}

fn result_of(stages: &[(TidyStage, StageResult)], stage: TidyStage) -> StageResult {
    stages
        .iter()
        .find(|(name, _)| *name == stage)
        .map(|(_, result)| result.clone())
        .unwrap_or_else(|| panic!("no stage {stage:?} in {stages:?}"))
}

fn skipped_reason(stages: &[(TidyStage, StageResult)], stage: TidyStage) -> String {
    match result_of(stages, stage) {
        StageResult::Skipped(reason) => reason,
        other => panic!("{stage:?} should be skipped, got {other:?}"),
    }
}

fn assert_no_forcing_flag(git: &RecordingGit) {
    for argv in git.argvs() {
        let words: Vec<&str> = argv.split(' ').collect();
        assert!(
            !words
                .iter()
                .any(|word| ["--force", "-f", "-D", "--delete"].contains(word)),
            "forcing flag in: {argv}"
        );
    }
}

// CA-043: a ordem e o diretório de cada chamada

#[test]
fn it_should_report_the_four_stages_in_the_fixed_order() {
    let git = RecordingGit::new(&with_linked_worktree());

    let stages = run(&git);

    let names: Vec<TidyStage> = stages.iter().map(|(stage, _)| *stage).collect();
    assert_eq!(
        names,
        [
            TidyStage::Worktree,
            TidyStage::Switch,
            TidyStage::FastForward,
            TidyStage::Branch
        ]
    );
}

#[test]
fn it_should_spell_each_stage_as_the_output_names_it() {
    let names = [
        TidyStage::Worktree,
        TidyStage::Switch,
        TidyStage::FastForward,
        TidyStage::Branch,
    ]
    .map(TidyStage::as_str);

    assert_eq!(names, ["worktree", "switch", "fast-forward", "branch"]);
}

#[test]
fn it_should_remove_the_worktree_then_fast_forward_then_delete_the_branch() {
    let git = RecordingGit::new(&with_linked_worktree());

    run(&git);

    let positions = [
        git.position("worktree remove"),
        git.position("fetch origin"),
        git.position(&format!("branch -d {BRANCH}")),
    ];
    assert!(positions.iter().all(Option::is_some), "{:?}", git.argvs());
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{:?}",
        git.argvs()
    );
}

#[test]
fn it_should_switch_the_main_checkout_before_the_fast_forward_and_the_branch_deletion() {
    let git = RecordingGit::new(&in_main_checkout());

    run(&git);

    let positions = [
        git.position(&format!("switch {INTEGRATION}")),
        git.position("fetch origin"),
        git.position(&format!("branch -d {BRANCH}")),
    ];
    assert!(positions.iter().all(Option::is_some), "{:?}", git.argvs());
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{:?}",
        git.argvs()
    );
}

#[test]
fn it_should_run_every_git_call_from_the_removal_on_in_the_main_directory() {
    let git = RecordingGit::new(&with_linked_worktree());

    run(&git);

    let calls = git.calls();
    let removal = git
        .position("worktree remove")
        .expect("the worktree is removed");
    let after: Vec<&(PathBuf, Vec<String>)> = calls[removal..].iter().collect();
    assert!(
        after.iter().all(|(dir, _)| dir == Path::new(MAIN)),
        "{after:?}"
    );
}

#[test]
fn it_should_not_name_the_worktree_path_after_the_removal() {
    let git = RecordingGit::new(&with_linked_worktree());

    run(&git);

    let calls = git.calls();
    let removal = git
        .position("worktree remove")
        .expect("the worktree is removed");
    let leaked: Vec<&(PathBuf, Vec<String>)> = calls[removal + 1..]
        .iter()
        .filter(|(dir, args)| {
            dir.starts_with(LINKED) || args.iter().any(|arg| arg.contains(LINKED))
        })
        .collect();
    assert!(leaked.is_empty(), "{leaked:?}");
}

#[test]
fn it_should_remove_the_worktree_by_its_path_without_force() {
    let git = RecordingGit::new(&with_linked_worktree());

    run(&git);

    assert!(
        git.argvs().contains(&format!("worktree remove {LINKED}")),
        "{:?}",
        git.argvs()
    );
}

// CA-044: as variantes da remoção

#[test]
fn it_should_find_a_worktree_outside_the_default_folder_by_its_branch() {
    let outside = "/elsewhere/x";
    let porcelain = format!(
        "{}{}",
        entry(MAIN, INTEGRATION, ""),
        entry(outside, BRANCH, "")
    );
    let git = RecordingGit::new(&porcelain);

    let stages = run(&git);

    assert!(git.argvs().contains(&format!("worktree remove {outside}")));
    assert_eq!(result_of(&stages, TidyStage::Worktree), StageResult::Done);
}

#[test]
fn it_should_skip_the_removal_with_the_blocking_files_when_git_refuses_a_dirty_worktree() {
    let git = RecordingGit::new(&with_linked_worktree())
        .answer(
            &["worktree", "remove"],
            failed("fatal: '/repo/.worktrees/feat-wi-w1__x' contains modified or untracked files, use --force to delete it"),
        )
        .answer(&["status"], ok("?? notes.txt\n"));

    let stages = run(&git);

    let reason = skipped_reason(&stages, TidyStage::Worktree);
    assert!(reason.contains("notes.txt"), "{reason}");
    assert!(reason.contains("ask the user"), "{reason}");
}

#[test]
fn it_should_list_at_most_twenty_status_lines_of_a_dirty_worktree() {
    let status: String = (1..=25).map(|n| format!("?? file-{n:02}.txt\n")).collect();
    let git = RecordingGit::new(&with_linked_worktree())
        .answer(
            &["worktree", "remove"],
            failed("fatal: contains modified or untracked files, use --force to delete it"),
        )
        .answer(&["status"], ok(&status));

    let stages = run(&git);

    let reason = skipped_reason(&stages, TidyStage::Worktree);
    assert!(reason.contains("file-20.txt"), "{reason}");
    assert!(!reason.contains("file-21.txt"), "{reason}");
}

#[test]
fn it_should_skip_the_branch_deletion_when_the_worktree_stays() {
    let git = RecordingGit::new(&with_linked_worktree())
        .answer(
            &["worktree", "remove"],
            failed("fatal: contains modified or untracked files, use --force to delete it"),
        )
        .answer(&["status"], ok(" M a.txt\n"));

    let stages = run(&git);

    assert!(matches!(
        result_of(&stages, TidyStage::Branch),
        StageResult::Skipped(_)
    ));
    assert_eq!(git.position("branch -d"), None, "{:?}", git.argvs());
}

#[test]
fn it_should_skip_a_locked_worktree_with_the_lock_reason() {
    let porcelain = format!(
        "{}{}",
        entry(MAIN, INTEGRATION, ""),
        entry(LINKED, BRANCH, "locked held by the owner\n")
    );
    let git = RecordingGit::new(&porcelain).answer(
        &["worktree", "remove"],
        failed("fatal: cannot remove a locked working tree, lock reason: held by the owner"),
    );

    let stages = run(&git);

    let reason = skipped_reason(&stages, TidyStage::Worktree);
    assert!(reason.contains("held by the owner"), "{reason}");
}

#[test]
fn it_should_prune_an_entry_whose_folder_is_gone() {
    let porcelain = format!(
        "{}{}",
        entry(MAIN, INTEGRATION, ""),
        entry(
            LINKED,
            BRANCH,
            "prunable gitdir file points to non-existent location\n"
        )
    );
    let git = RecordingGit::new(&porcelain);

    run(&git);

    assert!(
        git.position("worktree prune").is_some(),
        "{:?}",
        git.argvs()
    );
    assert_eq!(git.position("worktree remove"), None, "{:?}", git.argvs());
}

#[test]
fn it_should_never_remove_the_main_worktree() {
    let git = RecordingGit::new(&in_main_checkout());

    let stages = run(&git);

    assert_eq!(git.position("worktree remove"), None, "{:?}", git.argvs());
    assert_eq!(
        result_of(&stages, TidyStage::Worktree),
        StageResult::NotNeeded
    );
}

#[test]
fn it_should_pass_no_forcing_flag_to_git_on_the_happy_path() {
    let git = RecordingGit::new(&with_linked_worktree());

    run(&git);

    assert_no_forcing_flag(&git);
}

#[test]
fn it_should_pass_no_forcing_flag_to_git_when_every_step_is_refused() {
    let git = RecordingGit::new(&with_linked_worktree())
        .answer(
            &["worktree", "remove"],
            failed("fatal: contains modified or untracked files, use --force to delete it"),
        )
        .answer(&["status"], ok("?? notes.txt\n"))
        .answer(
            &["merge"],
            failed("fatal: Not possible to fast-forward, aborting."),
        )
        .answer(&["fetch"], failed("fatal: refusing to fetch into branch"))
        .answer(&["branch"], failed("error: the branch is not fully merged"));

    run(&git);

    assert_no_forcing_flag(&git);
}

// CA-046: o avanço rápido

#[test]
fn it_should_fast_forward_with_fetch_head_when_the_main_checkout_is_on_the_integration() {
    let git = RecordingGit::new(&with_linked_worktree());

    let stages = run(&git);

    let fetch = git
        .position(&format!("fetch origin {INTEGRATION}"))
        .expect("the integration is fetched");
    let merge = git
        .position("merge --ff-only FETCH_HEAD")
        .expect("the main checkout fast-forwards");
    assert!(fetch < merge, "{:?}", git.argvs());
    assert_eq!(
        result_of(&stages, TidyStage::FastForward),
        StageResult::Done
    );
}

#[test]
fn it_should_fetch_into_the_local_integration_when_no_checkout_uses_it() {
    let porcelain = format!("{}{}", entry(MAIN, "other", ""), entry(LINKED, BRANCH, ""));
    let git = RecordingGit::new(&porcelain);

    run(&git);

    assert!(
        git.argvs()
            .contains(&format!("fetch origin {INTEGRATION}:{INTEGRATION}")),
        "{:?}",
        git.argvs()
    );
    assert_eq!(git.position("merge"), None, "{:?}", git.argvs());
}

#[test]
fn it_should_skip_the_fast_forward_when_git_refuses_it() {
    let git = RecordingGit::new(&with_linked_worktree()).answer(
        &["merge", "--ff-only"],
        failed("fatal: Not possible to fast-forward, aborting."),
    );

    let stages = run(&git);

    assert!(matches!(
        result_of(&stages, TidyStage::FastForward),
        StageResult::Skipped(_)
    ));
}

#[test]
fn it_should_skip_the_fast_forward_when_git_refuses_to_fetch_into_a_branch_in_use() {
    let porcelain = format!("{}{}", entry(MAIN, "other", ""), entry(LINKED, BRANCH, ""));
    let git = RecordingGit::new(&porcelain).answer(
        &["fetch"],
        failed("fatal: refusing to fetch into branch 'refs/heads/main' checked out at '/int'"),
    );

    let stages = run(&git);

    assert!(matches!(
        result_of(&stages, TidyStage::FastForward),
        StageResult::Skipped(_)
    ));
}

// CA-047: a troca do checkout principal

#[test]
fn it_should_not_switch_the_main_checkout_when_a_worktree_existed() {
    let git = RecordingGit::new(&with_linked_worktree());

    let stages = run(&git);

    assert_eq!(git.position("switch"), None, "{:?}", git.argvs());
    assert_eq!(
        result_of(&stages, TidyStage::Switch),
        StageResult::NotNeeded
    );
}

#[test]
fn it_should_skip_the_switch_and_the_branch_deletion_when_the_tree_blocks_the_switch() {
    let git = RecordingGit::new(&in_main_checkout()).answer(
        &["switch"],
        failed("error: Your local changes to the following files would be overwritten by checkout"),
    );

    let stages = run(&git);

    assert!(matches!(
        result_of(&stages, TidyStage::Switch),
        StageResult::Skipped(_)
    ));
    assert!(matches!(
        result_of(&stages, TidyStage::Branch),
        StageResult::Skipped(_)
    ));
}

// CA-048: a exclusão da branch

#[test]
fn it_should_delete_the_branch_with_lowercase_d() {
    let git = RecordingGit::new(&with_linked_worktree());

    let stages = run(&git);

    assert!(git.argvs().contains(&format!("branch -d {BRANCH}")));
    assert_eq!(result_of(&stages, TidyStage::Branch), StageResult::Done);
}

#[test]
fn it_should_skip_the_branch_with_the_squash_and_rebase_limit_when_git_refuses_lowercase_d() {
    let git = RecordingGit::new(&with_linked_worktree()).answer(
        &["branch", "-d"],
        failed("error: the branch 'feat/wi-w1__x' is not fully merged"),
    );

    let stages = run(&git);

    let reason = skipped_reason(&stages, TidyStage::Branch);
    assert!(reason.contains("squash"), "{reason}");
    assert!(reason.contains("rebase"), "{reason}");
}
