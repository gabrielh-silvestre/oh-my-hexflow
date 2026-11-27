use crate::support;

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use support::child::{Output, omhl, omhl_json};
use support::git::{git_in, git_ok};
use support::omhl::{PR_7, closed_with_gate, pull_request_data, work_item_data};
use support::project::Project;

const W1: &str = "work-items.w1";
const PART_DIR: &str = "wi-w1__login";
const BUSINESS_PREMISE: &str = "premises.login-rule";
const DIVERGENCE: &str = "divergences.login-cmp-001";
const OTHER_DIVERGENCE: &str = "divergences.login-cmp-002";
const GAP: &str = "gaps.g1";

const ANSWER: &str = "premises.p1";
const DIRECTIVE: &str = "guidelines.d1";
const CANARY: &str = "CANARY-8f3a1c";
const MISSING_WORK_ITEM: &str = "work-items.ghost";
// Sem a linha do prefixo `chore`, o tipo gravado do W1 deixa de estar na tabela.
const FEAT_ONLY_WORKFLOW: &str = "version = 1\n\n[prefix.feat]\nrequire = []\n";

// Repositório git real: `omhl close` e `omhl report retro`

fn text(lines: &[impl AsRef<str>]) -> String {
    let lines: Vec<&str> = lines.iter().map(AsRef::as_ref).collect();

    format!("{}\n", lines.join("\n"))
}

fn valid_part() -> Vec<String> {
    [
        "## Objetivo",
        "",
        &format!("Permitir o login. {CANARY}"),
        "",
        "## Diretivas e premissas aplicáveis",
        "",
        "- premises.login-rule",
        "",
        "## Comportamentos",
        "",
        "- CMP-001: o sistema autentica o usuário.",
        "",
        "## Critérios de aceitação",
        "",
        "- CA-001: login válido devolve sessão.",
        "",
        "## Fora de escopo",
        "",
        "Logout.",
    ]
    .map(str::to_owned)
    .to_vec()
}

fn amendment_for(target: &str) -> Vec<String> {
    [
        "## Divergência",
        "",
        target,
        "",
        "## Correções",
        "",
        "- EMD-001 CMP-001: o sistema autentica o usuário e registra o acesso.",
        "",
        "## Motivo",
        "",
        &format!("A spec omitia o registro de acesso. {CANARY}"),
    ]
    .map(str::to_owned)
    .to_vec()
}

fn confirmed_by() -> Value {
    json!({ "by": "person:ana", "question": "A emenda está certa?", "answer": "Sim, está certa." })
}

fn node(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

fn relation(relation_type: &str, from: &str, to: &str) -> Value {
    json!({ "body": {
        "kind": "relation", "relation_type": relation_type, "from": from, "to": to,
    } })
}

fn divergence(id: &str, kind: &str) -> Value {
    let data = json!({
        "source_ref": "CMP-001",
        "kind": kind,
        "text": format!("o código registra o acesso e a spec não diz {CANARY}"),
        "evidence": [{ "source": "docs", "summary": "o doc diz isso" }],
        "resolution": "open",
        "work_item": W1,
    });

    node(id, "divergence", data)
}

fn gap(id: &str) -> Value {
    let data = json!({ "question": "q", "context": "c", "work_item": W1 });

    node(id, "gap", data)
}

fn agent_premise(id: &str) -> Value {
    let data = json!({ "text": "t", "status": "assumed", "kind": "technical", "work_item": W1 });

    node(id, "premise", data)
}

fn directive(id: &str) -> Value {
    node(
        id,
        "guideline",
        json!({ "text": "t", "kind": "business", "confirmed_by": confirmed_by() }),
    )
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_code(output: &Output) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
}

fn error_pointer(output: &Output) -> Value {
    json_line(&output.stderr)["error"]["details"][0]["path"].clone()
}

/// O `retro` de uma saída de `close` ou de `report retro`; a chave precisa existir, mesmo `null`.
fn retro_of(value: &Value) -> Value {
    let object = value.as_object().expect("the output is an object");
    assert!(
        object.contains_key("retro"),
        "the output has a retro key: {value}"
    );

    value["retro"].clone()
}

fn no_amendments() -> Value {
    json!({ "approved": [], "pending": [] })
}

/// Repositório git real com o marcador commitado e o work item `w1` aberto, sem pasta de spec.
struct Fixture {
    project: Project,
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
        let fixture = Fixture { project };
        fixture.append(vec![node(W1, "work-item", typed(work_item_data("t")))]);

        fixture
    }

    /// Como `new`, mais as divergências abertas dadas (kind `partial`).
    fn with_divergences(ids: &[&str]) -> Fixture {
        let fixture = Fixture::new();
        fixture.append(ids.iter().map(|id| divergence(id, "partial")).collect());

        fixture
    }

    /// Como `with_divergences([DIVERGENCE])`, mais a premissa de negócio e a `parte-01.md` válida.
    fn with_spec() -> Fixture {
        let fixture = Fixture::with_divergences(&[DIVERGENCE]);
        let premise = json!({
            "text": "t", "kind": "business", "work_item": W1,
            "status": "confirmed", "confirmed_by": confirmed_by(),
        });
        fixture.append(vec![node(BUSINESS_PREMISE, "premise", premise)]);
        fixture.write("parte-01.md", &valid_part());

        fixture
    }

    fn run(&self, args: &[&str]) -> Output {
        omhl_json(args)
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .run()
    }

    fn append(&self, items: Vec<Value>) {
        let mut request = json!({});
        request["items"] = Value::Array(items);
        let appended = omhl(&["append"])
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .stdin(request.to_string())
            .run();

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }

    fn spec_dir(&self) -> PathBuf {
        fs::canonicalize(self.project.dir())
            .expect("the main directory exists")
            .join(".omhl/specs")
            .join(PART_DIR)
    }

    fn write(&self, name: &str, lines: &[impl AsRef<str>]) {
        let dir = self.spec_dir();
        fs::create_dir_all(&dir).expect("spec directory is created");
        fs::write(dir.join(name), text(lines)).expect("spec file is written");
    }

    /// Aprova a emenda `amendment` (sem `.md`) para `DIVERGENCE`: revisão `spec-stale` com `base`.
    fn approve(&self, amendment: &str) {
        let current = self.run(&["get", DIVERGENCE, "--format", "json"]);
        assert_eq!(current.code, Some(0), "stderr: {}", current.stderr);
        let mut data = json!({
            "source_ref": "CMP-001", "kind": "partial", "text": "t",
            "evidence": [{ "source": "docs", "summary": "o doc diz isso" }],
            "resolution": "spec-stale", "work_item": W1, "amendment": amendment,
        });
        data["confirmed_by"] = confirmed_by();
        let mut revision = node(DIVERGENCE, "divergence", data);
        revision["base"] = json_line(&current.stdout)["node"]["record_id"].clone();

        self.append(vec![revision]);
    }

    /// Divergência `DIVERGENCE` aprovada com `emenda-01.md`, mais `emenda-02.md` ainda sem aprovação.
    fn with_approved_and_pending_amendments() -> Fixture {
        let fixture = Fixture::with_spec();
        fixture.append(vec![divergence(OTHER_DIVERGENCE, "missing")]);
        fixture.write("emenda-01.md", &amendment_for(DIVERGENCE));
        fixture.approve("emenda-01");
        fixture.write("emenda-02.md", &amendment_for(OTHER_DIVERGENCE));

        fixture
    }

    /// Gap `GAP` respondido por uma premissa técnica do agente, sem diretiva que a ampare.
    fn answer_without_criterion(&self) {
        self.append(vec![gap(GAP), agent_premise(ANSWER)]);
        self.append(vec![relation("answers", ANSWER, GAP)]);
    }

    fn write_workflow(&self, toml: &str) {
        let dir = self.project.dir().join(".omhl");
        fs::create_dir_all(&dir).expect("the .omhl directory exists");
        fs::write(dir.join("workflow.toml"), toml).expect("workflow.toml is written");
    }

    /// Uma segunda pasta `wi-w1__*` do mesmo work item, ao lado de `PART_DIR`.
    fn seed_second_folder(&self) {
        let dir = self.spec_dir().with_file_name("wi-w1__other");
        fs::create_dir_all(&dir).expect("second spec directory is created");
        fs::write(dir.join("parte-01.md"), "seed\n").expect("second part is written");
    }

    /// `parte-01.md` vira diretório: a parte existe na listagem, mas não dá para ler como arquivo.
    fn replace_first_part_with_directory(&self) {
        let part = self.spec_dir().join("parte-01.md");
        fs::remove_file(&part).expect("the part exists");
        fs::create_dir(&part).expect("directory takes the part name");
    }

    fn commit_all(&self) {
        for args in [
            &["add", "-A"][..],
            &["commit", "--allow-empty", "-m", "save the work"],
        ] {
            let output = git_in(self.project.dir(), args);
            assert!(output.status.success(), "git {args:?} failed");
        }
    }

    /// A árvore precisa estar limpa para o `close`: o que foi escrito até aqui é commitado.
    fn close_output(&self) -> Output {
        self.commit_all();
        let closed = self.run(&["close", W1]);
        assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
        assert_eq!(closed.stdout.lines().count(), 1, "one JSON line");

        closed
    }

    fn close(&self) -> Value {
        json_line(&self.close_output().stdout)
    }

    fn report_retro(&self) -> Output {
        self.run(&["report", "retro", W1])
    }

    fn retro_report(&self) -> Value {
        let report = self.report_retro();
        assert_eq!(report.code, Some(0), "stderr: {}", report.stderr);
        assert_eq!(report.stdout.lines().count(), 1, "one JSON line");

        json_line(&report.stdout)
    }

    /// O que a cadeia tem hoje: quantidade de records e a cabeça.
    fn chain(&self) -> Value {
        let verified = self.run(&["verify"]);
        assert_eq!(verified.code, Some(0), "stderr: {}", verified.stderr);
        let report = json_line(&verified.stdout);
        assert_eq!(report["ok"], json!(true), "the chain verifies");

        json!({ "total_records": report["total_records"], "chain_head": report["chain_head"] })
    }
}

