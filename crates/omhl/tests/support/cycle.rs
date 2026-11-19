//! Estados dos comandos simples do ciclo (`project`, `check sweep`, `check retire`, `guideline retire`,
//! `hooks install`, `spec init`, `byproduct`), montados pela CLI real e compartilhados por
//! `output_agent_leaves.rs` e `output_agent_cycle_simple.rs`.

use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use nix::fcntl::Flock;
use serde_json::{Value, json};
use tempfile::TempDir;

use super::child::{Cmd, omhl};
use super::ext::ExtTree;
use super::git::{commit_marker, git_in};
use super::inproc::run_inproc;
use super::omhl::work_item_data;
use super::project::Project;
use super::sweep_lock::serialize_sweeps;

pub const WORK_ITEM: &str = "work-items.payroll";
pub const WORK_BRANCH: &str = "feat/wi-payroll__x";

pub const ORPHAN_CHECK: &str = "checks.ledger-audit";
pub const ORPHAN_GUIDELINE: &str = "guidelines.brief-tone";
pub const SUCCESSOR_GUIDELINE: &str = "guidelines.kind-tone";
pub const CHECK_WITH_LINES: &str = "checks.payroll-lines";
pub const CHECK_WITH_WIDE_LINE: &str = "checks.payroll-wide";
pub const CHECK_WITHOUT_OUTPUT: &str = "checks.payroll-quiet";
pub const SPEC_SLUG: &str = "export-reports";

const GENERATED_HEADER: &str = "<!-- omhl: generated file, do not edit -->";

// ---------------------------------------------------------------- leitura do texto impresso

pub fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

pub fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

pub fn assert_text_starting_with(text: &str, symbol: &str) {
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    assert!(first_line(text).starts_with(symbol), "{text}");
}

pub fn assert_single_json_line(text: &str) {
    assert_eq!(text.trim_end().lines().count(), 1, "{text}");
    serde_json::from_str::<Value>(text.trim_end()).expect("the line is JSON");
}

pub fn parsed(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the output is JSON")
}

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
    omhl_succeeds(
        &project,
        &["--format", "json", "check", "run", "--event", "before-pr"],
        "",
    );

    project
}

// ---------------------------------------------------------------- `append` e `attest`

pub const MANUAL_CHECK: &str = "checks.ledger-review";

const MANUAL_CHECK_TOML: &str =
    "blocking = false\nhow = \"Review the ledger by hand.\"\n\n[when]\nevent = \"before-merge\"\n";

/// Pedido de um work item só.
pub const APPEND_ONE: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.ledger-close","type":"work-item","data":{"title":"Close the ledger","status":"open"}}}]}"#;

/// Pedido de dois work items.
pub const APPEND_TWO: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.ledger-close","type":"work-item","data":{"title":"Close the ledger","status":"open"}}},{"body":{"kind":"node","id":"work-items.audit-trail","type":"work-item","data":{"title":"Audit the trail","status":"open"}}}]}"#;

/// O pedido de um work item só, com `key`: repeti-lo devolve os mesmos records com `replayed: true`.
pub const APPEND_KEYED: &str = r#"{"key":"close-ledger-1","items":[{"body":{"kind":"node","id":"work-items.ledger-close","type":"work-item","data":{"title":"Close the ledger","status":"open"}}}]}"#;

/// Work item sem `title`: o schema recusa (`INVALID_RECORD`).
pub const APPEND_MISSING_TITLE: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.ledger-close","type":"work-item","data":{"status":"open"}}}]}"#;

pub const APPEND_NOT_JSON: &str = "{not json";

/// Grava o pedido com sucesso, pelo binário: a fixture do `append` que já gravou (o `replayed`).
pub fn appended(project: &Project, request: &str) {
    omhl_succeeds(project, &["append"], request);
}

/// Repositório git limpo na branch do work item, com o check manual carregado e o work item aberto: o `attest` grava.
pub fn attest_repo() -> Project {
    check_repo(&[("ledger-review", MANUAL_CHECK_TOML)])
}

