mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use hexlog::{Config, Deps, Hexlog, Lineage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::child::{Output as Child, omhl};
use support::git::git_in;
use support::omhl::{acme, work_item_data};
use support::project::Project;
use support::{EmptyNative, FixedClock, SeqIds, sid};

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const PART_DIR: &str = "wi-w1__login";
const BUSINESS_PREMISE: &str = "premises.login-rule";
const DIVERGENCE: &str = "divergences.login-cmp-001";
const OTHER_DIVERGENCE: &str = "divergences.login-cmp-002";
const CANARY: &str = "CANARY-8f3a1c";

const AMENDMENT_PATH: &str = "/items/0/body/data/amendment";
const RESOLUTION_PATH: &str = "/items/0/body/data/resolution";
const WORK_ITEM_PATH: &str = "/items/0/body/data/work_item";

fn valid_part() -> Vec<&'static str> {
    vec![
        "## Objetivo",
        "",
        "Permitir o login.",
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
}

fn amendment_for(divergence: &'static str, corrected: &'static str) -> Vec<String> {
    vec![
        "## Divergência".to_owned(),
        String::new(),
        divergence.to_owned(),
        String::new(),
        "## Correções".to_owned(),
        String::new(),
        format!("- EMD-001 {corrected}: o sistema autentica o usuário e registra o acesso."),
        String::new(),
        "## Motivo".to_owned(),
        String::new(),
        "A spec omitia o registro de acesso.".to_owned(),
    ]
}

fn valid_amendment() -> Vec<String> {
    amendment_for(DIVERGENCE, "CMP-001")
}

fn text(lines: &[impl AsRef<str>]) -> String {
    let lines: Vec<&str> = lines.iter().map(AsRef::as_ref).collect();

    format!("{}\n", lines.join("\n"))
}

fn confirmed_by() -> Value {
    json!({ "by": "person:ana", "question": "A emenda está certa?", "answer": "Sim, está certa." })
}

fn node(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

fn divergence_data(resolution: &str) -> Value {
    json!({
        "source_ref": "CMP-001",
        "kind": "partial",
        "text": "o código registra o acesso e a spec não diz",
        "evidence": [{ "source": "docs", "summary": "o doc diz isso" }],
        "resolution": resolution,
        "work_item": W1,
    })
}

fn spec_stale_data(amendment: &str) -> Value {
    let mut data = divergence_data("spec-stale");
    data["confirmed_by"] = confirmed_by();
    data["amendment"] = json!(amendment);

    data
}

fn without(mut item: Value, field: &str) -> Value {
    item["body"]["data"]
        .as_object_mut()
        .expect("the item has data")
        .remove(field);

    item
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_of(output: &Child) -> Value {
    json_line(&output.stderr)["error"].clone()
}

/// Cada detalhe do erro vira `(path, code)`.
fn details_of(error: &Value) -> Vec<(String, String)> {
    error["details"]
        .as_array()
        .expect("details is an array")
        .iter()
        .map(|detail| {
            (
                detail["path"].as_str().expect("detail path").to_owned(),
                detail["code"].as_str().expect("detail code").to_owned(),
            )
        })
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(path, code)| ((*path).to_owned(), (*code).to_owned()))
        .collect()
}

fn assert_refused(output: &Child, code: &str, details: &[(&str, &str)]) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    let error = error_of(output);
    assert_eq!(error["code"], json!(code), "error: {error}");
    assert_eq!(details_of(&error), pairs(details), "error: {error}");
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Repositório git real com o marcador commitado, o work item `w1` aberto, a premissa de negócio
/// confirmada `premises.login-rule` e a divergência aberta `divergences.login-cmp-001`
/// (`source_ref` `CMP-001`).
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
        let premise = json!({
            "text": "t", "kind": "business", "work_item": W1,
            "status": "confirmed", "confirmed_by": confirmed_by(),
        });
        fixture.append(vec![
            node(BUSINESS_PREMISE, "premise", premise),
            node(DIVERGENCE, "divergence", divergence_data("open")),
        ]);

        fixture
    }

    /// A spec do `w1` com `parte-01.md` válida e `emenda-01.md` válida para `DIVERGENCE`.
    fn with_spec() -> Fixture {
        let fixture = Fixture::new();
        fixture.write("parte-01.md", &valid_part());
        fixture.write("emenda-01.md", &valid_amendment());

        fixture
    }

    fn run(&self, args: &[&str]) -> Child {
        omhl(args)
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .run()
    }

    fn try_append(&self, items: Vec<Value>) -> Child {
        let mut request = json!({});
        request["items"] = Value::Array(items);

        omhl(&["append"])
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .stdin(request.to_string())
            .run()
    }

    fn append(&self, items: Vec<Value>) {
        let appended = self.try_append(items);

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }

    fn validate(&self) -> Child {
        self.run(&["spec", "validate", W1])
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

    /// Nome e hash de cada arquivo da pasta da spec.
    fn spec_files(&self) -> BTreeMap<String, String> {
        fs::read_dir(self.spec_dir())
            .expect("the spec directory is read")
            .map(|entry| {
                let entry = entry.expect("the entry is read");
                let bytes = fs::read(entry.path()).expect("the file is read");

                (
                    entry.file_name().to_string_lossy().into_owned(),
                    sha256_hex(&bytes),
                )
            })
            .collect()
    }

    fn current_record(&self, id: &str) -> Value {
        let current = self.run(&["get", id]);
        assert_eq!(current.code, Some(0), "stderr: {}", current.stderr);

        json_line(&current.stdout)["node"]["record_id"].clone()
    }

    /// A revisão de aprovação da divergência `id`: `spec-stale` com `base`, `amendment` e `confirmed_by`.
    fn approval_of(&self, id: &str, amendment: &str) -> Value {
        let mut revision = node(id, "divergence", spec_stale_data(amendment));
        revision["base"] = self.current_record(id);

        revision
    }

    fn approval(&self, amendment: &str) -> Value {
        self.approval_of(DIVERGENCE, amendment)
    }

    /// O que a cadeia tem hoje: quantidade de records e a cabeça.
    fn chain(&self) -> Value {
        let verified = self.run(&["verify"]);
        assert_eq!(verified.code, Some(0), "stderr: {}", verified.stderr);
        let report = json_line(&verified.stdout);
        assert_eq!(report["ok"], json!(true), "the chain verifies");

        json!({ "total_records": report["total_records"], "chain_head": report["chain_head"] })
    }

    /// As revisões da lineage da divergência, lidas direto do banco.
    fn divergence_revisions(&self) -> Vec<Value> {
        let clock = FixedClock::default();
        let ids = SeqIds::new();
        let deps = Deps {
            clock: &clock,
            ids: &ids,
            catalog: omhl::graphs::catalog().expect("the embedded catalogs build"),
            native: &EmptyNative,
        };
        let mut hexlog = Hexlog::open(&Config::new(self.project.db_path()), deps)
            .expect("the reader opens the database");

        hexlog
            .lineage_records(&acme(), &Lineage::Node(sid(DIVERGENCE)))
            .expect("the lineage is read")
            .into_iter()
            .map(|revision| revision.data.expect("a live revision"))
            .collect()
    }
}

fn resolutions(revisions: &[Value]) -> Vec<Value> {
    revisions
        .iter()
        .map(|data| data["resolution"].clone())
        .collect()
}

fn amendments_of(output: &Child) -> Value {
    json_line(&output.stdout)["amendments"].clone()
}

fn pending_amendment() -> Value {
    json!([{ "name": "emenda-01.md", "state": "pending", "violations": [] }])
}

fn approved_amendment() -> Value {
    json!([{ "name": "emenda-01.md", "state": "approved", "violations": [] }])
}

// Ciclo completo: divergência aberta, emenda criada e validada, revisão `spec-stale` aceita

#[test]
fn it_should_show_a_valid_amendment_as_pending_before_the_approval() {
    let fixture = Fixture::with_spec();

    let output = fixture.validate();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["ok"], json!(true));
    assert_eq!(amendments_of(&output), pending_amendment());
}

