//! Fixture da arrumação do clone depois do `omhl merge` (CA-043 a CA-049): o `MergeFixture` com um
//! `origin` de verdade (um repositório nu local, pelo `url.<nu>.insteadOf` da URL do GitHub), o
//! merge do GitHub simulado no nu, e um `git` de registro no `PATH` do `omhl`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::child::{Output, omhl};
use super::git::{git_ok, write_executable};
use super::merge::{MergeFixture, W1_BRANCH, path_with};

/// Como o GitHub junta o PR na `main` do remoto.
#[derive(Clone, Copy)]
pub enum Remote {
    MergeCommit,
    Squash,
    Rebase,
}

// Cada chamada anota `<cwd>\t<argv>`; antes de `worktree remove` roda `before-remove` e copia a pasta
// de dados, antes de `switch` roda `before-switch`; os dois scripts usam o git real.
const GIT_WRAPPER: &str = r#"#!/bin/sh
printf '%s\t%s\n' "$PWD" "$*" >> "@LOG@"
case " $* " in
  *" worktree remove "*)
    if [ -f "@HOOKS@/before-remove" ]; then sh "@HOOKS@/before-remove"; fi
    if [ ! -d "@SNAPSHOT@" ]; then cp -R "@XDG@" "@SNAPSHOT@"; fi ;;
  *" switch "*)
    if [ -f "@HOOKS@/before-switch" ]; then sh "@HOOKS@/before-switch"; fi ;;
esac
exec "@GIT@" "$@"
"#;

pub struct TidyFixture {
    pub merge: MergeFixture,
    /// Onde o `omhl merge` roda: a worktree ligada, ou o checkout principal.
    pub cwd: PathBuf,
    pub bare: PathBuf,
    bin: PathBuf,
    log: PathBuf,
    hooks: PathBuf,
    snapshot: PathBuf,
}

pub fn real_git() -> PathBuf {
    let found = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("sh runs");

    PathBuf::from(String::from_utf8_lossy(&found.stdout).trim())
}

impl TidyFixture {
    /// O trabalho na worktree `.worktrees/chore-wi-w1__x`, o checkout principal na `main`.
    pub fn in_default_worktree(remote: Remote) -> TidyFixture {
        TidyFixture::in_worktree(remote, Path::new(".worktrees/chore-wi-w1__x"))
    }

    /// O trabalho numa worktree fora do repositório, em `<scratch>/x`.
    pub fn in_outside_worktree(remote: Remote) -> TidyFixture {
        TidyFixture::in_worktree(remote, Path::new("../x"))
    }

    /// O trabalho numa worktree em `relative` (a partir do checkout principal; `../` cai no scratch).
    fn in_worktree(remote: Remote, relative: &Path) -> TidyFixture {
        let fixture = TidyFixture::base(remote);
        fixture.merge.git(&["switch", "-q", "main"]);
        let path = match relative.strip_prefix("..") {
            Ok(rest) => fixture.merge.scratch().join(rest),
            Err(_) => fixture.merge.dir().join(relative),
        };
        let path_arg = path.display().to_string();
        fixture
            .merge
            .git(&["worktree", "add", "-q", &path_arg, W1_BRANCH]);
        let exclude = fixture.merge.dir().join(".git/info/exclude");
        fs::write(&exclude, ".worktrees/\n").expect("the exclude file is written");
        let cwd = fs::canonicalize(&path).expect("the worktree exists");

        TidyFixture { cwd, ..fixture }
    }

    /// O trabalho no checkout principal, na branch do work item, sem worktree.
    pub fn in_main_checkout(remote: Remote) -> TidyFixture {
        TidyFixture::base(remote)
    }

    fn base(remote: Remote) -> TidyFixture {
        let merge = MergeFixture::new();
        fs::write(merge.dir().join(".gitignore"), ".env\n.worktrees/\n")
            .expect("the gitignore is written");
        merge.git(&["add", ".gitignore"]);
        merge.git(&["commit", "-q", "-m", "ignore"]);
        merge.git(&["push", "-q", "origin", W1_BRANCH]);
        merge.answer_open_pull_request();
        let bare = merge.bare();
        let bin = merge.scratch().join("git-bin");
        let hooks = merge.scratch().join("git-hooks");
        for folder in [&bin, &hooks] {
            fs::create_dir_all(folder).expect("the folder is created");
        }
        let log = merge.scratch().join("git.log");
        let snapshot = merge.scratch().join("xdg-at-removal");
        write_executable(
            &bin.join("git"),
            &GIT_WRAPPER
                .replace("@LOG@", &log.display().to_string())
                .replace("@HOOKS@", &hooks.display().to_string())
                .replace("@SNAPSHOT@", &snapshot.display().to_string())
                .replace("@XDG@", &merge.project.xdg().display().to_string())
                .replace("@GIT@", &real_git().display().to_string()),
        );
        let cwd = fs::canonicalize(merge.dir()).expect("the main checkout exists");
        let fixture = TidyFixture {
            merge,
            cwd,
            bare,
            bin,
            log,
            hooks,
            snapshot,
        };
        fixture.github_merges(remote);

        fixture
    }

