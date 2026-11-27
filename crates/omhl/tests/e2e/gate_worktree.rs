use crate::support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::child::{Output as Child, omhl};
use support::git::{git_in, git_ok, write_executable};
use support::legacy::seed_legacy_confirmed_premise;
use support::omhl::{project_dir_refusal, work_item_data};
use support::project::Project;
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const LINKED_BRANCH: &str = "feat/wi-w1__login";
const SPEC_SLUG: &str = "login";

// CLI: repositório git real

/// Repositório git real (`main`, marcador commitado) com uma worktree ligada em `LINKED_BRANCH`.
/// O banco só existe depois do primeiro `append`.
struct Fixture {
    project: Project,
    scratch: TempDir,
    linked: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let project = Project::new("acme");
        for args in [
            &["init", "-b", "main"][..],
            &["config", "user.name", "Test Author"],
            &["config", "user.email", "author@example.com"],
            &["add", "-A"],
            &["commit", "-m", "add marker"],
        ] {
            git_ok(project.dir(), args);
        }
        let scratch = TempDir::new().expect("scratch directory is created");
        let linked = scratch.path().join("wt");
        let added = git_in(
            project.dir(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                LINKED_BRANCH,
                &linked.to_string_lossy(),
            ],
        );
        assert!(added.status.success(), "git worktree add failed");

        Fixture {
            project,
            scratch,
            linked,
        }
    }

    /// Como `new`, com os work items `w1` e `w2` abertos no banco.
    fn seeded() -> Fixture {
        let fixture = Fixture::new();
        let items: Vec<Value> = [W1, W2]
            .iter()
            .map(|id| node_item(id, "work-item", &work_item_data("t")))
            .collect();
        fixture.append(&items);

        fixture
    }

    fn close_work_item(&self, work_item: &str) {
        let closed = self.run_in(self.project.dir(), &["close", work_item]);

        assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    }

    /// Como `seeded`, com o `W1` já fechado: o `close` passa com a premissa e a spec que o tipo exige.
    fn seeded_closed() -> Fixture {
        let fixture = Fixture::seeded();
        fixture.premise("premises.b1", W1, "business", true);
        fixture.write_valid_spec(W1, "premises.b1");
        fixture.close_work_item(W1);

        fixture
    }

    fn run_in(&self, cwd: &Path, args: &[&str]) -> Child {
        omhl(args).cwd(cwd).xdg(self.project.xdg()).run()
    }

    fn gate(&self, branch: &str) -> Child {
        self.run_in(self.project.dir(), &["gate-worktree", "--branch", branch])
    }

    fn gate_from_linked(&self, branch: &str) -> Child {
        self.run_in(&self.linked, &["gate-worktree", "--branch", branch])
    }

    /// Caminho de banco que ainda não existe: depois de um veredito que não consulta o banco, continua não existindo.
    fn absent_db(&self) -> PathBuf {
        self.scratch.path().join("absent").join("omhl.db")
    }

    fn gate_with_absent_db(&self, branch: &str) -> Child {
        let db = self.absent_db();
        let db = db.to_string_lossy();

        self.run_in(
            self.project.dir(),
            &["--db", &db, "gate-worktree", "--branch", branch],
        )
    }

    fn append(&self, items: &[Value]) {
        let appended = omhl(&["append"])
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .stdin(json!({ "items": items }).to_string())
            .run();

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }

    fn premise(&self, id: &str, work_item: &str, kind: &str, confirmed: bool) {
        self.append(&[premise_item(id, work_item, kind, confirmed)]);
    }

    fn write_workflow(&self, toml: &str) {
        let path = self.project.dir().join(".omhl/workflow.toml");
        fs::create_dir_all(path.parent().expect("the file has a parent")).expect("parent");
        fs::write(path, toml).expect("workflow.toml is written");
    }

    /// A versão do `workflow.toml` que está na worktree ligada (a da branch), não no diretório principal.
    fn write_linked_workflow(&self, toml: &str) {
        let path = self.linked.join(".omhl/workflow.toml");
        fs::create_dir_all(path.parent().expect("the file has a parent")).expect("parent");
        fs::write(path, toml).expect("workflow.toml is written");
    }

    fn main_dir(&self) -> String {
        fs::canonicalize(self.project.dir())
            .expect("the main directory exists")
            .to_string_lossy()
            .into_owned()
    }

    /// `omhl spec init`: devolve o caminho da primeira parte, ainda com as 5 seções vazias.
    fn init_spec(&self, work_item: &str) -> PathBuf {
        let output = self.run_in(
            self.project.dir(),
            &["--format", "json", "spec", "init", work_item, SPEC_SLUG],
        );
        assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

        PathBuf::from(
            json_line(&output.stdout)["path"]
                .as_str()
                .expect("spec init prints the path"),
        )
    }

    /// Spec criada por `omhl spec init` e preenchida com as 5 seções, citando `cited`.
    fn write_spec(&self, work_item: &str, cited: &str) {
        let path = self.init_spec(work_item);

        fs::write(path, valid_spec(cited)).expect("the spec part is written");
    }

    /// Como `write_spec`, e confere pela CLI que o validador a aprova: o gate não é o único juiz.
    fn write_valid_spec(&self, work_item: &str, cited: &str) {
        self.write_spec(work_item, cited);

        let validated = self.run_in(self.project.dir(), &["spec", "validate", work_item]);
        assert_eq!(validated.code, Some(0), "stderr: {}", validated.stderr);
    }
}