// `close`

#[test]
fn it_should_return_a_retro_object_when_a_work_item_closes_with_a_divergence() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed),
        json!({
            "divergences": [{ "kind": "partial", "resolution": "open", "ids": [DIVERGENCE] }],
            "amendments": no_amendments(),
            "no_criterion": [],
            "open_gaps": [],
        })
    );
}

#[test]
fn it_should_keep_the_closing_outcome_and_gate_report_next_to_the_retro() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);

    let closed = fixture.close();

    assert_eq!(
        (
            closed["outcome"]["records"].as_array().map(Vec::len),
            closed["report"]["gate"].clone(),
        ),
        (Some(1), json!("pull-request"))
    );
}

#[test]
fn it_should_group_the_retro_divergences_by_kind_and_resolution() {
    let fixture = Fixture::new();
    fixture.append(vec![
        divergence(DIVERGENCE, "partial"),
        divergence(OTHER_DIVERGENCE, "missing"),
    ]);

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed)["divergences"],
        json!([
            { "kind": "missing", "resolution": "open", "ids": [OTHER_DIVERGENCE] },
            { "kind": "partial", "resolution": "open", "ids": [DIVERGENCE] },
        ])
    );
}

#[test]
fn it_should_list_the_approved_amendment_when_a_divergence_is_spec_stale() {
    let fixture = Fixture::with_spec();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE));
    fixture.approve("emenda-01");

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed)["amendments"],
        json!({
            "approved": [{ "name": "emenda-01.md", "divergence": DIVERGENCE }],
            "pending": [],
        })
    );
}

#[test]
fn it_should_group_a_spec_stale_divergence_under_its_resolution() {
    let fixture = Fixture::with_spec();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE));
    fixture.approve("emenda-01");

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed)["divergences"],
        json!([{ "kind": "partial", "resolution": "spec-stale", "ids": [DIVERGENCE] }])
    );
}

#[test]
fn it_should_list_an_amendment_that_is_not_approved_as_pending() {
    let fixture = Fixture::with_approved_and_pending_amendments();

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed)["amendments"],
        json!({
            "approved": [{ "name": "emenda-01.md", "divergence": DIVERGENCE }],
            "pending": ["emenda-02.md"],
        })
    );
}

#[test]
fn it_should_return_a_retro_without_amendments_when_the_work_item_has_no_spec_folder() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);

    let closed = fixture.close();

    assert_eq!(retro_of(&closed)["amendments"], no_amendments());
}

#[test]
fn it_should_return_a_null_retro_when_there_is_no_divergence_and_no_gap() {
    let fixture = Fixture::new();

    let closed = fixture.close();

    assert_eq!(retro_of(&closed), Value::Null);
}

#[test]
fn it_should_return_a_null_retro_when_the_only_finding_is_an_open_gap() {
    let fixture = Fixture::new();
    fixture.append(vec![gap(GAP)]);

    let closed = fixture.close();

    assert_eq!(retro_of(&closed), Value::Null);
}

#[test]
fn it_should_return_a_null_retro_when_the_work_item_has_only_a_pending_amendment_file() {
    let fixture = Fixture::new();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE));

    let closed = fixture.close();

    assert_eq!(retro_of(&closed), Value::Null);
}

#[test]
fn it_should_return_a_retro_that_lists_an_answer_without_criterion_even_without_a_divergence() {
    let fixture = Fixture::new();
    fixture.answer_without_criterion();

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed),
        json!({
            "divergences": [],
            "amendments": no_amendments(),
            "no_criterion": [{ "gap": GAP, "answered_by": ANSWER }],
            "open_gaps": [],
        })
    );
}

#[test]
fn it_should_not_list_an_answer_that_a_confirmed_directive_supports() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);
    fixture.append(vec![gap(GAP), agent_premise(ANSWER), directive(DIRECTIVE)]);
    fixture.append(vec![
        relation("answers", ANSWER, GAP),
        relation("supports", DIRECTIVE, ANSWER),
    ]);

    let closed = fixture.close();

    assert_eq!(retro_of(&closed)["no_criterion"], json!([]));
}

#[test]
fn it_should_list_an_open_gap_in_a_retro_that_a_divergence_triggered() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);
    fixture.append(vec![gap(GAP)]);

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed)["open_gaps"],
        json!([{ "gap": GAP, "state": "open" }])
    );
}

#[test]
fn it_should_list_a_gap_whose_answer_contradicts_a_directive_as_open_with_the_needs_human_state() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);
    fixture.append(vec![gap(GAP), agent_premise(ANSWER), directive(DIRECTIVE)]);
    fixture.append(vec![
        relation("answers", ANSWER, GAP),
        relation("contradicts", ANSWER, DIRECTIVE),
    ]);

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed),
        json!({
            "divergences": [{ "kind": "partial", "resolution": "open", "ids": [DIVERGENCE] }],
            "amendments": no_amendments(),
            "no_criterion": [],
            "open_gaps": [{ "gap": GAP, "state": "needs-human" }],
        })
    );
}

#[test]
fn it_should_write_only_the_closing_record_when_a_work_item_closes_with_a_retro() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.close();

    let after = fixture.chain();
    assert_ne!(retro_of(&closed), Value::Null, "the retro was generated");
    assert_eq!(
        after["total_records"].as_u64(),
        before["total_records"].as_u64().map(|total| total + 1)
    );
}

#[test]
fn it_should_refuse_to_close_and_write_nothing_when_the_retro_facts_cannot_be_read() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);
    fixture.write_workflow("version = 1\ncolour = true\n");
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.run(&["close", W1]);

    assert_eq!(
        (closed.code, fixture.chain()),
        (Some(1), before),
        "stdout: {} stderr: {}",
        closed.stdout,
        closed.stderr
    );
}

#[test]
fn it_should_not_echo_the_spec_or_the_amendment_content_in_the_close_output() {
    let fixture = Fixture::with_approved_and_pending_amendments();

    let closed = fixture.close_output();

    assert_ne!(retro_of(&json_line(&closed.stdout)), Value::Null);
    assert!(
        !closed.stdout.contains(CANARY),
        "the spec content leaked: {}",
        closed.stdout
    );
}

const INVALID_WORKFLOW: &str = "version = 1\ncolour = true\n";

