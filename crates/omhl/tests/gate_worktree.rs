mod support;

use std::fs;
use std::path::{Path, PathBuf};

use ::omhl::workflow::config::{RequireAtom, WorkflowConfig};
use ::omhl::workflow::matrix::{PremiseFact, PremiseKind, missing_atoms};
use serde_json::{Value, json};
use support::child::{Output as Child, omhl};
use support::git::git_in;
use support::omhl::work_item_data;
use support::project::Project;
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const LINKED_BRANCH: &str = "feat/wi-w1__login";

// Função pura: a matriz de prefixos

const BUSINESS_PREFIXES: [&str; 2] = ["feat", "fix"];
const TECHNICAL_PREFIXES: [&str; 2] = ["refactor", "tests"];
const NOTHING_PREFIXES: [&str; 2] = ["chore", "docs"];

fn fact(kind: Option<PremiseKind>, confirmed: bool) -> PremiseFact {
    PremiseFact { kind, confirmed }
}

fn pending_business() -> PremiseFact {
    fact(Some(PremiseKind::Business), false)
}

fn confirmed_business() -> PremiseFact {
    fact(Some(PremiseKind::Business), true)
}

/// Premissa técnica sem confirmação humana: a "autônoma" do agente.
fn autonomous_technical() -> PremiseFact {
    fact(Some(PremiseKind::Technical), false)
}

fn confirmed_technical() -> PremiseFact {
    fact(Some(PremiseKind::Technical), true)
}

fn missing(prefix: &str, premises: &[PremiseFact]) -> Option<Vec<RequireAtom>> {
    missing_atoms(prefix, &WorkflowConfig::default(), premises)
}

fn assert_missing_for(prefixes: &[&str], premises: &[PremiseFact], expected: &[RequireAtom]) {
    for prefix in prefixes {
        assert_eq!(
            missing(prefix, premises),
            Some(expected.to_vec()),
            "prefix {prefix}"
        );
    }
}

#[test]
fn it_should_require_a_business_premise_for_feat_and_fix_when_there_is_no_premise() {
    assert_missing_for(&BUSINESS_PREFIXES, &[], &[RequireAtom::BusinessPremise]);
}

#[test]
fn it_should_require_a_business_premise_for_feat_and_fix_when_the_business_premise_is_pending() {
    assert_missing_for(
        &BUSINESS_PREFIXES,
        &[pending_business()],
        &[RequireAtom::BusinessPremise],
    );
}

#[test]
fn it_should_accept_feat_and_fix_with_a_confirmed_business_premise() {
    assert_missing_for(&BUSINESS_PREFIXES, &[confirmed_business()], &[]);
}

#[test]
fn it_should_not_accept_a_technical_premise_for_feat_and_fix() {
    assert_missing_for(
        &BUSINESS_PREFIXES,
        &[autonomous_technical(), confirmed_technical()],
        &[RequireAtom::BusinessPremise],
    );
}

#[test]
fn it_should_not_report_the_spec_atom_as_missing_for_feat_and_fix() {
    let missing_for_feat = missing("feat", &[]);

    assert_eq!(missing_for_feat, Some(vec![RequireAtom::BusinessPremise]));
}

#[test]
fn it_should_require_a_confirmed_technical_premise_for_refactor_and_tests_when_there_is_no_premise()
{
    assert_missing_for(
        &TECHNICAL_PREFIXES,
        &[],
        &[RequireAtom::TechnicalPremiseConfirmed],
    );
}

#[test]
fn it_should_not_accept_an_autonomous_technical_premise_for_refactor_and_tests() {
    assert_missing_for(
        &TECHNICAL_PREFIXES,
        &[autonomous_technical()],
        &[RequireAtom::TechnicalPremiseConfirmed],
    );
}

#[test]
fn it_should_not_accept_a_business_premise_for_refactor_and_tests() {
    assert_missing_for(
        &TECHNICAL_PREFIXES,
        &[pending_business(), confirmed_business()],
        &[RequireAtom::TechnicalPremiseConfirmed],
    );
}

#[test]
fn it_should_accept_refactor_and_tests_with_a_confirmed_technical_premise() {
    assert_missing_for(&TECHNICAL_PREFIXES, &[confirmed_technical()], &[]);
}

#[test]
fn it_should_require_an_agent_premise_for_bugfix_when_there_is_no_premise() {
    assert_missing_for(&["bugfix"], &[], &[RequireAtom::AgentPremise]);
}

#[test]
fn it_should_not_accept_a_business_premise_for_bugfix() {
    assert_missing_for(
        &["bugfix"],
        &[pending_business(), confirmed_business()],
        &[RequireAtom::AgentPremise],
    );
}