fn valid_spec(cited: &str) -> String {
    format!(
        "## Objetivo\n\nPermitir o login.\n\n## Diretivas e premissas aplicáveis\n\n- {cited}\n\n## Comportamentos\n\n- CMP-001: o sistema autentica o usuário.\n\n## Critérios de aceitação\n\n- CA-001: login válido devolve sessão.\n\n## Fora de escopo\n\nLogout.\n"
    )
}

fn node_item(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "body": { "kind": "node", "id": id, "type": type_name, "data": data } })
}

fn premise_item(id: &str, work_item: &str, kind: &str, confirmed: bool) -> Value {
    let mut data = json!({
        "text": "a premise",
        "status": if confirmed { "confirmed" } else { "assumed" },
        "kind": kind,
        "work_item": work_item,
    });
    if confirmed {
        data["confirmed_by"] =
            json!({ "by": "person:tester", "question": "Adopt it?", "answer": "Yes." });
    }

    node_item(id, "premise", &data)
}

fn late_premise_item(id: &str, work_item: &str) -> Value {
    let mut item = premise_item(id, work_item, "business", true);
    item["body"]["data"]["origin"] = json!("late");

    item
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_code(output: &Child) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
}

fn error_pointer(output: &Child) -> Value {
    json_line(&output.stderr)["error"]["details"][0]["path"].clone()
}

fn error_context(output: &Child) -> Value {
    json_line(&output.stderr)["error"]["details"][0]["context"].clone()
}

fn assert_allowed(output: &Child, reason: &str) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    let verdict = json_line(&output.stdout);
    assert_eq!(verdict["allowed"], json!(true));
    assert_eq!(verdict["reason"], json!(reason));
}

fn assert_allowed_without_db(fixture: &Fixture, output: &Child, reason: &str) {
    assert_allowed(output, reason);
    assert!(
        !fixture.absent_db().exists(),
        "the database file is not created"
    );
}

fn assert_refused(output: &Child, code: &str) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(output), json!(code));
}

// Ordem de resolução: o que sai 0 antes de aplicar a matriz

#[test]
fn it_should_exit_zero_in_silence_when_there_is_no_marker_above_the_directory() {
    let dir = TempDir::new().expect("directory is created");
    let scratch = TempDir::new().expect("scratch directory is created");
    let db = scratch.path().join("absent").join("omhl.db");

    let output = omhl(&[
        "--db",
        &db.to_string_lossy(),
        "gate-worktree",
        "--branch",
        "feat/wi-w1__x",
    ])
    .cwd(dir.path())
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert!(output.stderr.is_empty(), "stderr stays empty");
}

#[test]
fn it_should_refuse_a_project_dir_that_does_not_exist() {
    let dir = TempDir::new().expect("directory is created");
    let missing = dir.path().join("missing");

    let output = omhl(&[
        "--project-dir",
        &missing.to_string_lossy(),
        "gate-worktree",
        "--branch",
        "feat/wi-w1__x",
    ])
    .run();

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(json_line(&output.stderr), project_dir_refusal());
}

