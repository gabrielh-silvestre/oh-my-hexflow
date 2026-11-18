use crate::support;

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use support::child::{Output, omhl};
use support::git::git_in;
use support::omhl::work_item_data;
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
        fixture.append(vec![node(W1, "work-item", work_item_data("t"))]);

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
        omhl(args)
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
        let current = self.run(&["get", DIVERGENCE]);
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

// `close` só lê a pasta de spec e o `workflow.toml` quando a retro pode existir

const INVALID_WORKFLOW: &str = "version = 1\ncolour = true\n";

#[test]
fn it_should_close_with_a_null_retro_when_workflow_toml_is_invalid_and_there_is_nothing_to_report()
{
    let fixture = Fixture::new();
    fixture.write_workflow(INVALID_WORKFLOW);

    let closed = fixture.close();

    assert_eq!(retro_of(&closed), Value::Null);
}

#[test]
fn it_should_close_with_a_null_retro_when_workflow_toml_is_invalid_and_the_only_finding_is_an_open_gap()
 {
    let fixture = Fixture::new();
    fixture.append(vec![gap(GAP)]);
    fixture.write_workflow(INVALID_WORKFLOW);

    let closed = fixture.close();

    assert_eq!(retro_of(&closed), Value::Null);
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