#[test]
fn it_should_refuse_to_close_and_write_nothing_when_workflow_toml_is_invalid_and_there_is_nothing_to_report()
 {
    let fixture = Fixture::new();
    fixture.write_workflow(INVALID_WORKFLOW);
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.run(&["close", W1]);

    assert_eq!(
        (closed.code, error_code(&closed), fixture.chain()),
        (Some(1), json!("invalid-workflow-config"), before),
        "stdout: {} stderr: {}",
        closed.stdout,
        closed.stderr
    );
}

#[test]
fn it_should_refuse_to_close_and_write_nothing_when_workflow_toml_is_invalid_and_the_only_finding_is_an_open_gap()
 {
    let fixture = Fixture::new();
    fixture.append(vec![gap(GAP)]);
    fixture.write_workflow(INVALID_WORKFLOW);
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.run(&["close", W1]);

    assert_eq!(
        (closed.code, error_code(&closed), fixture.chain()),
        (Some(1), json!("invalid-workflow-config"), before),
        "stdout: {} stderr: {}",
        closed.stdout,
        closed.stderr
    );
}

#[test]
fn it_should_close_with_a_null_retro_when_the_work_item_has_two_spec_folders_and_nothing_to_report()
{
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.seed_second_folder();

    let closed = fixture.close();

    assert_eq!(retro_of(&closed), Value::Null);
}

#[test]
fn it_should_refuse_to_close_and_write_nothing_when_an_answer_without_criterion_needs_a_workflow_file_that_is_invalid()
 {
    let fixture = Fixture::new();
    fixture.answer_without_criterion();
    fixture.write_workflow(INVALID_WORKFLOW);
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.run(&["close", W1]);

    assert_eq!(
        (closed.code, fixture.chain()),
        (Some(1), before),
        "stdout: {} stderr: {}",
        closed.stdout,
        closed.stderr
    );
}

// Pasta de spec inutilizável: `spec-store-unusable` em `/work_item`, nunca `probe-failed`

#[test]
fn it_should_refuse_to_close_with_spec_store_unusable_when_the_work_item_has_two_spec_folders() {
    let fixture = Fixture::with_spec();
    fixture.seed_second_folder();
    fixture.commit_all();

    let closed = fixture.run(&["close", W1]);

    assert_eq!(closed.code, Some(1), "stdout: {}", closed.stdout);
    assert_eq!(error_code(&closed), json!("spec-store-unusable"));
    assert_eq!(error_pointer(&closed), json!("/work_item"));
}

#[test]
fn it_should_write_nothing_when_close_is_refused_for_two_spec_folders() {
    let fixture = Fixture::with_spec();
    fixture.seed_second_folder();
    fixture.commit_all();
    let before = fixture.chain();

    fixture.run(&["close", W1]);

    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_list_the_amendments_in_the_close_retro_when_a_part_cannot_be_read() {
    let fixture = Fixture::with_spec();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE));
    fixture.replace_first_part_with_directory();

    let closed = fixture.close();

    assert_eq!(
        retro_of(&closed)["amendments"],
        json!({ "approved": [], "pending": ["emenda-01.md"] })
    );
}

// `omhl report retro`

#[test]
fn it_should_refuse_report_retro_with_spec_store_unusable_when_the_work_item_has_two_spec_folders()
{
    let fixture = Fixture::with_spec();
    fixture.seed_second_folder();

    let report = fixture.report_retro();

    assert_eq!(report.code, Some(1), "stdout: {}", report.stdout);
    assert_eq!(error_code(&report), json!("spec-store-unusable"));
    assert_eq!(error_pointer(&report), json!("/work_item"));
}

#[test]
fn it_should_list_the_amendments_in_report_retro_when_a_part_cannot_be_read() {
    let fixture = Fixture::with_spec();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE));
    fixture.replace_first_part_with_directory();

    let report = fixture.retro_report();

    assert_eq!(
        retro_of(&report)["amendments"],
        json!({ "approved": [], "pending": ["emenda-01.md"] })
    );
}

#[test]
fn it_should_print_the_retro_of_an_open_work_item_with_a_divergence() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);

    let report = fixture.retro_report();

    assert_eq!(
        report,
        json!({ "retro": {
            "divergences": [{ "kind": "partial", "resolution": "open", "ids": [DIVERGENCE] }],
            "amendments": no_amendments(),
            "no_criterion": [],
            "open_gaps": [],
        } })
    );
}

#[test]
fn it_should_print_a_null_retro_when_the_work_item_has_nothing_to_report() {
    let fixture = Fixture::new();

    let report = fixture.retro_report();

    assert_eq!(retro_of(&report), Value::Null);
}

#[test]
fn it_should_print_the_same_retro_as_the_close_of_the_work_item() {
    let fixture = Fixture::with_approved_and_pending_amendments();
    fixture.answer_without_criterion();
    fixture.commit_all();
    let reported = fixture.retro_report();

    let closed = fixture.close();

    assert_eq!(retro_of(&reported), retro_of(&closed));
}

#[test]
fn it_should_print_the_retro_of_a_closed_work_item() {
    let fixture = Fixture::with_divergences(&[DIVERGENCE]);
    let closed = fixture.close();

    let report = fixture.retro_report();

    assert_eq!(retro_of(&report), retro_of(&closed));
}

#[test]
fn it_should_write_nothing_when_it_prints_the_retro() {
    let fixture = Fixture::with_approved_and_pending_amendments();
    let before = fixture.chain();

    let report = fixture.report_retro();

    assert_eq!(report.code, Some(0), "stderr: {}", report.stderr);
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_not_echo_the_spec_or_the_amendment_content_in_the_report_retro_output() {
    let fixture = Fixture::with_approved_and_pending_amendments();

    let report = fixture.report_retro();

    assert_eq!(report.code, Some(0), "stderr: {}", report.stderr);
    assert!(
        !report.stdout.contains(CANARY),
        "the spec content leaked: {}",
        report.stdout
    );
}

#[test]
fn it_should_refuse_the_retro_of_a_work_item_that_does_not_exist() {
    let fixture = Fixture::new();

    let output = fixture.run(&["report", "retro", "work-items.ghost"]);

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    let error = json_line(&output.stderr)["error"].clone();
    assert_eq!(
        (error["code"].clone(), error["details"][0]["path"].clone()),
        (json!("work-item-not-found"), json!("/work_item"))
    );
}

#[test]
fn it_should_refuse_the_retro_of_a_malformed_work_item_id_at_the_work_item_pointer() {
    let fixture = Fixture::new();

    let output = fixture.run(&["report", "retro", "Not A Valid Id"]);

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(
        json_line(&output.stderr)["error"]["details"][0]["path"],
        json!("/work_item")
    );
}

#[test]
fn it_should_refuse_report_retro_without_a_work_item() {
    let fixture = Fixture::new();

    let output = fixture.run(&["report", "retro"]);

    assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert!(
        output.stderr.contains("<WORK_ITEM>"),
        "stderr: {}",
        output.stderr
    );
}

// Famílias de nome da emenda: `amendment-` e `emenda-`

#[test]
fn it_should_list_an_unapproved_english_amendment_as_pending_in_the_report_and_in_the_close() {
    let fixture = Fixture::with_spec();
    fixture.write("amendment-01.md", &amendment_for(DIVERGENCE));

    let report = fixture.retro_report();
    let closed = fixture.close();

    let expected = json!({ "approved": [], "pending": ["amendment-01.md"] });
    assert_eq!(retro_of(&report)["amendments"], expected);
    assert_eq!(retro_of(&closed)["amendments"], expected);
}

#[test]
fn it_should_list_an_unapproved_portuguese_amendment_as_pending_in_the_report_and_in_the_close() {
    let fixture = Fixture::with_spec();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE));

    let report = fixture.retro_report();
    let closed = fixture.close();

    let expected = json!({ "approved": [], "pending": ["emenda-01.md"] });
    assert_eq!(retro_of(&report)["amendments"], expected);
    assert_eq!(retro_of(&closed)["amendments"], expected);
}