#[test]
fn it_should_stay_silent_for_an_existing_dir_without_marker() {
    let dir = TempDir::new().expect("directory is created");

    let output = omhl(&[
        "--project-dir",
        &dir.path().to_string_lossy(),
        "gate-worktree",
        "--branch",
        "feat/wi-w1__x",
    ])
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert!(output.stderr.is_empty(), "stderr stays empty");
}

#[test]
fn it_should_not_create_the_database_when_there_is_no_marker() {
    let dir = TempDir::new().expect("directory is created");
    let scratch = TempDir::new().expect("scratch directory is created");
    let db = scratch.path().join("absent").join("omhl.db");

    let output = omhl(&[
        "--db",
        &db.to_string_lossy(),
        "gate-worktree",
        "--branch",
        "feat/wi-w1__x",
    ])
    .cwd(dir.path())
    .run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(!db.exists(), "the database file is not created");
}

#[test]
fn it_should_allow_develop_without_looking_at_the_branch_name_pattern() {
    let fixture = Fixture::new();

    let output = fixture.gate_with_absent_db("develop");

    assert_allowed_without_db(&fixture, &output, "exempt-branch");
}

#[test]
fn it_should_allow_main_without_looking_at_the_branch_name_pattern() {
    let fixture = Fixture::new();

    let output = fixture.gate_with_absent_db("main");

    assert_allowed_without_db(&fixture, &output, "exempt-branch");
}

#[test]
fn it_should_allow_a_local_branch_that_already_has_a_worktree() {
    let fixture = Fixture::new();

    let output = fixture.gate_with_absent_db(LINKED_BRANCH);

    assert_allowed_without_db(&fixture, &output, "worktree-exists");
}

#[test]
fn it_should_allow_a_branch_that_has_a_worktree_when_run_from_that_worktree() {
    let fixture = Fixture::new();

    let output = fixture.gate_from_linked(LINKED_BRANCH);

    assert_allowed(&output, "worktree-exists");
}

#[test]
fn it_should_apply_the_matrix_to_a_local_branch_that_has_no_worktree() {
    let fixture = Fixture::seeded();
    git_ok(fixture.project.dir(), &["branch", "feat/wi-w1__nowt"]);

    let output = fixture.gate("feat/wi-w1__nowt");

    assert_refused(&output, "worktree-gate-refused");
}

// Formato do nome e prefixo fora da tabela

#[test]
fn it_should_refuse_a_prefix_outside_the_table_with_prefix_not_allowed() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("perf/wi-w1__x");

    assert_refused(&output, "prefix-not-allowed");
}

#[test]
fn it_should_refuse_a_name_outside_the_type_wi_id_slug_pattern_with_prefix_not_allowed() {
    let fixture = Fixture::seeded();
    let names = [
        "feat/x",
        "feat/wi-w1",
        "feat/wi-w1__",
        "wi-w1__x",
        "feat/wi-W1__x",
        "x",
    ];

    let codes: Vec<Value> = names
        .iter()
        .map(|name| error_code(&fixture.gate(name)))
        .collect();

    assert_eq!(codes, vec![json!("prefix-not-allowed"); names.len()]);
}

#[test]
fn it_should_pass_a_branch_with_a_leading_hyphen_to_omhl_as_a_separate_value() {
    let fixture = Fixture::seeded();

    let output = fixture.run_in(fixture.project.dir(), &["gate-worktree", "--branch", "-x"]);

    assert_refused(&output, "prefix-not-allowed");
}

#[test]
fn it_should_pass_a_branch_with_a_leading_hyphen_to_omhl_in_the_equals_form() {
    let fixture = Fixture::seeded();

    let output = fixture.run_in(fixture.project.dir(), &["gate-worktree", "--branch=-x"]);

    assert_refused(&output, "prefix-not-allowed");
}

#[test]
fn it_should_refuse_a_pr_reference_with_prefix_not_allowed() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("pr:7");

    assert_refused(&output, "prefix-not-allowed");
}

#[test]
fn it_should_not_execute_a_command_substitution_given_as_the_branch_name() {
    let fixture = Fixture::seeded();
    let canary = fixture.scratch.path().join("pwned-substitution");
    let name = format!("$(touch {})", canary.display());

    let output = fixture.gate(&name);

    assert_refused(&output, "prefix-not-allowed");
    assert!(!canary.exists(), "nothing was executed");
}

