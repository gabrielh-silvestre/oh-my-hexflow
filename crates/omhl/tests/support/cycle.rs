//! Estados dos comandos simples do ciclo (`project`, `check sweep`, `check retire`, `guideline retire`,
//! `hooks install`, `spec init`, `byproduct`), montados pela CLI real e compartilhados por
//! `output_agent_leaves.rs` e `output_agent_cycle_simple.rs`.

use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::child::{Cmd, omhl};
use super::ext::ExtTree;
use super::git::{commit_marker, git_in};
use super::inproc::run_inproc;
use super::omhl::work_item_data;
use super::project::Project;

pub const WORK_ITEM: &str = "work-items.payroll";
pub const WORK_BRANCH: &str = "feat/wi-payroll__x";

pub const ORPHAN_CHECK: &str = "checks.ledger-audit";
pub const ORPHAN_GUIDELINE: &str = "guidelines.brief-tone";
pub const SUCCESSOR_GUIDELINE: &str = "guidelines.kind-tone";
pub const CHECK_WITH_LINES: &str = "checks.payroll-lines";
pub const CHECK_WITH_WIDE_LINE: &str = "checks.payroll-wide";
pub const CHECK_WITHOUT_OUTPUT: &str = "checks.payroll-quiet";
pub const SPEC_SLUG: &str = "export-reports";

/// O aviso que todo texto de saída guardada de check traz, nos dois layouts (CA-10).
pub const SECRET_WARNING: &str = "The output may contain secrets: do not copy it into a PR, issue, commit, --note or omhl append, and do not repeat it to the user.";

const GENERATED_HEADER: &str = "<!-- omhl: generated file, do not edit -->";

// ---------------------------------------------------------------- git e `omhl` de preparo

pub fn git(dir: &Path, args: &[&str]) {
    let output = git_in(dir, args);

    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Preparo pelo binário: o teste não segue se o comando não saiu 0.
pub fn omhl_succeeds(project: &Project, args: &[&str], stdin: &str) {
    let output = omhl(args)
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(stdin)
        .run();

    assert_eq!(
        output.code,
        Some(0),
        "omhl {args:?} failed: {}",
        output.stderr
    );
}

/// Preparo em processo, em JSON: o teste não segue se o comando escreveu no stderr.
fn run_ok(project: &Project, args: &[&str], stdin: &str) {
    let mut full = vec!["--format", "json"];
    full.extend_from_slice(args);

    let run = run_inproc(project, &full, stdin);

    assert_eq!(run.stderr, "", "the fixture command {args:?} failed");
}

fn append_work_items(project: &Project, items: &[(&str, &str)]) {
    let wires: Vec<Value> = items
        .iter()
        .map(|(id, title)| {
            json!({ "body": {
                "kind": "node", "id": id, "type": "work-item", "data": work_item_data(title),
            } })
        })
        .collect();

    run_ok(project, &["append"], &json!({ "items": wires }).to_string());
}

/// TOML de um check `before-pr` bloqueante que roda `command` num shell.
pub fn check_file(concern: &str, variant: &str, command: &str) -> String {
    let run = serde_json::to_string(&["sh", "-c", command]).expect("the argv serializes");

    format!(
        "blocking = true\nhow = \"Guards the payroll.\"\nconcern = \"{concern}\"\nvariant = \"{variant}\"\nrun = {run}\ntimeout_secs = 60\nexpect = [{{ exit = 0, outcome = \"passed\" }}]\n\n[when]\nevent = \"before-pr\"\n"
    )
}

/// Repositório git na branch do work item, com os checks no commit base, o work item gravado e o `load` feito.
pub fn check_repo(checks: &[(&str, &str)]) -> Project {
    let project = Project::new("acme");
    let dir = project.dir();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "Test Author"]);
    git(dir, &["config", "user.email", "author@example.com"]);
    let tree = ExtTree::at(dir);
    for (slug, toml) in checks {
        tree.check(slug, toml);
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "base"]);
    git(dir, &["switch", "-q", "-c", WORK_BRANCH]);
    let work_item = json!({
        "items": [{ "body": {
            "kind": "node", "id": WORK_ITEM, "type": "work-item",
            "data": work_item_data("Payroll rollout"),
        } }]
    });
    omhl_succeeds(
        &project,
        &["--format", "json", "append"],
        &work_item.to_string(),
    );
    omhl_succeeds(&project, &["--format", "json", "load"], "");

    project
}