#[test]
fn it_should_take_an_approved_english_amendment_out_of_pending() {
    let fixture = Fixture::with_spec();
    fixture.write("amendment-01.md", &amendment_for(DIVERGENCE));
    fixture.approve("amendment-01");

    let report = fixture.retro_report();

    assert_eq!(
        retro_of(&report)["amendments"],
        json!({
            "approved": [{ "name": "amendment-01.md", "divergence": DIVERGENCE }],
            "pending": [],
        })
    );
}

/// Um work item com tipo sem átomo: o `close` não procura o branch, e a fixture não tem um.
fn typed(mut data: serde_json::Value) -> serde_json::Value {
    data["type"] = serde_json::json!("chore");
    data
}

// Gate reprovado: recusa do `close`, `--force --note` e a paridade com o `gate-pr` (CA-007 a CA-009)

const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";
const OWNER_NOTE: &str = "owner approved the override";

/// Repositório com os nativos dados ligados: o gate reprova pelas perguntas deles.
fn fixture_with_natives(natives: &[&str]) -> Fixture {
    let fixture = Fixture::new();
    fixture.project.enable_natives(natives);

    fixture
}

impl Fixture {
    /// Commita o que foi escrito e roda o `close` com os argumentos extras, sem exigir saída 0.
    fn close_with(&self, extra: &[&str]) -> Output {
        self.commit_all();
        let mut args = vec!["close", W1];
        args.extend_from_slice(extra);

        self.run(&args)
    }

    /// Atesta o check no `HEAD` atual, sem commitar: o `close` seguinte precisa da árvore como está.
    fn attest(&self, check: &str) {
        let attested = self.run(&["attest", check, W1, "--outcome", "passed"]);
        assert_eq!(attested.code, Some(0), "stderr: {}", attested.stderr);
    }

    fn gate_pr(&self) -> Value {
        let gate = self.run(&["gate-pr", W1]);
        assert_eq!(gate.code, Some(0), "stderr: {}", gate.stderr);

        json_line(&gate.stdout)
    }

    fn node_data(&self) -> Value {
        json_line(&self.run(&["get", W1]).stdout)["node"]["data"].clone()
    }
}

/// Todos os arquivos sob `dir`, concatenados: a projeção é uma árvore de arquivos.
fn all_text_under(dir: &std::path::Path) -> String {
    let mut text = String::new();
    for entry in fs::read_dir(dir).expect("the directory is read") {
        let path = entry.expect("the entry is read").path();
        if path.is_dir() {
            text.push_str(&all_text_under(&path));
        } else {
            text.push_str(&fs::read_to_string(&path).unwrap_or_default());
        }
    }

    text
}