#[test]
fn it_should_not_execute_a_command_after_a_semicolon_given_as_the_branch_name() {
    let fixture = Fixture::seeded();
    let canary = fixture.scratch.path().join("pwned-semicolon");
    let name = format!("a;touch {}", canary.display());

    let output = fixture.gate(&name);

    assert_refused(&output, "prefix-not-allowed");
    assert!(!canary.exists(), "nothing was executed");
}

#[test]
fn it_should_not_execute_a_command_substitution_inside_a_well_formed_branch_name() {
    let fixture = Fixture::seeded();
    let canary = fixture.scratch.path().join("pwned-slug");
    let name = format!("feat/wi-w1__$(touch {})", canary.display());

    let output = fixture.gate(&name);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(!canary.exists(), "nothing was executed");
}

#[test]
fn it_should_not_echo_the_rejected_branch_name_in_the_error() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("Canary/wi-w1__x");

    assert_refused(&output, "prefix-not-allowed");
    assert!(!output.stderr.contains("Canary"));
}

#[test]
fn it_should_refuse_a_branch_name_with_shell_metacharacters() {
    let fixture = Fixture::seeded();
    let canary = fixture.scratch.path().join("pwned-metacharacters");
    let shown = canary.display();
    let names = [
        format!("feat/wi-`touch {shown}`__x"),
        format!("feat/wi-w1|touch {shown}__x"),
        format!("feat/wi-w1&&touch {shown}__x"),
        format!("feat/wi-w1>{shown}__x"),
    ];

    for name in names {
        let output = fixture.gate(&name);

        assert_refused(&output, "prefix-not-allowed");
    }
    assert!(!canary.exists(), "nothing was executed");
}

#[test]
fn it_should_answer_main_dir_unknown_when_git_cannot_find_the_main_directory() {
    let fixture = Fixture::seeded();
    let fakebin = fixture.scratch.path().join("fakebin");
    fs::create_dir_all(&fakebin).expect("fakebin is created");
    let inherited = std::env::var_os("PATH").expect("the test process has a PATH");
    let real_git = std::env::split_paths(&inherited)
        .map(|dir| dir.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git is on the PATH");
    let script = format!(
        "#!/bin/sh\nfor arg in \"$@\"; do [ \"$arg\" = --git-common-dir ] && exit 1; done\nexec {} \"$@\"\n",
        real_git.display()
    );
    write_executable(&fakebin.join("git"), &script);
    let path =
        std::env::join_paths(std::iter::once(fakebin).chain(std::env::split_paths(&inherited)))
            .expect("PATH is joined");

    let output = omhl(&["gate-worktree", "--branch", "feat/wi-w1__new"])
        .cwd(fixture.project.dir())
        .xdg(fixture.project.xdg())
        .env("PATH", path)
        .run();

    assert_refused(&output, "main-dir-unknown");
}

// Matriz sobre o banco

#[test]
fn it_should_refuse_feat_without_a_premise_with_worktree_gate_refused() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

#[test]
fn it_should_list_the_premise_and_the_spec_atoms_in_the_context_when_feat_has_neither() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("feat/wi-w1__new");

    assert_eq!(
        error_context(&output)["missing"],
        json!(["business-premise", "spec"])
    );
}

#[test]
fn it_should_name_the_branch_prefix_and_work_item_in_the_context_of_the_refusal() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("feat/wi-w1__new");

    let context = error_context(&output);
    assert_eq!(context["kind"], json!("worktree-gate-refused"));
    assert_eq!(context["branch"], json!("feat/wi-w1__new"));
    assert_eq!(context["prefix"], json!("feat"));
    assert_eq!(context["work_item"], json!(W1));
}

#[test]
fn it_should_name_the_main_directory_in_the_refusal_when_run_from_the_main_directory() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("feat/wi-w1__new");

    assert_eq!(
        error_context(&output)["main_dir"],
        json!(fixture.main_dir())
    );
}

#[test]
fn it_should_name_the_main_directory_in_the_refusal_when_run_from_a_linked_worktree() {
    let fixture = Fixture::seeded();

    let output = fixture.gate_from_linked("feat/wi-w1__new");

    assert_eq!(
        error_context(&output)["main_dir"],
        json!(fixture.main_dir())
    );
}

