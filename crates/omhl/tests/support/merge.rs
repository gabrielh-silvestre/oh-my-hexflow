//! Repositório de fixture do `omhl merge`: git real com `origin` no GitHub, o work item `w1` aberto
//! na sua branch, um banco novo e o `gh` falso (`support::gh`) no começo do `PATH`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::child::{Cmd, Output, omhl, omhl_json};
use super::ext::ExtTree;
use super::gh::{FakeGh, MERGE_COMMIT, check_run, pr, rollup, state};
use super::git::{git_ok, write_executable};
use super::project::Project;

pub const W1: &str = "work-items.w1";
pub const W1_BRANCH: &str = "chore/wi-w1__x";
pub const W2_BRANCH: &str = "chore/wi-w2__y";
pub const PR_ID: &str = "work-items.w1.github-42";
pub const ORIGIN: &str = "https://github.com/acme/widgets.git";
pub const PROVIDER_ON: &str = "version = 1\n\n[provider]\nname = \"github\"\n";
pub const PROVIDER_REQUIRE_CI: &str =
    "version = 1\n\n[provider]\nname = \"github\"\nrequire_ci = true\n";
pub const BROKEN: &str = "version = 1\ncolour = true\n";
pub const CI_GREEN: &str = "checks.ci-green-before-merge";
pub const RUN_TESTS: &str = "checks.run-tests";
pub const OWNER_NOTE: &str = "owner approved merging with lint red";
pub const TOKEN: &str = "test-token-123";

pub struct MergeFixture {
    pub project: Project,
    scratch: TempDir,
    pub gh: FakeGh,
}

impl MergeFixture {
    /// Integração ligada, work item `w1` (tipo `chore`) aberto na `chore/wi-w1__x` com um commit
    /// próprio, e o PR 42 aberto dessa branch para `main`, com o CI verde e o merge aceito.
    pub fn new() -> MergeFixture {
        MergeFixture::with_workflow(Some(PROVIDER_ON))
    }

    pub fn with_workflow(workflow: Option<&str>) -> MergeFixture {
        MergeFixture::build(workflow, &[])
    }

    /// Como `with_workflow`, com checks do projeto (`slug`, `toml`) carregados por `omhl load`; o
    /// marcador `@ORDER@` do toml vira o caminho do registro de ordem do `gh` falso.
    pub fn with_checks(workflow: Option<&str>, checks: &[(&str, &str)]) -> MergeFixture {
        MergeFixture::build(workflow, checks)
    }

    fn build(workflow: Option<&str>, checks: &[(&str, &str)]) -> MergeFixture {
        let project = Project::new("acme");
        let scratch = TempDir::new().expect("scratch directory is created");
        let gh = FakeGh::new(scratch.path());
        if let Some(text) = workflow {
            project.write_workflow(text);
        }
        let tree = ExtTree::at(project.dir());
        let order = gh.order_log().display().to_string();
        for (slug, toml) in checks {
            tree.check(slug, &toml.replace("@ORDER@", &order));
        }
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Test Author"],
            &["config", "user.email", "author@example.com"],
            &["remote", "add", "origin", ORIGIN],
            &["add", "-A"],
            &["commit", "-q", "-m", "base"],
            &["switch", "-q", "-c", W1_BRANCH],
            &["commit", "-q", "--allow-empty", "-m", "the work"],
        ] {
            git_ok(project.dir(), args);
        }
        let bare = scratch.path().join("remote.git");
        let bare_arg = bare.display().to_string();
        git_ok(
            scratch.path(),
            &["init", "-q", "--bare", "-b", "main", &bare_arg],
        );
        // o `origin` continua a URL do GitHub para o `omhl`, e o `git` busca no repositório nu local
        let rewrite = format!("url.{bare_arg}.insteadOf");
        git_ok(project.dir(), &["config", &rewrite, ORIGIN]);
        git_ok(
            project.dir(),
            &["push", "-q", "-u", "origin", "main", W1_BRANCH],
        );
        let fixture = MergeFixture {
            project,
            scratch,
            gh,
        };
        fixture.append(&[node(
            W1,
            "work-item",
            json!({ "title": "t", "status": "open", "type": "chore" }),
        )]);
        if !checks.is_empty() {
            let loaded = fixture.omhl_cmd(&["load"]).run();
            assert_eq!(loaded.code, Some(0), "load stderr: {}", loaded.stderr);
        }
        fixture.answer_open_pull_request();