#[test]
fn it_should_refuse_the_close_with_close_warnings_when_only_a_warn_question_fails() {
    let fixture = fixture_with_natives(&[RUN_TESTS]);
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.run(&["close", W1]);

    assert_eq!(closed.code, Some(1), "stdout: {}", closed.stdout);
    assert_eq!(closed.stdout, "");
    let error = json_line(&closed.stderr)["error"].clone();
    assert_eq!(error["code"], json!("close-warnings"));
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| message.contains("--force --note"))
    );
    assert_eq!(error["details"][0]["code"], json!("warn"));
    assert_eq!(error["details"][0]["context"]["label"], json!(RUN_TESTS));
    assert_eq!(fixture.node_data()["status"], json!("open"));
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_the_close_with_close_refused_when_a_refuse_question_fails_with_a_warn_one() {
    let fixture = fixture_with_natives(&[RUN_TESTS, CI_GREEN]);
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.run(&["close", W1]);

    assert_eq!(closed.code, Some(1), "stdout: {}", closed.stdout);
    let error = json_line(&closed.stderr)["error"].clone();
    assert_eq!(error["code"], json!("close-refused"));
    let message = error["message"].as_str().expect("the message is text");
    assert!(message.contains("--abandon"), "{message}");
    assert!(message.contains("--failed-delivery --note"), "{message}");
    assert!(!message.contains("new commit"), "{message}");
    assert_eq!(error["details"][0]["code"], json!("warn"));
    assert_eq!(error["details"][1]["code"], json!("refuse"));
    assert_eq!(error["details"][1]["context"]["label"], json!(CI_GREEN));
    assert_eq!(fixture.node_data()["status"], json!("open"));
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_close_with_exit_zero_once_the_refused_questions_pass() {
    let fixture = fixture_with_natives(&[RUN_TESTS, CI_GREEN]);
    fixture.commit_all();
    let refused = fixture.run(&["close", W1]);
    assert_eq!(refused.code, Some(1), "stderr: {}", refused.stderr);
    fixture.attest(RUN_TESTS);
    fixture.attest(CI_GREEN);

    let closed = fixture.run(&["close", W1]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(fixture.node_data()["status"], json!("closed"));
    assert!(fixture.node_data()["gate"].get("forced").is_none());
}

#[test]
fn it_should_close_with_exit_zero_when_the_whole_gate_passes() {
    let fixture = Fixture::new();

    let closed = fixture.close_with(&[]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(fixture.node_data()["status"], json!("closed"));
}

#[test]
fn it_should_record_the_forced_close_with_the_note_and_the_overridden_labels() {
    let fixture = fixture_with_natives(&[RUN_TESTS]);

    let closed = fixture.close_with(&["--force", "--note", OWNER_NOTE]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(
        json_line(&closed.stdout)["forced"],
        json!({ "overridden": [RUN_TESTS], "note": OWNER_NOTE })
    );
    let gate = fixture.node_data()["gate"].clone();
    assert_eq!(gate["passed"], json!(false));
    assert_eq!(gate["forced"], json!(true));
    assert_eq!(gate["overridden"], json!([RUN_TESTS]));
    assert_eq!(gate["note"], json!(OWNER_NOTE));
}

#[test]
fn it_should_print_the_forced_header_for_the_forced_close() {
    let fixture = fixture_with_natives(&[RUN_TESTS]);
    fixture.commit_all();

    let closed = omhl(&["close", W1, "--force", "--note", OWNER_NOTE])
        .cwd(fixture.project.dir())
        .xdg(fixture.project.xdg())
        .run();

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert!(
        closed
            .stdout
            .lines()
            .any(|line| line == "⚠ closed · forced · 1 overridden"),
        "{}",
        closed.stdout
    );
}

#[test]
fn it_should_show_the_note_and_the_labels_in_the_retro_and_the_projection() {
    let fixture = fixture_with_natives(&[RUN_TESTS]);
    fixture.close_with(&["--force", "--note", OWNER_NOTE]);

    let retro = retro_of(&fixture.retro_report());
    let projection = fixture.run(&["project"]);
    assert_eq!(projection.code, Some(0), "stderr: {}", projection.stderr);
    let dir = json_line(&projection.stdout)["dir"]
        .as_str()
        .expect("the projection dir is text")
        .to_owned();

    assert_eq!(
        retro["gate_override"],
        json!({ "note": OWNER_NOTE, "overridden": [RUN_TESTS] })
    );
    let written = all_text_under(&fixture.project.dir().join(dir));
    assert!(
        written.contains(&format!("- note: {OWNER_NOTE}")),
        "{written}"
    );
    assert!(
        written.contains(&format!("- overridden: {RUN_TESTS}")),
        "{written}"
    );
}

#[test]
fn it_should_refuse_the_forced_close_and_write_nothing_when_a_refuse_question_fails() {
    let fixture = fixture_with_natives(&[RUN_TESTS, CI_GREEN]);
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.run(&["close", W1, "--force", "--note", OWNER_NOTE]);

    assert_eq!(closed.code, Some(1), "stdout: {}", closed.stdout);
    assert_eq!(
        json_line(&closed.stderr)["error"]["code"],
        json!("close-refused")
    );
    assert_eq!(fixture.node_data()["status"], json!("open"));
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_discard_the_note_and_record_nothing_forced_when_the_gate_passes() {
    let fixture = Fixture::new();
    fixture.commit_all();

    let closed = fixture.run(&["close", W1, "--force", "--note", OWNER_NOTE]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(json_line(&closed.stdout)["note_discarded"], json!(true));
    let gate = fixture.node_data()["gate"].clone();
    assert!(gate.get("forced").is_none());
    assert!(gate.get("note").is_none());
    assert!(gate.get("overridden").is_none());
}

#[test]
fn it_should_print_the_discarded_note_warning_when_the_gate_passes() {
    let fixture = Fixture::new();
    fixture.commit_all();

    let closed = omhl(&["close", W1, "--force", "--note", OWNER_NOTE])
        .cwd(fixture.project.dir())
        .xdg(fixture.project.xdg())
        .run();

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert!(
        closed
            .stdout
            .lines()
            .any(|line| line == "⚠ note discarded · gate passed"),
        "{}",
        closed.stdout
    );
}

#[test]
fn it_should_refuse_force_and_note_that_do_not_come_together() {
    let fixture = fixture_with_natives(&[RUN_TESTS]);
    fixture.commit_all();
    let before = fixture.chain();

    let force_alone = fixture.run(&["close", W1, "--force"]);
    let note_alone = fixture.run(&["close", W1, "--note", OWNER_NOTE]);

    assert_ne!(force_alone.code, Some(0), "stderr: {}", force_alone.stderr);
    assert_ne!(note_alone.code, Some(0), "stderr: {}", note_alone.stderr);
    assert_eq!(fixture.node_data()["status"], json!("open"));
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_close_with_snapshot_dirty_with_and_without_force_before_any_gate_question() {
    let fixture = Fixture::new();
    fs::write(fixture.project.dir().join("dirty.txt"), "not committed\n")
        .expect("the dirty file is written");
    let before = fixture.chain();

    let plain = fixture.run(&["close", W1]);
    let forced = fixture.run(&["close", W1, "--force", "--note", OWNER_NOTE]);

    assert_eq!(
        (error_code(&plain), error_code(&forced), fixture.chain()),
        (json!("snapshot-dirty"), json!("snapshot-dirty"), before),
        "stderr: {} / {}",
        plain.stderr,
        forced.stderr
    );
}

#[test]
fn it_should_refuse_close_with_invalid_workflow_config_before_a_missing_work_item() {
    let fixture = Fixture::new();
    fixture.write_workflow(INVALID_WORKFLOW);
    fixture.commit_all();
    let before = fixture.chain();

    let plain = fixture.run(&["close", MISSING_WORK_ITEM]);
    let forced = fixture.run(&["close", MISSING_WORK_ITEM, "--force", "--note", OWNER_NOTE]);

    assert_eq!(
        (error_code(&plain), error_code(&forced), fixture.chain()),
        (
            json!("invalid-workflow-config"),
            json!("invalid-workflow-config"),
            before
        ),
        "stderr: {} / {}",
        plain.stderr,
        forced.stderr
    );
}

#[test]
fn it_should_refuse_close_with_work_item_not_found_with_and_without_force() {
    let fixture = Fixture::new();
    fixture.commit_all();
    let before = fixture.chain();

    let plain = fixture.run(&["close", MISSING_WORK_ITEM]);
    let forced = fixture.run(&["close", MISSING_WORK_ITEM, "--force", "--note", OWNER_NOTE]);

    assert_eq!(
        (error_code(&plain), error_code(&forced), fixture.chain()),
        (
            json!("work-item-not-found"),
            json!("work-item-not-found"),
            before
        ),
        "stderr: {} / {}",
        plain.stderr,
        forced.stderr
    );
}

#[test]
fn it_should_refuse_close_of_a_closed_work_item_with_and_without_force() {
    let fixture = Fixture::new();
    fixture.close_output();
    let before = fixture.chain();

    let plain = fixture.run(&["close", W1]);
    let forced = fixture.run(&["close", W1, "--force", "--note", OWNER_NOTE]);

    assert_eq!(
        (error_code(&plain), error_code(&forced), fixture.chain()),
        (
            json!("invalid-transition"),
            json!("invalid-transition"),
            before
        ),
        "stderr: {} / {}",
        plain.stderr,
        forced.stderr
    );
}

#[test]
fn it_should_refuse_close_of_a_closed_work_item_whose_type_is_no_longer_in_the_table() {
    let fixture = Fixture::new();
    fixture.close_output();
    fixture.write_workflow(FEAT_ONLY_WORKFLOW);
    fixture.commit_all();
    let before = fixture.chain();

    let plain = fixture.run(&["close", W1]);
    let forced = fixture.run(&["close", W1, "--force", "--note", OWNER_NOTE]);

    assert_eq!(
        (error_code(&plain), error_code(&forced), fixture.chain()),
        (
            json!("invalid-transition"),
            json!("invalid-transition"),
            before
        ),
        "stderr: {} / {}",
        plain.stderr,
        forced.stderr
    );
}

#[test]
fn it_should_refuse_gate_pr_with_invalid_workflow_config_before_a_missing_work_item() {
    let fixture = Fixture::new();
    fixture.write_workflow(INVALID_WORKFLOW);
    fixture.commit_all();

    let gate = fixture.run(&["gate-pr", MISSING_WORK_ITEM]);

    assert_eq!(
        error_code(&gate),
        json!("invalid-workflow-config"),
        "stderr: {}",
        gate.stderr
    );
}

#[test]
fn it_should_ask_gate_pr_the_same_failing_questions_and_classes_that_close_refuses() {
    for natives in [&[RUN_TESTS][..], &[RUN_TESTS, CI_GREEN][..]] {
        let fixture = fixture_with_natives(natives);
        fixture.commit_all();

        let gate = fixture.gate_pr();
        let closed = fixture.run(&["close", W1]);

        let asked: Vec<Value> = gate["questions"]
            .as_array()
            .expect("the questions are a list")
            .iter()
            .filter(|question| question["passed"] == json!(false))
            .map(|question| json!([question["label"], question["class"]]))
            .collect();
        let refused: Vec<Value> = json_line(&closed.stderr)["error"]["details"]
            .as_array()
            .expect("the details are a list")
            .iter()
            .map(|detail| json!([detail["context"]["label"], detail["code"]]))
            .collect();
        assert_eq!(gate["passed"], json!(false));
        assert_eq!(asked, refused, "natives {natives:?}");
    }
}

// `close --abandon`: fecha sem avaliar o gate (CA-011 a CA-013 e CA-015)

const ABANDON_REASON: &str = "the owner dropped the feature";
const ABANDON_NOTE: &str = "authorized by the owner";

/// Os dois tipos de nó que o `close` aceita: o work item da fixture e um pull-request.
fn closable_ids() -> [&'static str; 2] {
    [W1, PR_7]
}

impl Fixture {
    fn with_pull_request() -> Fixture {
        let fixture = Fixture::new();
        fixture.append(vec![node(
            PR_7,
            "pull-request",
            pull_request_data("Add gate"),
        )]);

        fixture
    }

    fn abandon_cmd(&self, id: &str) -> support::child::Cmd {
        omhl_json(&[
            "close",
            id,
            "--abandon",
            "--reason",
            ABANDON_REASON,
            "--note",
            ABANDON_NOTE,
        ])
        .cwd(self.project.dir())
        .xdg(self.project.xdg())
    }

    fn abandon(&self, id: &str) -> Output {
        self.abandon_cmd(id).run()
    }

    fn data_of(&self, id: &str) -> Value {
        json_line(&self.run(&["get", id]).stdout)["node"]["data"].clone()
    }

    fn head(&self) -> String {
        let output = git_in(self.project.dir(), &["rev-parse", "HEAD"]);

        String::from_utf8(output.stdout)
            .expect("the HEAD id is text")
            .trim()
            .to_owned()
    }
}

#[test]
fn it_should_abandon_with_a_dirty_tree_and_an_invalid_workflow_and_record_the_closing() {
    for id in closable_ids() {
        let fixture = Fixture::with_pull_request();
        fs::write(fixture.project.dir().join("dirty.txt"), "not committed\n")
            .expect("the dirty file is written");
        fixture.write_workflow(INVALID_WORKFLOW);

        let closed = fixture.abandon(id);

        assert_eq!(closed.code, Some(0), "{id} stderr: {}", closed.stderr);
        let data = fixture.data_of(id);
        assert_eq!(data["status"], json!("closed"), "{id}");
        assert_eq!(
            data["abandonment"],
            json!({ "reason": ABANDON_REASON, "note": ABANDON_NOTE }),
            "{id}"
        );
        assert_eq!(data["gate"]["name"], json!("abandoned"), "{id}");
        assert_eq!(data["gate"]["passed"], json!(false), "{id}");
        assert_eq!(
            data["gate"]["snapshot"]["kind"],
            json!("git-commit"),
            "{id}"
        );
        assert_eq!(
            data["gate"]["snapshot"]["id"],
            json!(fixture.head()),
            "{id}"
        );
        let abandoned = json_line(&closed.stdout)["abandoned"].clone();
        assert_eq!(abandoned, data["abandonment"], "{id}");
    }
}

#[test]
fn it_should_record_the_session_of_the_abandonment() {
    let fixture = Fixture::new();

    let closed = fixture
        .abandon_cmd(W1)
        .env("CLAUDE_CODE_SESSION_ID", "sess-abandon-1")
        .run();

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    let sessions = fixture.run(&["report", "sessions"]).stdout;
    assert!(sessions.contains("sess-abandon-1"), "{sessions}");
}

#[test]
fn it_should_refuse_the_abandonment_with_probe_failed_and_write_nothing_without_a_commit() {
    let project = Project::new("acme");
    let init = git_in(project.dir(), &["init", "-b", "main"]);
    assert!(init.status.success(), "git init failed");
    let fixture = Fixture { project };
    fixture.append(vec![node(W1, "work-item", typed(work_item_data("t")))]);
    let before = fixture.chain();

    let closed = fixture.abandon(W1);

    assert_eq!(
        (closed.code, error_code(&closed), fixture.chain()),
        (Some(1), json!("probe-failed"), before),
        "stderr: {}",
        closed.stderr
    );
}

#[test]
fn it_should_exit_two_for_the_abandon_argument_combinations_that_do_not_hold() {
    let fixture = Fixture::new();
    let combinations: [&[&str]; 8] = [
        &["close", W1, "--abandon", "--note", "n"],
        &["close", W1, "--abandon", "--reason", "r"],
        &["close", W1, "--reason", "r"],
        &["close", W1, "--force"],
        &["close", W1, "--note", "n"],
        &[
            "close",
            W1,
            "--abandon",
            "--force",
            "--reason",
            "r",
            "--note",
            "n",
        ],
        &[
            "close",
            W1,
            "--abandon",
            "--failed-delivery",
            "--reason",
            "r",
            "--note",
            "n",
        ],
        &[
            "close",
            W1,
            "--abandon",
            "--base",
            "main",
            "--reason",
            "r",
            "--note",
            "n",
        ],
    ];
    let before = fixture.chain();

    for args in combinations {
        let closed = fixture.run(args);

        assert_eq!(closed.code, Some(2), "{args:?} stderr: {}", closed.stderr);
    }
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_an_empty_or_too_long_reason_or_note_with_invalid_input() {
    let fixture = Fixture::new();
    let long = "x".repeat(501);
    let invalid: [(&str, &str); 4] = [
        ("", "n"),
        ("r", ""),
        (long.as_str(), "n"),
        ("r", long.as_str()),
    ];
    let before = fixture.chain();

    for (reason, note) in invalid {
        let closed = fixture.run(&["close", W1, "--abandon", "--reason", reason, "--note", note]);

        assert_eq!(
            (closed.code, error_code(&closed)),
            (Some(1), json!("INVALID_INPUT")),
            "stderr: {}",
            closed.stderr
        );
    }
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_to_abandon_a_closed_item_and_an_unknown_one() {
    let fixture = Fixture::new();
    let first = fixture.abandon(W1);
    assert_eq!(first.code, Some(0), "stderr: {}", first.stderr);

    let again = fixture.abandon(W1);
    let ghost = fixture.abandon(MISSING_WORK_ITEM);

    assert_eq!(error_code(&again), json!("invalid-transition"));
    assert_eq!(error_code(&ghost), json!("work-item-not-found"));
}

#[test]
fn it_should_refuse_an_append_that_carries_abandonment_or_delivery_failure() {
    for id in closable_ids() {
        for (field, value) in [
            ("abandonment", json!({ "reason": "r", "note": "n" })),
            ("delivery_failure", json!({ "note": "n" })),
        ] {
            let fixture = Fixture::with_pull_request();
            let mut data = fixture.data_of(id);
            data[field] = value;
            let before = fixture.chain();
            let mut request = json!({});
            let type_name = if id == W1 {
                "work-item"
            } else {
                "pull-request"
            };
            request["items"] = Value::Array(vec![node(id, type_name, data)]);

            let appended = omhl_json(&["append"])
                .cwd(fixture.project.dir())
                .xdg(fixture.project.xdg())
                .stdin(request.to_string())
                .run();

            assert_eq!(
                (appended.code, error_code(&appended), fixture.chain()),
                (Some(1), json!("use-close-work-item"), before),
                "{id} {field} stderr: {}",
                appended.stderr
            );
        }
    }
}

#[test]
fn it_should_list_an_abandoned_item_as_closed_and_refuse_a_second_close() {
    for id in closable_ids() {
        let fixture = Fixture::with_pull_request();
        fixture.abandon(id);

        let listed = fixture.run(&["list", "--where", "status=closed"]);
        let again = fixture.run(&["close", id, "--force", "--note", "n"]);

        assert!(listed.stdout.contains(id), "{id} {}", listed.stdout);
        assert_eq!(error_code(&again), json!("invalid-transition"), "{id}");
    }
}

#[test]
fn it_should_exit_two_when_the_close_options_appear_in_another_subcommand() {
    let fixture = Fixture::new();

    for flag in [
        "--force",
        "--abandon",
        "--failed-delivery",
        "--reason",
        "--note",
    ] {
        for command in [&["gate-pr", W1][..], &["append"]] {
            let mut args = command.to_vec();
            args.push(flag);
            if matches!(flag, "--reason" | "--note") {
                args.push("x");
            }

            let output = fixture.run(&args);

            assert_eq!(output.code, Some(2), "{args:?} stderr: {}", output.stderr);
        }
    }
}

#[test]
fn it_should_still_accept_force_in_hooks_install() {
    let fixture = Fixture::new();

    let installed = fixture.run(&["hooks", "install", "--force"]);

    assert_ne!(installed.code, Some(2), "stderr: {}", installed.stderr);
}

// Leitores do nó abandonado e do nó fechado no formato antigo

const OLD_WORK_ITEM: &str = "work-items.old";
const OLD_PULL_REQUEST: &str = "work-items.old-pr";

impl Fixture {
    fn abandoned(id: &str) -> Fixture {
        let fixture = Fixture::with_pull_request();
        let closed = fixture.abandon(id);
        assert_eq!(closed.code, Some(0), "{id} stderr: {}", closed.stderr);

        fixture
    }

    /// Gravados direto no banco: a guarda do `append` recusa `closed` fora do `close`.
    fn with_old_closed_nodes() -> Fixture {
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
        support::forge::forge(
            &project,
            support::author(),
            vec![
                support::omhl::node_item(
                    OLD_WORK_ITEM,
                    "work-item",
                    closed_with_gate(work_item_data("Old work")),
                ),
                support::omhl::node_item(
                    OLD_PULL_REQUEST,
                    "pull-request",
                    closed_with_gate(pull_request_data("Old pr")),
                ),
            ],
        );

        Fixture { project }
    }
}

#[test]
fn it_should_show_the_reason_and_the_note_in_get_of_an_abandoned_node() {
    for id in closable_ids() {
        let fixture = Fixture::abandoned(id);

        let got = fixture.run(&["get", id]);

        assert_eq!(got.code, Some(0), "{id} stderr: {}", got.stderr);
        assert!(got.stdout.contains(ABANDON_REASON), "{id} {}", got.stdout);
        assert!(got.stdout.contains(ABANDON_NOTE), "{id} {}", got.stdout);
    }
}

#[test]
fn it_should_show_the_reason_and_the_note_in_report_retro_of_an_abandoned_node() {
    for id in closable_ids() {
        let fixture = Fixture::abandoned(id);

        let retro = fixture.run(&["report", "retro", id]);

        assert_eq!(retro.code, Some(0), "{id} stderr: {}", retro.stderr);
        assert!(
            retro.stdout.contains(ABANDON_REASON),
            "{id} {}",
            retro.stdout
        );
        assert!(retro.stdout.contains(ABANDON_NOTE), "{id} {}", retro.stdout);
    }
}

#[test]
fn it_should_run_render_without_error_when_a_node_is_abandoned() {
    for id in closable_ids() {
        let fixture = Fixture::abandoned(id);

        let rendered = fixture.run(&["render"]);

        assert_eq!(rendered.code, Some(0), "{id} stderr: {}", rendered.stderr);
    }
}

#[test]
fn it_should_run_the_projection_without_error_when_a_node_is_abandoned() {
    for id in closable_ids() {
        let fixture = Fixture::abandoned(id);

        let projected = fixture.run(&["project"]);

        assert_eq!(projected.code, Some(0), "{id} stderr: {}", projected.stderr);
    }
}

#[test]
fn it_should_print_only_the_retro_line_of_the_abandonment_without_any_gate_question() {
    for id in closable_ids() {
        let fixture = Fixture::with_pull_request();

        let closed = omhl(&[
            "close",
            id,
            "--abandon",
            "--reason",
            ABANDON_REASON,
            "--note",
            ABANDON_NOTE,
        ])
        .cwd(fixture.project.dir())
        .xdg(fixture.project.xdg())
        .run();

        assert_eq!(closed.code, Some(0), "{id} stderr: {}", closed.stderr);
        assert!(closed.stdout.contains("retro"), "{id} {}", closed.stdout);
        assert!(
            closed.stdout.contains(ABANDON_REASON),
            "{id} {}",
            closed.stdout
        );
        assert!(
            !closed.stdout.contains("question"),
            "{id} {}",
            closed.stdout
        );
        assert!(!closed.stdout.contains('✘'), "{id} {}", closed.stdout);
    }
}

#[test]
fn it_should_read_a_node_closed_in_the_old_format_with_get() {
    let fixture = Fixture::with_old_closed_nodes();

    for (id, title) in [(OLD_WORK_ITEM, "Old work"), (OLD_PULL_REQUEST, "Old pr")] {
        let got = fixture.run(&["get", id]);

        assert_eq!(got.code, Some(0), "{id} stderr: {}", got.stderr);
        assert!(got.stdout.contains(title), "{id} {}", got.stdout);
    }
}

#[test]
fn it_should_read_a_node_closed_in_the_old_format_with_the_projection() {
    let fixture = Fixture::with_old_closed_nodes();

    let projected = fixture.run(&["project"]);

    assert_eq!(projected.code, Some(0), "stderr: {}", projected.stderr);
}

#[test]
fn it_should_read_a_node_closed_in_the_old_format_with_report_retro() {
    let fixture = Fixture::with_old_closed_nodes();

    for id in [OLD_WORK_ITEM, OLD_PULL_REQUEST] {
        let retro = fixture.run(&["report", "retro", id]);

        assert_eq!(retro.code, Some(0), "{id} stderr: {}", retro.stderr);
        assert_eq!(retro_of(&json_line(&retro.stdout)), Value::Null, "{id}");
    }
}

// `close --failed-delivery`: fecha o item já mergeado cujo commit ainda reprova (CA-040 a CA-044)

const DELIVERY_NOTE: &str = "autorizado";
const ITEM_BRANCH: &str = "feat/wi-w1__login";
const OTHER_ITEM_BRANCH: &str = "feat/wi-w2__other";

impl Fixture {
    /// Branch do item com um commit próprio, `develop` ainda sem ele.
    fn unmerged(natives: &[&str]) -> Fixture {
        let fixture = fixture_with_natives(natives);
        fixture.git(&["checkout", "-b", ITEM_BRANCH]);
        fs::write(fixture.project.dir().join("feature.txt"), "feature\n")
            .expect("the feature file is written");
        fixture.commit_all();
        fixture.git(&["branch", "develop", "main"]);

        fixture
    }

    /// Item mergeado na `develop` com merge commit; o `HEAD` volta para a ponta do item.
    fn merged(natives: &[&str]) -> Fixture {
        let fixture = Fixture::unmerged(natives);
        fixture.git(&["checkout", "develop"]);
        fixture.git(&["merge", "--no-ff", "-m", "merge the item", ITEM_BRANCH]);
        fixture.git(&["checkout", ITEM_BRANCH]);

        fixture
    }

    fn squashed(natives: &[&str]) -> Fixture {
        let fixture = Fixture::unmerged(natives);
        fixture.git(&["checkout", "develop"]);
        fixture.git(&["merge", "--squash", ITEM_BRANCH]);
        fixture.git(&["commit", "-m", "squash the item"]);
        fixture.git(&["checkout", ITEM_BRANCH]);

        fixture
    }

    fn git(&self, args: &[&str]) {
        git_ok(self.project.dir(), args);
    }

    fn develop_commit(&self) -> String {
        git_ok(self.project.dir(), &["rev-parse", "refs/heads/develop"])
            .trim()
            .to_owned()
    }

    fn failed_delivery(&self, id: &str) -> Output {
        self.run(&["close", id, "--failed-delivery", "--note", DELIVERY_NOTE])
    }

    fn failed_delivery_text(&self, id: &str) -> Output {
        omhl(&["close", id, "--failed-delivery", "--note", DELIVERY_NOTE])
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .run()
    }

    fn pull_request_on(&self, branch: &str) {
        let mut data = pull_request_data("Add gate");
        data["branch"] = json!(branch);
        self.append(vec![node(PR_7, "pull-request", data)]);
    }

    fn assert_refused_with(&self, output: &Output, code: &str, before: &Value) {
        assert_eq!(
            (output.code, error_code(output), &self.chain()),
            (Some(1), json!(code), before),
            "stderr: {}",
            output.stderr
        );
    }
}

#[test]
fn it_should_close_a_merged_work_item_recording_the_failure_and_leaving_the_gate_without_override()
{
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);

    let closed = fixture.failed_delivery(W1);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    let data = fixture.data_of(W1);
    assert_eq!(data["status"], json!("closed"));
    assert_eq!(data["gate"]["passed"], json!(false));
    for key in ["forced", "overridden", "note"] {
        assert!(data["gate"].get(key).is_none(), "{key} in the gate");
    }
}

#[test]
fn it_should_record_the_note_the_failed_questions_and_the_integration_read() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);

    fixture.failed_delivery(W1);

    assert_eq!(
        fixture.data_of(W1)["delivery_failure"],
        json!({
            "note": DELIVERY_NOTE,
            "failed": [
                { "label": RUN_TESTS, "class": "warn" },
                { "label": CI_GREEN, "class": "refuse" },
            ],
            "integration": { "ref": "develop", "commit": fixture.develop_commit() },
        })
    );
}

#[test]
fn it_should_close_a_merged_pull_request_recording_the_failure() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);
    fixture.pull_request_on(ITEM_BRANCH);

    let closed = fixture.failed_delivery(PR_7);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    let data = fixture.data_of(PR_7);
    assert_eq!(data["status"], json!("closed"));
    assert_eq!(data["gate"]["passed"], json!(false));
    assert_eq!(data["delivery_failure"]["note"], json!(DELIVERY_NOTE));
    assert_eq!(
        data["delivery_failure"]["integration"]["commit"],
        json!(fixture.develop_commit())
    );
}

#[test]
fn it_should_print_the_delivered_with_failure_header() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);

    let closed = fixture.failed_delivery_text(W1);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert!(
        closed
            .stdout
            .lines()
            .any(|line| line == "⚠ closed · delivered with failure · 2 failed"),
        "{}",
        closed.stdout
    );
}

const REVIEW: &str = "checks.config-loosening-review";

#[test]
fn it_should_leave_the_review_question_out_of_the_failed_ones_when_the_base_range_touches_only_source()
 {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN, REVIEW]);

    let closed = fixture.run(&[
        "close",
        W1,
        "--failed-delivery",
        "--note",
        DELIVERY_NOTE,
        "--base",
        "main",
    ]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    let failed = fixture.data_of(W1)["delivery_failure"]["failed"].to_string();
    assert!(!failed.contains(REVIEW), "{failed}");
}

#[test]
fn it_should_refuse_the_failed_delivery_with_review_base_invalid_and_write_nothing_when_the_base_does_not_resolve()
 {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN, REVIEW]);
    let before = fixture.chain();

    let closed = fixture.run(&[
        "close",
        W1,
        "--failed-delivery",
        "--note",
        DELIVERY_NOTE,
        "--base",
        "no-such-ref",
    ]);

    fixture.assert_refused_with(&closed, "review-base-invalid", &before);
}