// ---------------------------------------------------------------- `gate-pr` e `close`

pub const NATIVE_CHECKS: [&str; 2] = ["checks.run-tests", "checks.ci-green-before-merge"];
pub const DECISION_UNANCHORED: &str = "decisions.ledger-split";
pub const DECISION_ON_ASSUMED: &str = "decisions.bonus-cap";
pub const DIVERGENCE_PARTIAL: &str = "divergences.rounding-drift";
pub const DIVERGENCE_MISSING: &str = "divergences.overtime-cap";
pub const GAP_ANSWERED: &str = "gaps.holiday-calendar";
pub const GAP_ANSWER: &str = "premises.calendar-source";

fn node_wire(id: &str, type_name: &str, data: Value) -> Value {
    let mut wire = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    wire["body"]["data"] = data;

    wire
}

fn relation_wire(relation_type: &str, from: &str, to: &str) -> Value {
    json!({ "body": { "kind": "relation", "relation_type": relation_type, "from": from, "to": to } })
}

fn append_wires(project: &Project, items: &[Value]) {
    omhl_succeeds(
        project,
        &["--format", "json", "append"],
        &json!({ "items": items }).to_string(),
    );
}

/// Os dois checks nativos que o gate cobra, atestados no commit atual.
fn attest_natives(project: &Project) {
    for check in NATIVE_CHECKS {
        omhl_succeeds(
            project,
            &[
                "--format",
                "json",
                "attest",
                check,
                WORK_ITEM,
                "--outcome",
                "passed",
            ],
            "",
        );
    }
}

/// Nenhum check atestado: as duas perguntas dos nativos reprovam, sem `next_step`.
pub fn gate_unattested() -> Project {
    check_repo(&[])
}

/// Os nativos atestados, mais um check mecânico que o `check run` gravou com o selo: sete perguntas, todas aprovadas.
pub fn gate_approved() -> Project {
    let toml = check_file("style", "format", "exit 0");
    let project = check_repo(&[("payroll-format", &toml)]);
    omhl_succeeds(
        &project,
        &["--format", "json", "check", "run", "--event", "before-pr"],
        "",
    );
    attest_natives(&project);

    project
}

/// Os nativos atestados; uma decisão sem `rests-on` e outra apoiada numa premissa só assumida: duas perguntas
/// reprovam, cada uma com o seu `next_step`.
pub fn gate_failed() -> Project {
    let project = check_repo(&[]);
    attest_natives(&project);
    let evidence = json!([{ "source": "docs", "summary": "the finance docs ask for it" }]);
    append_wires(
        &project,
        &[
            node_wire(
                DECISION_UNANCHORED,
                "decision",
                json!({ "text": "Split the ledger by region", "work_item": WORK_ITEM, "evidence": evidence }),
            ),
            node_wire(
                DECISION_ON_ASSUMED,
                "decision",
                json!({ "text": "Cap the bonus at ten percent", "work_item": WORK_ITEM, "evidence": evidence }),
            ),
            node_wire(
                "premises.rounding-rule",
                "premise",
                json!({ "text": "Amounts round half up", "kind": "technical", "work_item": WORK_ITEM, "status": "assumed" }),
            ),
        ],
    );
    append_wires(
        &project,
        &[relation_wire(
            "rests-on",
            DECISION_ON_ASSUMED,
            "premises.rounding-rule",
        )],
    );

    project
}

/// Um arquivo solto na árvore: o gate recusa com `snapshot-dirty`.
pub fn gate_dirty() -> Project {
    let project = check_repo(&[]);
    fs::write(project.dir().join("loose-note.txt"), "not committed\n")
        .expect("the loose file is written");

    project
}