// ---------------------------------------------------------------- `project`

fn project_with_two_work_items() -> Project {
    let project = Project::new("acme");
    append_work_items(
        &project,
        &[
            ("work-items.alpha", "Alpha item"),
            ("work-items.bravo", "Bravo item"),
        ],
    );

    project
}

/// Nada projetado ainda: o `project` escreve os dois arquivos.
pub fn project_first() -> Project {
    project_with_two_work_items()
}

/// Dois arquivos já projetados, um work item novo e um arquivo gerado sem dono: 1 escrito, 2 iguais, 1 removido.
pub fn project_mixed() -> Project {
    let project = project_with_two_work_items();
    run_ok(&project, &["project"], "");
    append_work_items(&project, &[("work-items.carol", "Carol item")]);
    let stale = project.dir().join(".omhl/projection/stale-note.md");
    fs::write(stale, format!("{GENERATED_HEADER}\nold\n")).expect("the stale file is written");

    project
}

/// Um arquivo escrito à mão no lugar de um gerado: a projeção recusa sobrescrevê-lo.
pub fn project_blocked() -> Project {
    let project = project_with_two_work_items();
    let projection = project.dir().join(".omhl/projection");
    fs::create_dir_all(&projection).expect("the projection folder is created");
    fs::write(projection.join("work-items.alpha.md"), "written by hand\n")
        .expect("the hand-written file is written");

    project
}

// ---------------------------------------------------------------- `check retire` e `guideline retire`

/// O check carregado e o arquivo apagado: o node segue vigente, órfão.
pub fn retire_check() -> Project {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).check("ledger-audit", &check_file("style", "format", "exit 0"));
    run_ok(&project, &["load"], "");
    fs::remove_file(project.dir().join(".omhl/checks/ledger-audit.toml"))
        .expect("the check file is removed");

    project
}

/// Duas diretivas carregadas e o arquivo de `brief-tone` apagado: `kind-tone` segue com arquivo e serve de sucessora.
pub fn retire_guidelines() -> Project {
    let project = Project::new("acme");
    let tree = ExtTree::at(project.dir());
    tree.guideline(
        "brief-tone",
        "kind = \"business\"\ntext = \"Be brief at all times\"\n",
    );
    tree.guideline(
        "kind-tone",
        "kind = \"business\"\ntext = \"Be kind at all times\"\n",
    );
    run_ok(&project, &["load"], "");
    fs::remove_file(project.dir().join(".omhl/guidelines/brief-tone.toml"))
        .expect("the guideline file is removed");

    project
}

// ---------------------------------------------------------------- `spec init`

/// Repositório git com o marcador commitado e o work item aberto.
pub fn spec_repo() -> Project {
    let project = Project::new("acme");
    commit_marker(project.dir());
    append_work_items(&project, &[(WORK_ITEM, "Payroll rollout")]);

    project
}

// ---------------------------------------------------------------- `byproduct`

/// Um `check run` guardou a saída de três checks: com três linhas, com uma linha acima do limite e sem saída.
pub fn byproduct_repo() -> Project {
    let lines = check_file(
        "style",
        "format",
        "echo first-line-out; echo second-line-out; echo err-line-here >&2",
    );
    let wide = check_file(
        "test",
        "suite",
        "head -c 2100 /dev/zero | tr '\\0' 'w'; echo; echo short-tail-line",
    );
    let quiet = check_file("compile", "build", "exit 0");
    let project = check_repo(&[
        ("payroll-lines", &lines),
        ("payroll-wide", &wide),
        ("payroll-quiet", &quiet),
    ]);
    let _runs = check_run_guard();

    omhl_succeeds(
        &project,
        &["--format", "json", "check", "run", "--event", "before-pr"],
        "",
    );

    project
}

// ---------------------------------------------------------------- `check sweep`

/// `check run` e `check sweep` varrem o `/proc` inteiro: um `check run` que começa mata o fugido de outro.
/// Quem roda `check run` segura a trava compartilhada (vários ao mesmo tempo); quem cria um fugido e conta com ele
/// segura a exclusiva. O `flock` vale entre threads (cada guarda abre o arquivo de novo) e entre binários de teste
/// (é o arquivo do `check_sweep_cli`).
pub struct SweepGuard {
    _file: File,
}