#[test]
fn it_should_record_the_session_of_the_failed_delivery_close() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);

    let closed = omhl_json(&["close", W1, "--failed-delivery", "--note", DELIVERY_NOTE])
        .cwd(fixture.project.dir())
        .xdg(fixture.project.xdg())
        .env("CLAUDE_CODE_SESSION_ID", "sess-failed-delivery-1")
        .run();

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    let sessions = fixture.run(&["report", "sessions"]).stdout;
    assert!(sessions.contains("sess-failed-delivery-1"), "{sessions}");
}

#[test]
fn it_should_show_the_note_the_questions_and_the_integration_in_get_report_retro_and_projection() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);
    fixture.failed_delivery(W1);
    let projection = fixture.run(&["project"]);
    let dir = json_line(&projection.stdout)["dir"]
        .as_str()
        .expect("the projection dir is text")
        .to_owned();
    let projected = all_text_under(&fixture.project.dir().join(dir));

    let got = fixture.run(&["get", W1]).stdout;
    let retro = fixture.report_retro().stdout;

    for shown in [&got, &retro, &projected] {
        assert!(shown.contains(DELIVERY_NOTE), "{shown}");
        assert!(shown.contains(CI_GREEN), "{shown}");
    }
    assert!(got.contains(&fixture.develop_commit()), "{got}");
    assert_eq!(fixture.run(&["render"]).code, Some(0));
}