        fixture
    }

    pub fn dir(&self) -> &Path {
        self.project.dir()
    }

    pub fn scratch(&self) -> &Path {
        self.scratch.path()
    }

    /// O `origin` local (repositório nu) que o `url.<nu>.insteadOf` põe no lugar do GitHub.
    pub fn bare(&self) -> PathBuf {
        self.scratch.path().join("remote.git")
    }

    pub fn git(&self, args: &[&str]) -> String {
        git_ok(self.dir(), args)
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    /// Um commit novo na branch, sem empurrar: o `HEAD` deixa de ser o `headRefOid` lido.
    pub fn commit_locally(&self) {
        self.git(&["commit", "-q", "--allow-empty", "-m", "not pushed"]);
    }

    /// O PR 42 aberto da branch no `HEAD`, CI verde, e o merge que deixa o PR `MERGED` na `main`.
    pub fn answer_open_pull_request(&self) {
        let head = self.head();
        self.answer_pull_requests(&[pr(42, "OPEN", &head, "main", false, false)]);
        self.gh.answer_json(
            "checks",
            &rollup(&head, &[check_run("build", "COMPLETED", "SUCCESS")]),
        );
        self.gh
            .answer_json("state", &state("OPEN", &head, "main", None, false));
        self.gh.answer_json(
            "merged",
            &state("MERGED", &head, "main", Some(MERGE_COMMIT), false),
        );
    }

    pub fn answer_pull_requests(&self, prs: &[Value]) {
        self.gh.answer_json("list", &Value::Array(prs.to_vec()));
    }

    /// Um `omhl` com o `gh` falso primeiro no `PATH` e um ambiente que um `gh` sem isolamento leria.
    pub fn omhl_cmd(&self, args: &[&str]) -> Cmd {
        self.omhl_cmd_with_path(args, &path_with(self.gh.bin()))
    }

    pub fn omhl_cmd_with_path(&self, args: &[&str], path: &str) -> Cmd {
        omhl(args)
            .cwd(self.dir())
            .xdg(self.project.xdg())
            .env("PATH", path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GH_REPO", "intruder/elsewhere")
            .env("GH_HOST", "intruder.example.com")
            .env("GH_FORCE_TTY", "1")
            .env("CLICOLOR_FORCE", "1")
            .env("GH_TOKEN", TOKEN)
    }

    /// `omhl --format json merge w1 <extra>`.
    pub fn merge(&self, extra: &[&str]) -> Output {
        self.omhl_cmd(&merge_args(true, extra)).run()
    }

    /// `omhl merge w1 <extra>` no formato padrão (texto para o agente).
    pub fn merge_text(&self, extra: &[&str]) -> Output {
        self.omhl_cmd(&merge_args(false, extra)).run()
    }

    pub fn merge_with_path(&self, path: &str) -> Output {
        self.omhl_cmd_with_path(&merge_args(true, &[]), path).run()
    }

    pub fn run_json(&self, args: &[&str]) -> Output {
        let mut full = vec!["--format", "json"];
        full.extend_from_slice(args);

        self.omhl_cmd(&full).run()
    }

    pub fn append(&self, items: &[Value]) {
        let appended = omhl(&["append"])
            .cwd(self.dir())
            .xdg(self.project.xdg())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .stdin(json!({ "items": items }).to_string())
            .run();

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }

    /// `omhl attest <check> w1 --outcome <outcome>` no `HEAD`.
    pub fn attest(&self, check: &str, outcome: &str) {
        let attested = omhl_json(&["attest", check, W1, "--outcome", outcome])
            .cwd(self.dir())
            .xdg(self.project.xdg())
            .run();

        assert_eq!(attested.code, Some(0), "stderr: {}", attested.stderr);
    }

    /// Os dados do node, ou `None` quando o `get` não o acha.
    pub fn node_data(&self, id: &str) -> Option<Value> {
        let got = self.run_json(&["get", id]);

        (got.code == Some(0)).then(|| json_line(&got.stdout)["node"]["data"].clone())
    }

    pub fn work_item_status(&self) -> Value {
        self.node_data(W1).expect("the work item exists")["status"].clone()
    }

    /// O total de registros e a cabeça da cadeia: igual antes e depois quer dizer que nada foi gravado.
    pub fn chain(&self) -> Value {
        let verified = self.run_json(&["verify"]);
        assert_eq!(verified.code, Some(0), "stderr: {}", verified.stderr);
        let report = json_line(&verified.stdout);

        json!({ "total_records": report["total_records"], "chain_head": report["chain_head"] })
    }

    pub fn list(&self, type_name: &str) -> Vec<Value> {
        let listed = self.run_json(&["list", "--type", type_name]);
        assert_eq!(listed.code, Some(0), "list stderr: {}", listed.stderr);

        json_line(&listed.stdout)["items"]
            .as_array()
            .expect("items is an array")
            .clone()
    }

    /// As attestations de `checks.ci-green-before-merge` do work item.
    pub fn ci_attestations(&self) -> Vec<Value> {
        self.list("attestation")
            .into_iter()
            .filter(|item| item["data"]["check"] == CI_GREEN)
            .collect()
    }

    /// Um `PATH` com só o `git` e o `sh` reais: nenhum `gh` alcançável.
    pub fn path_without_gh(&self) -> String {
        let bin = self.scratch().join("no-gh-bin");
        fs::create_dir_all(&bin).expect("the folder is created");
        for program in ["git", "sh"] {
            let real = which(program);
            write_executable(
                &bin.join(program),
                &format!(
                    "#!{}\nexec \"{}\" \"$@\"\n",
                    which("sh").display(),
                    real.display()
                ),
            );
        }

        bin.display().to_string()
    }
}

fn merge_args<'a>(json_format: bool, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = if json_format {
        vec!["--format", "json", "merge", W1]
    } else {
        vec!["merge", W1]
    };
    args.extend_from_slice(extra);

    args
}