/// O gate aprovado e um retro com itens: duas divergências abertas e um gap respondido sem diretiva que o ampare.
pub fn close_with_retro() -> Project {
    let project = gate_approved();
    let divergence = |id: &str, kind: &str| {
        node_wire(
            id,
            "divergence",
            json!({
                "source_ref": "CMP-001", "kind": kind, "text": "the code does more than the spec says",
                "evidence": [{ "source": "docs", "summary": "the docs say so" }],
                "resolution": "open", "work_item": WORK_ITEM,
            }),
        )
    };
    append_wires(
        &project,
        &[
            divergence(DIVERGENCE_PARTIAL, "partial"),
            divergence(DIVERGENCE_MISSING, "missing"),
            node_wire(
                GAP_ANSWERED,
                "gap",
                json!({ "question": "Which calendar?", "context": "payroll", "work_item": WORK_ITEM }),
            ),
            node_wire(
                GAP_ANSWER,
                "premise",
                json!({ "text": "Use the national calendar", "kind": "technical", "work_item": WORK_ITEM, "status": "assumed" }),
            ),
        ],
    );
    append_wires(
        &project,
        &[relation_wire("answers", GAP_ANSWER, GAP_ANSWERED)],
    );

    project
}

// ---------------------------------------------------------------- `check sweep`

/// `check sweep` varre o `/proc` inteiro: quem cria um fugido e conta com ele, ou roda o `check sweep` com sucesso,
/// segura a trava exclusiva de `sweep_lock`, a mesma de `check_sweep_cli` e `skill_examples`. O `check run` só varre a
/// própria raiz, então não precisa de trava. Cada guarda reabre o arquivo: tomar o exclusivo duas vezes na mesma
/// thread trava para sempre.
pub type SweepGuard = Flock<File>;

pub fn stray_guard() -> SweepGuard {
    serialize_sweeps()
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
    /// Quem roda o `check sweep` com sucesso segura a trava exclusiva até o fim do teste.
    pub sweep_guard: Option<SweepGuard>,
}

impl State {
    /// Projeto que o `check sweep` varre: a trava exclusiva fica no estado, porque o `default_formats!` só devolve o estado.
    pub fn swept(project: Project) -> State {
        State {
            project,
            program: None,
            sweep_guard: Some(serialize_sweeps()),
        }
    }

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