#[test]
fn it_should_exit_two_and_write_nothing_for_the_failed_delivery_argument_combinations_that_do_not_hold()
 {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);
    let before = fixture.chain();

    for extra in [
        &["--failed-delivery"][..],
        &["--failed-delivery", "--note", DELIVERY_NOTE, "--force"],
        &[
            "--failed-delivery",
            "--note",
            DELIVERY_NOTE,
            "--abandon",
            "--reason",
            "r",
        ],
        &[
            "--failed-delivery",
            "--note",
            DELIVERY_NOTE,
            "--reason",
            "r",
        ],
    ] {
        let mut args = vec!["close", W1];
        args.extend_from_slice(extra);

        let output = fixture.run(&args);

        assert_eq!(output.code, Some(2), "{args:?} stderr: {}", output.stderr);
    }
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_with_failed_delivery_unneeded_when_the_whole_gate_passes() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);
    fixture.attest(RUN_TESTS);
    fixture.attest(CI_GREEN);
    let before = fixture.chain();

    let closed = fixture.failed_delivery(W1);

    fixture.assert_refused_with(&closed, "failed-delivery-unneeded", &before);
}

#[test]
fn it_should_refuse_with_failed_delivery_unneeded_pointing_to_force_when_only_a_warn_fails() {
    let fixture = Fixture::merged(&[RUN_TESTS]);
    let before = fixture.chain();

    let closed = fixture.failed_delivery(W1);

    fixture.assert_refused_with(&closed, "failed-delivery-unneeded", &before);
    assert!(
        json_line(&closed.stderr)["error"]
            .to_string()
            .contains("--force --note"),
        "{}",
        closed.stderr
    );
}

