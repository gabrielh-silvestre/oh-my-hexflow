//! A arrumação do clone depois de o `omhl merge` fechar o work item: nada é forçado, o que o `git` recusa vira um aviso.

use std::path::Path;

pub(crate) use crate::probe::{GitOutput, ProbeError, TidyGit};

const STATUS_LINES: usize = 20;
const ASK: &str = "ask the user what to do with what is left before removing anything";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TidyStage {
    Worktree,
    Switch,
    FastForward,
    Branch,
}

impl TidyStage {
    pub fn as_str(self) -> &'static str {
        match self {
            TidyStage::Worktree => "worktree",
            TidyStage::Switch => "switch",
            TidyStage::FastForward => "fast-forward",
            TidyStage::Branch => "branch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageResult {
    Done,
    Skipped(String),
    NotNeeded,
}

#[derive(Debug, Default)]
struct Entry {
    path: String,
    branch: Option<String>,
    locked: Option<String>,
    prunable: bool,
}

fn entries(porcelain: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for line in porcelain.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "worktree" {
            entries.push(Entry {
                path: value.to_owned(),
                ..Entry::default()
            });
            continue;
        }
        let Some(entry) = entries.last_mut() else {
            continue;
        };
        match key {
            "branch" => entry.branch = Some(value.trim_start_matches("refs/heads/").to_owned()),
            "locked" => entry.locked = Some(value.to_owned()),
            "prunable" => entry.prunable = true,
            _ => {}
        }
    }
    entries
}

fn failure(output: &Result<GitOutput, ProbeError>) -> Option<String> {
    match output {
        Ok(output) if output.code == 0 => None,
        Ok(output) => Some(output.stderr.trim().to_owned()),
        Err(error) => Some(error.message.clone()),
    }
}

fn ran(
    output: &Result<GitOutput, ProbeError>,
    skipped: impl FnOnce(String) -> String,
) -> StageResult {
    match failure(output) {
        None => StageResult::Done,
        Some(reason) => StageResult::Skipped(skipped(reason)),
    }
}

/// A branch sai por último porque o `git branch` recusa uma em uso; depois da remoção nada roda no cwd apagado.
pub(crate) fn tidy(
    git: &dyn TidyGit,
    main_dir: &Path,
    branch: &str,
    integration: &str,
    declared: bool,
) -> Vec<(TidyStage, StageResult)> {
    let listed = git
        .git(main_dir, &["worktree", "list", "--porcelain"])
        .ok()
        .filter(|output| output.code == 0)
        .map(|output| entries(&output.stdout))
        .unwrap_or_default();
    let main_branch = listed.first().and_then(|main| main.branch.clone());
    let own = listed
        .iter()
        .skip(1)
        .find(|entry| entry.branch.as_deref() == Some(branch));

    let worktree = match own {
        None => StageResult::NotNeeded,
        Some(entry) if entry.locked.is_some() => StageResult::Skipped(format!(
            "the worktree {} is locked ({}), so it stays; {ASK}",
            entry.path,
            entry
                .locked
                .as_deref()
                .filter(|reason| !reason.is_empty())
                .unwrap_or("no reason given")
        )),
        Some(entry) if entry.prunable => {
            ran(&git.git(main_dir, &["worktree", "prune"]), |reason| {
                format!("git worktree prune failed: {reason}")
            })
        }
        Some(entry) => remove(git, main_dir, entry),
    };

    let on_branch = own.is_none() && main_branch.as_deref() == Some(branch);
    let switch = match on_branch {
        false => StageResult::NotNeeded,
        true => ran(&git.git(main_dir, &["switch", integration]), |reason| {
            format!("the main checkout did not switch to {integration}: {reason}")
        }),
    };
    let integration_here = match on_branch {
        true => switch == StageResult::Done,
        false => main_branch.as_deref() == Some(integration),
    };

    let fast_forward = fast_forward(git, main_dir, integration, declared, integration_here);

    let blocked =
        matches!(worktree, StageResult::Skipped(_)) || matches!(switch, StageResult::Skipped(_));
    let branch_stage = match blocked {
        true => StageResult::Skipped(format!(
            "the branch {branch} is still in use, so git branch -d was not run"
        )),
        false => ran(&git.git(main_dir, &["branch", "-d", branch]), |reason| {
            format!(
                "git branch -d kept {branch}: after a squash or rebase merge its commits are not in {integration}, so the branch stays (known limit; deleting it is the owner's decision): {reason}"
            )
        }),
    };

    vec![
        (TidyStage::Worktree, worktree),
        (TidyStage::Switch, switch),
        (TidyStage::FastForward, fast_forward),
        (TidyStage::Branch, branch_stage),
    ]
}

fn remove(git: &dyn TidyGit, main_dir: &Path, entry: &Entry) -> StageResult {
    let removed = git.git(main_dir, &["worktree", "remove", &entry.path]);
    // Só a recusa deixa a worktree de pé: o `status` dela diz o que bloqueou.
    let Some(reason) = failure(&removed) else {
        return StageResult::Done;
    };
    let status = git
        .git(Path::new(&entry.path), &["status", "--porcelain"])
        .map(|output| {
            output
                .stdout
                .lines()
                .take(STATUS_LINES)
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    let blocking = match status.is_empty() {
        true => reason,
        false => format!("modified or untracked files: {status}"),
    };
    StageResult::Skipped(format!(
        "the worktree {} stays: {blocking}; {ASK}",
        entry.path
    ))
}

fn fast_forward(
    git: &dyn TidyGit,
    main_dir: &Path,
    integration: &str,
    declared: bool,
    in_main: bool,
) -> StageResult {
    let skipped = |reason: String| format!("{integration} was not fast-forwarded: {reason}");
    if in_main {
        if let Some(reason) = failure(&git.git(main_dir, &["fetch", "origin", integration])) {
            return StageResult::Skipped(skipped(reason));
        }
        return ran(
            &git.git(main_dir, &["merge", "--ff-only", "FETCH_HEAD"]),
            skipped,
        );
    }
    // Com a integração declarada a falta da local já recusou antes do merge.
    if !declared {
        let local = format!("refs/heads/{integration}");
        let exists = git.git(main_dir, &["rev-parse", "--verify", "--quiet", &local]);
        if failure(&exists).is_some() {
            return StageResult::Skipped(format!(
                "there is no local {integration}, and it is not created"
            ));
        }
    }
    let refspec = format!("{integration}:{integration}");
    ran(&git.git(main_dir, &["fetch", "origin", &refspec]), skipped)
}

#[cfg(test)]
mod tests;