#[test]
fn it_should_list_every_missing_atom_in_the_order_of_the_require_list() {
    let fixture = Fixture::seeded();
    fixture.write_workflow(
        "version = 1\n\n[prefix.chore]\nrequire = [\"agent-premise\", \"business-premise\", \"technical-premise-confirmed\"]\n",
    );

    let output = fixture.gate("chore/wi-w1__new");

    assert_eq!(
        error_context(&output)["missing"],
        json!([
            "agent-premise",
            "business-premise",
            "technical-premise-confirmed"
        ])
    );
}

#[test]
fn it_should_list_only_the_atoms_that_are_still_missing() {
    let fixture = Fixture::seeded();
    fixture.write_workflow(
        "version = 1\n\n[prefix.chore]\nrequire = [\"business-premise\", \"agent-premise\"]\n",
    );
    fixture.premise("premises.b1", W1, "business", true);

    let output = fixture.gate("chore/wi-w1__new");

    assert_eq!(error_context(&output)["missing"], json!(["agent-premise"]));
}

#[test]
fn it_should_refuse_feat_with_a_pending_business_premise() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", false);

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

#[test]
fn it_should_refuse_feat_with_a_confirmed_business_premise_and_no_spec_listing_only_spec() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["spec"]));
}

#[test]
fn it_should_refuse_fix_with_a_confirmed_business_premise_and_no_spec_listing_only_spec() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);

    let output = fixture.gate("fix/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["spec"]));
}

#[test]
fn it_should_not_count_a_confirmed_business_premise_of_another_work_item() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b2", W2, "business", true);

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

#[test]
fn it_should_not_count_a_technical_premise_for_feat() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.t1", W1, "technical", true);

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

#[test]
fn it_should_refuse_refactor_with_an_autonomous_technical_premise() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.t1", W1, "technical", false);

    let output = fixture.gate("refactor/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

#[test]
fn it_should_refuse_refactor_with_a_legacy_confirmed_technical_premise() {
    let fixture = Fixture::seeded();
    seed_legacy_confirmed_premise(&fixture.project.db_path(), "premises.t1", W1, "technical");

    let output = fixture.gate("refactor/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

#[test]
fn it_should_allow_refactor_with_a_confirmed_technical_premise() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.t1", W1, "technical", true);

    let output = fixture.gate("refactor/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_allow_tests_with_a_confirmed_technical_premise() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.t1", W1, "technical", true);

    let output = fixture.gate("tests/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_refuse_bugfix_without_a_premise() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("bugfix/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["agent-premise"]));
}

#[test]
fn it_should_refuse_bugfix_with_only_a_business_premise() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);

    let output = fixture.gate("bugfix/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

#[test]
fn it_should_allow_bugfix_with_an_autonomous_technical_premise() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.t1", W1, "technical", false);

    let output = fixture.gate("bugfix/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_not_count_an_autonomous_technical_premise_of_another_work_item_for_bugfix() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.t2", W2, "technical", false);

    let output = fixture.gate("bugfix/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

// chore e docs: nada, e o banco nem abre

#[test]
fn it_should_allow_chore_without_any_premise_and_without_opening_the_database() {
    let fixture = Fixture::new();

    let output = fixture.gate_with_absent_db("chore/wi-w1__new");

    assert_allowed_without_db(&fixture, &output, "matrix-satisfied");
}

#[test]
fn it_should_allow_docs_without_any_premise_and_without_opening_the_database() {
    let fixture = Fixture::new();

    let output = fixture.gate_with_absent_db("docs/wi-w1__new");

    assert_allowed_without_db(&fixture, &output, "matrix-satisfied");
}

// Prefixo vindo do workflow.toml

#[test]
fn it_should_respect_a_new_prefix_declared_in_workflow_toml() {
    let fixture = Fixture::seeded();
    fixture.write_workflow("version = 1\n\n[prefix.perf]\nrequire = [\"agent-premise\"]\n");

    let output = fixture.gate("perf/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["agent-premise"]));
}

#[test]
fn it_should_allow_a_new_prefix_from_workflow_toml_once_its_require_is_met() {
    let fixture = Fixture::seeded();
    fixture.write_workflow("version = 1\n\n[prefix.perf]\nrequire = [\"agent-premise\"]\n");
    fixture.premise("premises.t1", W1, "technical", false);

    let output = fixture.gate("perf/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_keep_refusing_a_prefix_that_workflow_toml_does_not_declare() {
    let fixture = Fixture::seeded();
    fixture.write_workflow("version = 1\n\n[prefix.perf]\nrequire = []\n");

    let output = fixture.gate("style/wi-w1__new");

    assert_refused(&output, "prefix-not-allowed");
}

#[test]
fn it_should_read_the_workflow_file_of_the_main_directory_when_run_from_a_linked_worktree() {
    let fixture = Fixture::seeded();
    fixture.write_workflow("version = 1\n\n[prefix.perf]\nrequire = [\"agent-premise\"]\n");
    fixture.write_linked_workflow("version = 1\n\n[prefix.perf]\nrequire = []\n");

    let output = fixture.gate_from_linked("perf/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["agent-premise"]));
}

// Átomo spec: sem spec, spec inválida e spec válida

#[test]
fn it_should_list_the_premise_and_the_spec_atoms_when_feat_has_a_pending_business_premise_and_no_spec()
 {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", false);

    let output = fixture.gate("feat/wi-w1__new");

    assert_eq!(
        error_context(&output)["missing"],
        json!(["business-premise", "spec"])
    );
}

#[test]
fn it_should_list_the_premise_and_the_spec_atoms_when_feat_has_an_autonomous_technical_premise_and_no_spec()
 {
    let fixture = Fixture::seeded();
    fixture.premise("premises.t1", W1, "technical", false);

    let output = fixture.gate("feat/wi-w1__new");

    assert_eq!(
        error_context(&output)["missing"],
        json!(["business-premise", "spec"])
    );
}

#[test]
fn it_should_refuse_feat_when_the_spec_is_the_empty_skeleton_created_by_spec_init() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.init_spec(W1);

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["spec"]));
}

fn assert_spec_store_unusable(output: &Child) {
    assert_refused(output, "spec-store-unusable");
    assert_eq!(
        json_line(&output.stderr)["error"]["details"][0]["path"],
        json!("/work_item")
    );
}

#[test]
fn it_should_refuse_with_spec_store_unusable_when_a_part_of_the_spec_is_a_directory() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    let part = fixture.init_spec(W1);
    fs::remove_file(&part).expect("the part exists");
    fs::create_dir(&part).expect("directory takes the part name");

    let output = fixture.gate("feat/wi-w1__new");

    assert_spec_store_unusable(&output);
}

#[test]
fn it_should_refuse_with_spec_store_unusable_when_the_work_item_has_two_spec_folders() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    let part = fixture.init_spec(W1);
    let other = part
        .parent()
        .expect("the part has a folder")
        .with_file_name("wi-w1__other");
    fs::create_dir(&other).expect("second spec directory is created");

    let output = fixture.gate("feat/wi-w1__new");

    assert_spec_store_unusable(&output);
}