#[test]
fn it_should_refuse_the_failed_delivery_with_snapshot_dirty_first() {
    let fixture = Fixture::new();
    fixture.write_workflow(INVALID_WORKFLOW);
    let before = fixture.chain();

    let closed = fixture.failed_delivery(MISSING_WORK_ITEM);

    fixture.assert_refused_with(&closed, "snapshot-dirty", &before);
}

#[test]
fn it_should_refuse_the_failed_delivery_with_invalid_workflow_config_before_a_missing_work_item() {
    let fixture = Fixture::new();
    fixture.write_workflow(INVALID_WORKFLOW);
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.failed_delivery(MISSING_WORK_ITEM);

    fixture.assert_refused_with(&closed, "invalid-workflow-config", &before);
}

#[test]
fn it_should_refuse_the_failed_delivery_with_work_item_not_found() {
    let fixture = Fixture::new();
    fixture.commit_all();
    let before = fixture.chain();

    let closed = fixture.failed_delivery(MISSING_WORK_ITEM);

    fixture.assert_refused_with(&closed, "work-item-not-found", &before);
}

#[test]
fn it_should_refuse_the_failed_delivery_of_a_closed_work_item_with_invalid_transition() {
    let fixture = Fixture::new();
    fixture.close_output();
    let before = fixture.chain();

    let closed = fixture.failed_delivery(W1);

    fixture.assert_refused_with(&closed, "invalid-transition", &before);
}

#[test]
fn it_should_refuse_with_delivery_not_integrated_before_the_merge() {
    let fixture = Fixture::unmerged(&[RUN_TESTS, CI_GREEN]);
    let before = fixture.chain();

    let closed = fixture.failed_delivery(W1);

    fixture.assert_refused_with(&closed, "delivery-not-integrated", &before);
}

#[test]
fn it_should_refuse_with_delivery_not_integrated_after_a_squash_merge() {
    let fixture = Fixture::squashed(&[RUN_TESTS, CI_GREEN]);
    let before = fixture.chain();

    let closed = fixture.failed_delivery(W1);

    fixture.assert_refused_with(&closed, "delivery-not-integrated", &before);
}

#[test]
fn it_should_refuse_with_delivery_not_integrated_when_the_local_develop_does_not_exist() {
    let fixture = Fixture::unmerged(&[RUN_TESTS, CI_GREEN]);
    fixture.git(&["branch", "-D", "develop"]);
    let before = fixture.chain();

    let closed = fixture.failed_delivery(W1);

    fixture.assert_refused_with(&closed, "delivery-not-integrated", &before);
}

#[test]
fn it_should_refuse_with_delivery_not_on_work_item_branch_when_head_is_on_develop() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);
    fixture.git(&["checkout", "develop"]);
    let before = fixture.chain();

    let closed = fixture.failed_delivery(W1);

    fixture.assert_refused_with(&closed, "delivery-not-on-work-item-branch", &before);
}

#[test]
fn it_should_refuse_with_delivery_not_on_work_item_branch_on_another_work_item_branch() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);
    fixture.git(&["checkout", "-b", OTHER_ITEM_BRANCH]);
    let before = fixture.chain();

    let closed = fixture.failed_delivery(W1);

    fixture.assert_refused_with(&closed, "delivery-not-on-work-item-branch", &before);
}

#[test]
fn it_should_refuse_with_delivery_not_on_work_item_branch_when_the_pull_request_branch_differs() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);
    fixture.pull_request_on("feat/wi-w1__another-name");
    let before = fixture.chain();

    let closed = fixture.failed_delivery(PR_7);

    fixture.assert_refused_with(&closed, "delivery-not-on-work-item-branch", &before);
}

#[test]
fn it_should_print_the_retro_block_and_the_next_line_when_closing_with_failed_delivery() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);

    let closed = fixture.failed_delivery_text(W1);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert!(closed.stdout.contains("retro"), "{}", closed.stdout);
    assert!(closed.stdout.contains(CI_GREEN), "{}", closed.stdout);
    assert!(
        closed
            .stdout
            .lines()
            .last()
            .is_some_and(|line| line.starts_with("Next:")),
        "{}",
        closed.stdout
    );
}

#[test]
fn it_should_return_a_non_null_retro_when_closing_with_failed_delivery_without_divergence() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);

    let closed = fixture.failed_delivery(W1);

    let retro = retro_of(&json_line(&closed.stdout));
    assert!(!retro.is_null(), "{}", closed.stdout);
    assert!(retro.to_string().contains(DELIVERY_NOTE), "{retro}");
}

#[test]
fn it_should_print_the_same_retro_in_report_retro_after_the_failed_delivery_close() {
    let fixture = Fixture::merged(&[RUN_TESTS, CI_GREEN]);
    let closed = json_line(&fixture.failed_delivery(W1).stdout);

    let reported = fixture.retro_report();

    assert_eq!(retro_of(&reported), retro_of(&closed));
}