    /// O GitHub junta a branch na `main` do nu, como a estratégia dada.
    fn github_merges(&self, remote: Remote) {
        let clone = self.merge.scratch().join("github");
        let clone_arg = clone.display().to_string();
        let bare_arg = self.bare.display().to_string();
        git_ok(
            self.merge.scratch(),
            &["clone", "-q", &bare_arg, &clone_arg],
        );
        let tip = format!("origin/{W1_BRANCH}");
        let github = |args: &[&str]| {
            let mut full = vec![
                "-c",
                "user.name=GitHub",
                "-c",
                "user.email=noreply@github.com",
            ];
            full.extend_from_slice(args);
            git_ok(&clone, &full)
        };
        match remote {
            Remote::MergeCommit => {
                github(&[
                    "merge",
                    "-q",
                    "--no-ff",
                    "-m",
                    "Merge pull request #42",
                    &tip,
                ]);
            }
            Remote::Squash => {
                github(&["merge", "-q", "--squash", &tip]);
                github(&["commit", "-q", "-m", "the work (#42)"]);
            }
            Remote::Rebase => {
                // outro PR entra antes: os commits reaplicados ganham outro pai e nunca repetem o SHA
                github(&[
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    "another pull request",
                ]);
                let range = format!("main..{tip}");
                github(&["cherry-pick", "--allow-empty", &range]);
            }
        }
        github(&["push", "-q", "origin", "main"]);
    }

    pub fn main_dir(&self) -> PathBuf {
        fs::canonicalize(self.merge.dir()).expect("the main checkout exists")
    }

    pub fn git(&self, args: &[&str]) -> String {
        self.merge.git(args).trim().to_owned()
    }

    /// O GitHub apagou a branch do PR, e o clone a esquece com `fetch --prune`.
    pub fn delete_remote_branch(&self) {
        self.git(&["push", "-q", "origin", "--delete", W1_BRANCH]);
        self.git(&["fetch", "-q", "--prune", "origin"]);
    }

    /// Um script `sh` rodado logo antes do `git worktree remove` (`before-remove`) ou do `git switch`
    /// (`before-switch`); `@GIT@` vira o git real.
    pub fn before(&self, hook: &str, script: &str) {
        fs::write(
            self.hooks.join(hook),
            script.replace("@GIT@", &real_git().display().to_string()),
        )
        .expect("the hook script is written");
    }

    fn path(&self) -> String {
        path_with(&self.bin).replacen(':', &format!(":{}:", self.merge.gh.bin().display()), 1)
    }

    /// `omhl --format json merge w1 <extra>` rodado de `cwd`.
    pub fn merge_json(&self, extra: &[&str]) -> Output {
        let mut args = vec!["--format", "json", "merge", super::merge::W1];
        args.extend_from_slice(extra);

        self.merge
            .omhl_cmd_with_path(&args, &self.path())
            .cwd(&self.cwd)
            .run()
    }

    /// `omhl merge w1` (texto) rodado de `cwd`.
    pub fn merge_text(&self) -> Output {
        self.merge
            .omhl_cmd_with_path(&["merge", super::merge::W1], &self.path())
            .cwd(&self.cwd)
            .run()
    }

    /// As chamadas do `git` feitas pelo `omhl`: `(cwd, argv)`.
    pub fn git_calls(&self) -> Vec<(String, String)> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(cwd, argv)| (cwd.to_owned(), argv.to_owned()))
            .collect()
    }

    pub fn called(&self, needle: &str) -> bool {
        self.git_calls()
            .iter()
            .any(|(_, argv)| argv.contains(needle))
    }

    /// O estado do work item no banco copiado no instante do `git worktree remove`.
    pub fn status_at_removal(&self) -> Option<String> {
        if !self.snapshot.is_dir() {
            return None;
        }
        let got = omhl(&["--format", "json", "get", super::merge::W1])
            .cwd(self.merge.dir())
            .xdg(&self.snapshot)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .run();
        let value: serde_json::Value =
            serde_json::from_str(got.stdout.trim_end()).unwrap_or_default();

        value["node"]["data"]["status"].as_str().map(str::to_owned)
    }

    pub fn rev(&self, reference: &str) -> Option<String> {
        let output = Command::new(real_git())
            .args(["rev-parse", "--verify", "-q", reference])
            .current_dir(self.merge.dir())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .expect("git runs");

        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    pub fn remote_main(&self) -> String {
        let bare_arg = self.bare.display().to_string();
        git_ok(
            self.merge.scratch(),
            &["-C", &bare_arg, "rev-parse", "main"],
        )
        .trim()
        .to_owned()
    }

    /// Roda `omhl --format json <args>` de dentro de `dir` já apagado: o shell entra, apaga e executa.
    pub fn omhl_from_deleted(&self, dir: &Path, args: &[&str]) -> Output {
        let quoted: Vec<String> = args.iter().map(|arg| format!("'{arg}'")).collect();
        let script = format!(
            "mkdir -p '{dir}' && cd '{dir}' && rmdir '{dir}' && exec '{bin}' --format json {args}",
            dir = dir.display(),
            bin = env!("CARGO_BIN_EXE_omhl"),
            args = quoted.join(" "),
        );
        let output = Command::new("sh")
            .args(["-c", &script])
            .current_dir(self.merge.dir())
            .env("XDG_DATA_HOME", self.merge.project.xdg())
            .env("HOME", self.merge.scratch())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .expect("sh runs");

        Output {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}