#[test]
fn it_should_refuse_feat_when_the_spec_cites_a_premise_that_is_not_in_the_database() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.write_spec(W1, "premises.ghost");

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["spec"]));
}

#[test]
fn it_should_allow_feat_with_a_confirmed_business_premise_and_a_valid_spec() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.write_valid_spec(W1, "premises.b1");

    let output = fixture.gate("feat/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_refuse_feat_when_the_only_confirmed_business_premise_is_late() {
    let fixture = Fixture::seeded();
    fixture.append(&[late_premise_item("premises.late", W1)]);
    fixture.write_spec(W1, "premises.late");

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(
        error_context(&output)["missing"],
        json!(["business-premise", "spec"])
    );
}

#[test]
fn it_should_allow_feat_with_a_main_business_premise_next_to_a_late_one() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.append(&[late_premise_item("premises.late", W1)]);
    fixture.write_valid_spec(W1, "premises.b1");

    let output = fixture.gate("feat/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_allow_fix_with_a_confirmed_business_premise_and_a_valid_spec() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.write_valid_spec(W1, "premises.b1");

    let output = fixture.gate("fix/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_find_the_valid_spec_in_the_main_directory_when_run_from_a_linked_worktree() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.write_valid_spec(W1, "premises.b1");

    let output = fixture.gate_from_linked("feat/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_not_count_the_valid_spec_of_another_work_item() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.premise("premises.b2", W2, "business", true);
    fixture.write_valid_spec(W2, "premises.b2");

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["spec"]));
}