#[test]
fn it_should_accept_the_spec_stale_revision_that_cites_a_valid_amendment() {
    let fixture = Fixture::with_spec();
    let before = fixture.chain();

    let output = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    let after = fixture.chain();
    assert_eq!(
        after["total_records"],
        json!(before["total_records"].as_u64().expect("a count") + 1)
    );
}

#[test]
fn it_should_accept_the_approval_while_the_work_item_branch_already_freezes_the_spec() {
    let fixture = Fixture::with_spec();
    let branch = git_in(fixture.project.dir(), &["branch", "feat/wi-w1__login"]);
    assert!(branch.status.success(), "the work item branch is created");

    let output = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_keep_the_open_revision_first_in_the_lineage_of_the_divergence() {
    let fixture = Fixture::with_spec();
    fixture.append(vec![fixture.approval("emenda-01")]);

    let revisions = fixture.divergence_revisions();

    assert_eq!(
        resolutions(&revisions),
        vec![json!("open"), json!("spec-stale")]
    );
    assert_eq!(revisions[1]["amendment"], json!("emenda-01"));
}

#[test]
fn it_should_leave_every_file_of_the_spec_folder_unchanged_by_the_approval() {
    let fixture = Fixture::with_spec();
    let before = fixture.spec_files();

    fixture.append(vec![fixture.approval("emenda-01")]);

    assert_eq!(fixture.spec_files(), before);
}

#[test]
fn it_should_show_the_amendment_as_approved_after_the_approval() {
    let fixture = Fixture::with_spec();
    fixture.append(vec![fixture.approval("emenda-01")]);

    let output = fixture.validate();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(amendments_of(&output), approved_amendment());
}

// Recusas: cada uma com o código e sem record novo na cadeia

#[test]
fn it_should_refuse_an_approval_whose_amendment_file_is_missing() {
    let fixture = Fixture::with_spec();
    let before = fixture.chain();

    let output = fixture.try_append(vec![fixture.approval("emenda-02")]);

    assert_refused(
        &output,
        "amendment-invalid",
        &[(AMENDMENT_PATH, "amendment-not-found")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_an_approval_whose_work_item_has_no_spec_folder() {
    let fixture = Fixture::new();
    let before = fixture.chain();

    let output = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_refused(
        &output,
        "amendment-invalid",
        &[(AMENDMENT_PATH, "amendment-not-found")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_an_approval_whose_amendment_corrects_an_item_that_does_not_exist() {
    let fixture = Fixture::with_spec();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE, "CMP-999"));
    let before = fixture.chain();

    let output = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_refused(
        &output,
        "amendment-invalid",
        &[
            ("emenda-01.md:3", "amendment-correction-missing"),
            ("emenda-01.md:7", "correction-target-unknown"),
        ],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_list_the_same_violations_in_spec_validate_for_the_invalid_pending_amendment() {
    let fixture = Fixture::with_spec();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE, "CMP-999"));

    let output = fixture.validate();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        amendments_of(&output),
        json!([{
            "name": "emenda-01.md",
            "state": "pending",
            "violations": [
                { "path": "emenda-01.md:3", "code": "amendment-correction-missing" },
                { "path": "emenda-01.md:7", "code": "correction-target-unknown" },
            ],
        }])
    );
}

#[test]
fn it_should_not_echo_the_amendment_content_when_it_refuses_the_approval() {
    let fixture = Fixture::with_spec();
    let mut lines = amendment_for(DIVERGENCE, "CMP-999");
    lines[6] = format!("- EMD-001 CMP-999: {CANARY}");
    lines[10] = format!("Motivo {CANARY}");
    fixture.write("emenda-01.md", &lines);

    let output = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(error_of(&output)["code"], json!("amendment-invalid"));
    assert!(!output.stderr.contains(CANARY), "stderr: {}", output.stderr);
    assert!(!output.stdout.contains(CANARY), "stdout: {}", output.stdout);
}

#[test]
fn it_should_refuse_an_approval_whose_amendment_cites_another_divergence() {
    let fixture = Fixture::with_spec();
    fixture.append(vec![node(
        OTHER_DIVERGENCE,
        "divergence",
        divergence_data("open"),
    )]);
    fixture.write("emenda-01.md", &amendment_for(OTHER_DIVERGENCE, "CMP-001"));
    let before = fixture.chain();

    let output = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_refused(
        &output,
        "amendment-invalid",
        &[("emenda-01.md:3", "amendment-divergence-mismatch")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_an_approval_whose_revision_moves_the_divergence_to_another_work_item() {
    let fixture = Fixture::with_spec();
    fixture.append(vec![node(W2, "work-item", work_item_data("t"))]);
    let mut moved = fixture.approval("emenda-01");
    moved["body"]["data"]["work_item"] = json!(W2);
    let before = fixture.chain();

    let output = fixture.try_append(vec![moved]);

    assert_refused(
        &output,
        "amendment-invalid",
        &[(WORK_ITEM_PATH, "amendment-work-item-mismatch")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_an_approval_of_a_divergence_that_is_already_spec_stale() {
    let fixture = Fixture::with_spec();
    fixture.append(vec![fixture.approval("emenda-01")]);
    let before = fixture.chain();

    let output = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_refused(
        &output,
        "amendment-divergence-not-open",
        &[(RESOLUTION_PATH, "amendment-divergence-not-open")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_a_new_divergence_born_spec_stale_without_a_base() {
    let fixture = Fixture::with_spec();
    let born_spec_stale = node(OTHER_DIVERGENCE, "divergence", spec_stale_data("emenda-01"));
    let before = fixture.chain();

    let output = fixture.try_append(vec![born_spec_stale]);

    assert_refused(
        &output,
        "amendment-divergence-not-open",
        &[(RESOLUTION_PATH, "amendment-divergence-not-open")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_refuse_an_approval_without_confirmed_by_through_the_guard() {
    let fixture = Fixture::with_spec();
    let before = fixture.chain();

    let output = fixture.try_append(vec![without(fixture.approval("emenda-01"), "confirmed_by")]);

    assert_refused(
        &output,
        "confirmation-required",
        &[("/items/0/body/data/confirmed_by", "confirmation-required")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_leave_an_approval_without_amendment_to_the_guard() {
    let fixture = Fixture::with_spec();
    let before = fixture.chain();

    let output = fixture.try_append(vec![without(fixture.approval("emenda-01"), "amendment")]);

    assert_refused(
        &output,
        "confirmation-required",
        &[(AMENDMENT_PATH, "confirmation-required")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_leave_an_amendment_name_outside_the_emenda_form_to_the_guard() {
    let fixture = Fixture::with_spec();
    let before = fixture.chain();

    let output = fixture.try_append(vec![fixture.approval("../parte-01")]);

    assert_refused(
        &output,
        "confirmation-invalid",
        &[(AMENDMENT_PATH, "confirmation-invalid")],
    );
    assert_eq!(fixture.chain(), before);
}

// Lote atômico: a recusa da revisão não deixa gravar o item válido

#[test]
fn it_should_record_none_of_a_batch_whose_spec_stale_revision_is_refused() {
    let fixture = Fixture::with_spec();
    let gap = node(
        "gaps.login-timeout",
        "gap",
        json!({ "question": "q", "context": "c", "work_item": W1 }),
    );
    let before = fixture.chain();

    let output = fixture.try_append(vec![gap, fixture.approval("emenda-02")]);

    assert_refused(
        &output,
        "amendment-invalid",
        &[("/items/1/body/data/amendment", "amendment-not-found")],
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_not_create_the_valid_node_of_a_refused_batch() {
    let fixture = Fixture::with_spec();
    let gap = node(
        "gaps.login-timeout",
        "gap",
        json!({ "question": "q", "context": "c", "work_item": W1 }),
    );

    fixture.try_append(vec![gap, fixture.approval("emenda-02")]);

    let found = fixture.run(&["get", "gaps.login-timeout"]);
    assert_eq!(json_line(&found.stdout)["node"], Value::Null);
}

// CLI: o veredito e a cadeia intacta na recusa

#[test]
fn it_should_accept_the_approval_through_the_cli_append() {
    let fixture = Fixture::with_spec();
    let before = fixture.chain();

    fixture.append(vec![fixture.approval("emenda-01")]);

    assert_eq!(
        fixture.divergence_revisions().len(),
        2,
        "the lineage gains the approval"
    );
    assert_eq!(
        fixture.chain()["total_records"],
        json!(before["total_records"].as_u64().expect("a count") + 1)
    );
}

#[test]
fn it_should_leave_the_chain_intact_when_the_cli_refuses_a_missing_amendment() {
    let fixture = Fixture::with_spec();
    let before = fixture.chain();

    let cli = fixture.try_append(vec![fixture.approval("emenda-02")]);

    assert_eq!(error_of(&cli)["code"], json!("amendment-invalid"));
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_leave_the_chain_intact_when_the_cli_refuses_a_divergence_that_is_not_open() {
    let fixture = Fixture::with_spec();
    fixture.append(vec![fixture.approval("emenda-01")]);
    let before = fixture.chain();

    let cli = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_eq!(
        error_of(&cli)["code"],
        json!("amendment-divergence-not-open")
    );
    assert_eq!(fixture.chain(), before);
}

#[test]
fn it_should_leave_the_chain_intact_when_the_cli_refuses_an_invalid_amendment() {
    let fixture = Fixture::with_spec();
    fixture.write("emenda-01.md", &amendment_for(DIVERGENCE, "CMP-999"));
    let before = fixture.chain();

    let cli = fixture.try_append(vec![fixture.approval("emenda-01")]);

    assert_eq!(error_of(&cli)["code"], json!("amendment-invalid"));
    assert_eq!(fixture.chain(), before);
}