    State {
        project,
        program,
        sweep_guard: None,
    }
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

// ---------------------------------------------------------------- `spec validate`

pub const SPEC_PREMISE: &str = "premises.payroll-rule";
pub const SPEC_DIVERGENCE: &str = "divergences.payroll-cmp-001";
const SPEC_FOLDER: &str = ".omhl/specs/wi-payroll__export-reports";

fn confirmed_by_wire() -> Value {
    json!({ "by": "person:ana", "question": "Is this right?", "answer": "Yes, this is right." })
}

fn spec_part(item: u8) -> String {
    format!(
        "## Objetivo\n\nPagar a folha.\n\n## Diretivas e premissas aplicáveis\n\n- {SPEC_PREMISE}\n\n## Comportamentos\n\n- CMP-00{item}: o sistema paga a folha.\n\n## Critérios de aceitação\n\n- CA-00{item}: a folha paga fecha o mês.\n\n## Fora de escopo\n\nDecimo terceiro.\n"
    )
}

fn spec_amendment(divergence: &str, item: u8) -> String {
    format!(
        "## Divergência\n\n{divergence}\n\n## Correções\n\n- EMD-00{item} CMP-001: o sistema paga a folha e registra o pagamento.\n\n## Motivo\n\nA spec omitia o registro do pagamento.\n"
    )
}

fn write_spec_file(project: &Project, name: &str, text: &str) {
    let dir = project.dir().join(SPEC_FOLDER);
    fs::create_dir_all(&dir).expect("the spec folder is created");
    fs::write(dir.join(name), text).expect("the spec file is written");
}

/// Repositório com o work item, a premissa de negócio confirmada e a divergência aberta, sem pasta de spec.
fn spec_graph() -> Project {
    let project = spec_repo();
    append_wires(
        &project,
        &[
            node_wire(
                SPEC_PREMISE,
                "premise",
                json!({
                    "text": "Payroll closes on the last day", "kind": "business", "work_item": WORK_ITEM,
                    "status": "confirmed", "confirmed_by": confirmed_by_wire(),
                }),
            ),
            node_wire(
                SPEC_DIVERGENCE,
                "divergence",
                json!({
                    "source_ref": "CMP-001", "kind": "partial", "text": "the code records the payment and the spec is silent",
                    "evidence": [{ "source": "docs", "summary": "the docs say so" }],
                    "resolution": "open", "work_item": WORK_ITEM,
                }),
            ),
        ],
    );

    project
}

/// Aprova a emenda: nova revisão da divergência em `spec-stale`, apoiada na revisão vigente.
fn approve_amendment(project: &Project, amendment: &str) {
    let current = run_inproc(project, &["--format", "json", "get", SPEC_DIVERGENCE], "");
    let base = serde_json::from_str::<Value>(current.stdout.trim_end())
        .expect("the get prints JSON")["node"]["record_id"]
        .clone();
    let mut revision = node_wire(
        SPEC_DIVERGENCE,
        "divergence",
        json!({
            "source_ref": "CMP-001", "kind": "partial", "text": "the code records the payment and the spec is silent",
            "evidence": [{ "source": "docs", "summary": "the docs say so" }],
            "resolution": "spec-stale", "work_item": WORK_ITEM,
            "confirmed_by": confirmed_by_wire(), "amendment": amendment,
        }),
    );
    revision["base"] = base;
    append_wires(project, &[revision]);
}

/// Uma parte e nenhuma emenda.
pub fn spec_one_part() -> Project {
    let project = spec_graph();
    write_spec_file(&project, "parte-01.md", &spec_part(1));

    project
}

/// Duas partes, uma emenda aprovada e outra pendente com violação (divergência que não existe).
pub fn spec_with_amendments() -> Project {
    let project = spec_graph();
    write_spec_file(&project, "parte-01.md", &spec_part(1));
    write_spec_file(&project, "parte-02.md", &spec_part(2));
    write_spec_file(
        &project,
        "emenda-01.md",
        &spec_amendment(SPEC_DIVERGENCE, 1),
    );
    write_spec_file(
        &project,
        "emenda-02.md",
        &spec_amendment("divergences.ghost-split", 2),
    );
    approve_amendment(&project, "emenda-01");

    project
}

/// Duas partes reprovadas: uma cita um id que não existe, a outra deixa um marcador de trabalho.
pub fn spec_broken() -> Project {
    let project = spec_graph();
    write_spec_file(
        &project,
        "parte-01.md",
        &spec_part(1).replace(SPEC_PREMISE, "premises.ghost-rule"),
    );
    write_spec_file(
        &project,
        "parte-02.md",
        &spec_part(2).replace("Decimo terceiro.", "Decimo terceiro TODO."),
    );

    project
}

// ---------------------------------------------------------------- `load`

/// TOML de um check manual (sem `run`); `after` cria a relação com os checks citados.
fn manual_check(how: &str, after: &[&str]) -> String {
    let after_line = match after {
        [] => String::new(),
        ids => {
            let targets: Vec<String> = ids.iter().map(|id| format!("\"{id}\"")).collect();
            format!("after = [{}]\n", targets.join(", "))
        }
    };

    format!("blocking = true\nhow = \"{how}\"\n{after_line}\n[when]\nevent = \"before-commit\"\n")
}

/// Arquivos novos ainda não carregados: um check e outro que vem depois dele.
pub fn load_fresh() -> Project {
    let project = Project::new("acme");
    let tree = ExtTree::at(project.dir());
    tree.check("payroll-base", &manual_check("Guards the base", &[]));
    tree.check(
        "payroll-extra",
        &manual_check("Guards the extra", &["checks.payroll-base"]),
    );

    project
}

/// Tudo o que o relatório do `load` conta numa execução só: criado, revisado, pulado (aposentado), órfão,
/// relação criada, relação órfã, não mudado e um aviso (arquivo escondido).
pub fn load_full() -> Project {
    let project = Project::new("acme");
    let tree = ExtTree::at(project.dir());
    let keep = "checks.payroll-keep";
    tree.check("payroll-keep", &manual_check("Guards the keep", &[]));
    tree.check(
        "payroll-revise",
        &manual_check("Guards the first draft", &[]),
    );
    tree.check("payroll-retire", &manual_check("Guards the retire", &[]));
    tree.check("payroll-orphan", &manual_check("Guards the orphan", &[]));
    tree.check("payroll-linked", &manual_check("Guards the link", &[keep]));
    run_ok(&project, &["load"], "");

    let checks = project.dir().join(".omhl/checks");
    fs::remove_file(checks.join("payroll-retire.toml")).expect("the file is removed");
    run_ok(&project, &["check", "retire", "checks.payroll-retire"], "");
    tree.check("payroll-retire", &manual_check("Guards the retire", &[]));
    fs::remove_file(checks.join("payroll-orphan.toml")).expect("the file is removed");
    tree.check(
        "payroll-revise",
        &manual_check("Guards the second draft", &[]),
    );
    tree.check("payroll-linked", &manual_check("Guards the link", &[]));
    tree.check("payroll-new", &manual_check("Guards the new", &[keep]));
    tree.file(".omhl/checks/.gitkeep", "");

    project
}

/// Uma diretiva sem `kind`: o `load` recusa e não grava nada.
pub fn load_invalid() -> Project {
    let project = Project::new("acme");
    ExtTree::at(project.dir()).guideline("payroll-tone", "text = \"Be brief\"\n");

    project
}

/// Os seis testes de CA-1 de um comando: `agent` (sem flag), `human` e `json`, no sucesso e no erro.
/// O arquivo que expande define `run(&estado, formato, args)`; um `=> <stdin>` opcional após os
/// argumentos de cada cenário passa a entrada como 4º argumento de `run`.
// Cada arquivo de teste compila `support` inteiro, mas só os `output_agent_cycle_*` expandem a macro.
#[allow(unused_macros)]
macro_rules! default_formats {
    (
        $command:ident,
        $success:expr,
        $success_args:expr
        $(=> $success_stdin:expr)?,
        $failure:expr,
        $failure_args:expr
        $(=> $failure_stdin:expr)?
        $(,)?
    ) => {
        mod $command {
            use super::*;

            #[test]
            fn it_should_print_agent_text_for_the_success_when_no_format_is_given() {
                let state = $success;

                let output = run(&state, None, $success_args $(, $success_stdin)?);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                $crate::support::cycle::assert_text_starting_with(&output.stdout, "✔");
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_human_text_for_the_success() {
                let state = $success;

                let output = run(&state, Some("human"), $success_args $(, $success_stdin)?);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                $crate::support::cycle::assert_text_starting_with(&output.stdout, "✔");
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_a_single_json_line_for_the_success_with_format_json() {
                let state = $success;

                let output = run(&state, Some("json"), $success_args $(, $success_stdin)?);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                $crate::support::cycle::assert_single_json_line(&output.stdout);
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_agent_error_text_when_no_format_is_given() {
                let state = $failure;

                let output = run(&state, None, $failure_args $(, $failure_stdin)?);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                $crate::support::cycle::assert_text_starting_with(&output.stderr, "✘");
                assert_eq!(output.stdout, "");
            }

            #[test]
            fn it_should_print_human_error_text() {
                let state = $failure;

                let output = run(&state, Some("human"), $failure_args $(, $failure_stdin)?);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                $crate::support::cycle::assert_text_starting_with(&output.stderr, "✘");
                assert_eq!(output.stdout, "");
            }

            #[test]
            fn it_should_print_a_single_json_error_line_with_format_json() {
                let state = $failure;

                let output = run(&state, Some("json"), $failure_args $(, $failure_stdin)?);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                $crate::support::cycle::assert_single_json_line(&output.stderr);
                assert_eq!(output.stdout, "");
            }
        }
    };
}

#[allow(unused_imports)]
pub(crate) use default_formats;