#[test]
fn it_should_list_only_the_premise_atom_when_the_spec_is_valid_but_the_premise_is_missing() {
    let fixture = Fixture::seeded();
    fixture.write_workflow("version = 1\n\n[specs]\ncite_business_premise = false\n");
    fixture.premise("premises.t1", W1, "technical", true);
    fixture.write_valid_spec(W1, "premises.t1");

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(
        error_context(&output)["missing"],
        json!(["business-premise"])
    );
}

#[test]
fn it_should_allow_refactor_with_a_confirmed_technical_premise_whatever_the_spec_is() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.t1", W1, "technical", true);
    fixture.init_spec(W1);

    let output = fixture.gate("refactor/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_keep_refusing_perf_with_prefix_not_allowed_even_with_a_premise_and_a_valid_spec() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.write_valid_spec(W1, "premises.b1");

    let output = fixture.gate("perf/wi-w1__new");

    assert_refused(&output, "prefix-not-allowed");
}

// Prefixo novo do workflow.toml com require = ["spec"]

const SPEC_ONLY_PERF: &str = "version = 1\n\n[prefix.perf]\nrequire = [\"spec\"]\n";

#[test]
fn it_should_require_a_spec_for_a_new_prefix_whose_require_is_only_spec() {
    let fixture = Fixture::seeded();
    fixture.write_workflow(SPEC_ONLY_PERF);

    let output = fixture.gate("perf/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["spec"]));
}

#[test]
fn it_should_refuse_a_new_prefix_whose_require_is_only_spec_when_the_spec_is_invalid() {
    let fixture = Fixture::seeded();
    fixture.write_workflow(SPEC_ONLY_PERF);
    fixture.init_spec(W1);

    let output = fixture.gate("perf/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
    assert_eq!(error_context(&output)["missing"], json!(["spec"]));
}

#[test]
fn it_should_allow_a_new_prefix_whose_require_is_only_spec_once_the_spec_is_valid() {
    let fixture = Fixture::seeded();
    fixture.write_workflow(SPEC_ONLY_PERF);
    fixture.premise("premises.b1", W1, "business", true);
    fixture.write_valid_spec(W1, "premises.b1");

    let output = fixture.gate("perf/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

// Roteiro 7.3, passos 1, 3 e 5

#[test]
fn it_should_walk_the_gate_from_refusal_without_premise_to_release_with_a_valid_spec() {
    let fixture = Fixture::new();
    fixture.append(&[node_item(W1, "work-item", &work_item_data("t"))]);

    let without_premise = fixture.gate("feat/wi-w1__new");
    fixture.premise("premises.b1", W1, "business", true);
    let without_spec = fixture.gate("feat/wi-w1__new");
    fixture.write_valid_spec(W1, "premises.b1");
    let with_spec = fixture.gate("feat/wi-w1__new");

    assert_refused(&without_premise, "worktree-gate-refused");
    assert_eq!(
        error_context(&without_premise)["missing"],
        json!(["business-premise", "spec"])
    );
    assert_refused(&without_spec, "worktree-gate-refused");
    assert_eq!(error_context(&without_spec)["missing"], json!(["spec"]));
    assert_allowed(&with_spec, "matrix-satisfied");
}

// Status do work item: o gate recusa work item fechado ou inexistente

#[test]
fn it_should_refuse_a_closed_work_item_even_with_a_confirmed_premise_and_a_valid_spec() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);
    fixture.write_valid_spec(W1, "premises.b1");
    fixture.close_work_item(W1);

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "work-item-closed");
    assert_eq!(error_pointer(&output), json!("/branch"));
}

#[test]
fn it_should_refuse_feat_of_a_work_item_that_is_not_in_the_database_with_work_item_not_found() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("feat/wi-ghost__new");

    assert_refused(&output, "work-item-not-found");
    assert_eq!(error_pointer(&output), json!("/branch"));
}

#[test]
fn it_should_allow_chore_of_a_closed_work_item_because_its_require_never_opens_the_database() {
    let fixture = Fixture::seeded_closed();

    let output = fixture.gate("chore/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

// CA-002: `require = []` num tipo padrão não tira o átomo de negócio do `feat`
#[test]
fn it_should_still_refuse_feat_with_an_empty_require_list_without_a_business_premise() {
    let fixture = Fixture::seeded();
    fixture.write_workflow("version = 1\n\n[prefix.feat]\nrequire = []\n");

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}