#[test]
fn it_should_accept_bugfix_with_an_autonomous_technical_premise() {
    assert_missing_for(&["bugfix"], &[autonomous_technical()], &[]);
}

#[test]
fn it_should_accept_bugfix_with_a_technical_premise_the_human_confirmed_later() {
    assert_missing_for(&["bugfix"], &[confirmed_technical()], &[]);
}

#[test]
fn it_should_require_nothing_for_chore_and_docs_whatever_the_premises() {
    let states: [Vec<PremiseFact>; 5] = [
        vec![],
        vec![pending_business()],
        vec![confirmed_business()],
        vec![autonomous_technical()],
        vec![confirmed_technical()],
    ];

    for premises in &states {
        assert_missing_for(&NOTHING_PREFIXES, premises, &[]);
    }
}

#[test]
fn it_should_count_a_premise_without_kind_for_no_atom() {
    let untyped = [fact(None, true), fact(None, false)];

    assert_missing_for(
        &BUSINESS_PREFIXES,
        &untyped,
        &[RequireAtom::BusinessPremise],
    );
    assert_missing_for(
        &TECHNICAL_PREFIXES,
        &untyped,
        &[RequireAtom::TechnicalPremiseConfirmed],
    );
    assert_missing_for(&["bugfix"], &untyped, &[RequireAtom::AgentPremise]);
}

#[test]
fn it_should_accept_when_one_of_several_premises_satisfies_the_atom() {
    let premises = [
        pending_business(),
        autonomous_technical(),
        confirmed_business(),
    ];

    assert_missing_for(&BUSINESS_PREFIXES, &premises, &[]);
}

#[test]
fn it_should_give_none_for_a_prefix_outside_the_table() {
    let outside = missing("perf", &[confirmed_business()]);

    assert_eq!(outside, None);
}

#[test]
fn it_should_not_report_the_spec_atom_even_when_it_is_the_only_one_required() {
    let mut config = WorkflowConfig::default();
    config
        .prefixes
        .insert("feat".to_owned(), vec![RequireAtom::Spec]);

    let missing_for_feat = missing_atoms("feat", &config, &[]);

    assert_eq!(missing_for_feat, Some(vec![]));
}

#[test]
fn it_should_report_the_missing_atoms_in_the_order_of_the_require_list() {
    let mut config = WorkflowConfig::default();
    config.prefixes.insert(
        "perf".to_owned(),
        vec![
            RequireAtom::AgentPremise,
            RequireAtom::Spec,
            RequireAtom::BusinessPremise,
        ],
    );

    let missing_for_perf = missing_atoms("perf", &config, &[]);

    assert_eq!(
        missing_for_perf,
        Some(vec![
            RequireAtom::AgentPremise,
            RequireAtom::BusinessPremise
        ])
    );
}

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
            let output = git_in(project.dir(), args);
            assert!(output.status.success(), "git {args:?} failed");
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

    fn git(&self, args: &[&str]) {
        let output = git_in(self.project.dir(), args);

        assert!(output.status.success(), "git {args:?} failed");
    }

    fn main_dir(&self) -> String {
        fs::canonicalize(self.project.dir())
            .expect("the main directory exists")
            .to_string_lossy()
            .into_owned()
    }
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

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_code(output: &Child) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
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
    fixture.git(&["branch", "feat/wi-w1__nowt"]);

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

// Matriz sobre o banco

#[test]
fn it_should_refuse_feat_without_a_premise_with_worktree_gate_refused() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("feat/wi-w1__new");

    assert_refused(&output, "worktree-gate-refused");
}

#[test]
fn it_should_list_the_missing_atom_in_the_context_of_the_refusal() {
    let fixture = Fixture::seeded();

    let output = fixture.gate("feat/wi-w1__new");

    assert_eq!(
        error_context(&output)["missing"],
        json!(["business-premise"])
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
        "version = 1\n\n[prefix.feat]\nrequire = [\"agent-premise\", \"business-premise\", \"technical-premise-confirmed\"]\n",
    );

    let output = fixture.gate("feat/wi-w1__new");

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
        "version = 1\n\n[prefix.feat]\nrequire = [\"business-premise\", \"agent-premise\"]\n",
    );
    fixture.premise("premises.b1", W1, "business", true);

    let output = fixture.gate("feat/wi-w1__new");

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
fn it_should_allow_feat_with_a_confirmed_business_premise_and_no_spec() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);

    let output = fixture.gate("feat/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
}

#[test]
fn it_should_allow_fix_with_a_confirmed_business_premise_and_no_spec() {
    let fixture = Fixture::seeded();
    fixture.premise("premises.b1", W1, "business", true);

    let output = fixture.gate("fix/wi-w1__new");

    assert_allowed(&output, "matrix-satisfied");
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