fn lock_file() -> File {
    File::create(Path::new(env!("CARGO_TARGET_TMPDIR")).join("check_sweep_cli.lock"))
        .expect("the sweep lock file opens")
}

pub fn check_run_guard() -> SweepGuard {
    let file = lock_file();
    file.lock_shared().expect("the shared sweep lock is taken");

    SweepGuard { _file: file }
}

pub fn stray_guard() -> SweepGuard {
    let file = lock_file();
    file.lock().expect("the exclusive sweep lock is taken");

    SweepGuard { _file: file }
}

/// Um check solta um `sleep 61` em sessão própria e só sai quando o neto gravou o pid: o `check run` termina com o fugido vivo.
fn stray_command(pidfile: &Path) -> String {
    let path = pidfile.display();

    format!(
        "setsid sh -c 'echo $$ > {path}.tmp; mv {path}.tmp {path}; exec sleep 61' >/dev/null 2>&1 </dev/null & while [ ! -e {path} ]; do sleep 0.01; done; exit 0"
    )
}

/// O `check run` já terminou e deixou um fugido vivo para o `check sweep` matar; o guarda segue com quem chama.
pub fn sweep_stray() -> (Project, SweepGuard) {
    let pids = TempDir::new().expect("the pid folder is created");
    let command = stray_command(&pids.path().join("stray.pid"));
    let project = check_repo(&[("payroll-stray", &check_file("style", "format", &command))]);
    let guard = stray_guard();

    omhl_succeeds(
        &project,
        &["--format", "json", "check", "run", "--event", "before-pr"],
        "",
    );

    (project, guard)
}

// ---------------------------------------------------------------- `hooks install`

/// Cópia do binário do cargo numa pasta própria: o `hooks install` grava o caminho de quem roda.
pub struct Program {
    path: PathBuf,
    dir: TempDir,
}

impl Program {
    /// O binário em `<pasta temporária>/<rel>`: link físico quando o disco é o mesmo (são 100 MB), senão `cp`.
    pub fn at(rel: &str) -> Program {
        let dir = TempDir::new().expect("the program folder is created");
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().expect("the copy has a parent folder"))
            .expect("the copy folder is created");
        let source = fs::canonicalize(env!("CARGO_BIN_EXE_omhl")).expect("the omhl binary exists");
        if fs::hard_link(&source, &path).is_err() {
            // cópia por processo filho: `fs::copy` deixaria o arquivo aberto para escrita (ETXTBSY)
            let status = Command::new("cp")
                .arg("-p")
                .arg(&source)
                .arg(&path)
                .status()
                .expect("cp runs");
            assert!(status.success(), "cp of the omhl binary failed");
        }

        Program {
            path: fs::canonicalize(&path).expect("the copy exists"),
            dir,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }
}

/// Um projeto e, quando o comando grava o caminho do binário, a cópia que o roda.
pub struct State {
    pub project: Project,
    pub program: Option<Program>,
}

impl State {
    /// O comando pelo binário (ou pela cópia), com a pasta do projeto como cwd e o git sem configuração do usuário.
    pub fn command(&self, args: &[&str]) -> Cmd {
        let cmd = omhl(args)
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");

        match &self.program {
            Some(program) => cmd.program(program.path()),
            None => cmd,
        }
    }
}

fn hooks_state(program: Option<Program>) -> State {
    let project = Project::new("acme");
    commit_marker(project.dir());

    State { project, program }
}

/// O binário fora de qualquer pasta `target/`: a instalação não traz aviso.
pub fn hooks_clean() -> State {
    hooks_state(Some(Program::at("bin/omhl")))
}

/// O binário numa pasta `target/debug`: a instalação traz o aviso `binary-in-build-dir`.
pub fn hooks_in_build_dir() -> State {
    hooks_state(Some(Program::at("target/debug/omhl")))
}

/// Um hook que não é do omhl já mora no repositório: a instalação recusa antes de escrever.
pub fn hooks_foreign() -> State {
    let state = hooks_state(None);
    let hook = state.project.dir().join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 0\n").expect("the foreign hook is written");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))
        .expect("the foreign hook is executable");

    state
}