pub fn path_with(first: &Path) -> String {
    format!(
        "{}:{}",
        first.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

fn which(program: &str) -> PathBuf {
    let found = Command::new("sh")
        .args(["-c", &format!("command -v {program}")])
        .output()
        .expect("sh runs");

    PathBuf::from(String::from_utf8_lossy(&found.stdout).trim())
}

pub fn node(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

pub fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).unwrap_or_else(|error| panic!("{error}: {text}"))
}

/// O objeto `error` de uma recusa em JSON (stderr).
pub fn error_of(output: &Output) -> Value {
    assert_eq!(
        output.code,
        Some(1),
        "stdout: {} stderr: {}",
        output.stdout,
        output.stderr
    );

    json_line(&output.stderr)["error"].clone()
}

pub fn code_of(output: &Output) -> Value {
    error_of(output)["code"].clone()
}

/// Os nomes dos passos que a saída (sucesso ou recusa) traz, na ordem.
pub fn step_names(steps: &Value) -> Vec<String> {
    steps
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| step["step"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

pub fn step<'v>(steps: &'v Value, name: &str) -> &'v Value {
    steps
        .as_array()
        .and_then(|steps| steps.iter().find(|step| step["step"] == name))
        .unwrap_or_else(|| panic!("no step {name} in {steps}"))
}
