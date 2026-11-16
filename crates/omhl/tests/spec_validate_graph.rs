mod support;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use ::omhl::workflow::config::{SpecsConfig, WorkflowConfig};
use ::omhl::workflow::gap::{Answerer, GapState};
use ::omhl::workflow::spec::{
    AmendmentState, DivergenceFact, SpecFacts, SpecFile, amendment_state, validate_graph,
};
use serde_json::{Value, json};
use support::child::{Output as Child, omhl};
use support::ext::ExtTree;
use support::git::git_in;
use support::omhl::work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const PART_DIR: &str = "wi-w1__login";
const WORKFLOW_PATH: &str = ".omhl/workflow.toml";
const BUSINESS_PREMISE: &str = "premises.login-rule";
const DIVERGENCE: &str = "divergences.login-cmp-001";
const NATIVE_DIRECTIVE: &str = "guidelines.one-decision-per-turn";

// Linhas das fixtures: o índice no vetor é a linha menos um.
const PART_OBJECTIVE_LINE: usize = 3;
const PART_DIRECTIVES_HEADING_LINE: usize = 5;
const PART_FIRST_CITATION_LINE: usize = 7;
const PART_SECOND_CITATION_LINE: usize = 8;
const AMENDMENT_DIVERGENCE_LINE: usize = 3;
const AMENDMENT_CORRECTION_LINE: usize = 7;
const AMENDMENT_MOTIVE_BODY_LINE: usize = 11;

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

fn valid_amendment() -> Vec<&'static str> {
    vec![
        "## Divergência",
        "",
        "divergences.login-cmp-001",
        "",
        "## Correções",
        "",
        "- EMD-001 CMP-001: o sistema autentica o usuário e registra o acesso.",
        "",
        "## Motivo",
        "",
        "A spec omitia o registro de acesso.",
    ]
}

fn text(lines: &[&str]) -> String {
    format!("{}\n", lines.join("\n"))
}

fn with_line<'a>(mut lines: Vec<&'a str>, line: usize, replacement: &'a str) -> Vec<&'a str> {
    lines[line - 1] = replacement;

    lines
}

// Camada pura: `omhl::workflow::spec::{validate_graph, amendment_state}` com fatos de mentira

struct FakeNode {
    confirmed: bool,
    kind: Option<&'static str>,
    work_item: Option<&'static str>,
}

#[derive(Default)]
struct FakeFacts {
    nodes: HashMap<String, FakeNode>,
    gaps: HashMap<String, GapState>,
    divergences: HashMap<String, DivergenceFact>,
    approved: Vec<(String, String)>,
}

impl FakeFacts {
    fn premise(
        mut self,
        id: &str,
        kind: &'static str,
        work_item: &'static str,
        confirmed: bool,
    ) -> FakeFacts {
        let node = FakeNode {
            confirmed,
            kind: Some(kind),
            work_item: Some(work_item),
        };
        self.nodes.insert(id.to_owned(), node);

        self
    }

    fn guideline(mut self, id: &str, confirmed: bool) -> FakeFacts {
        let node = FakeNode {
            confirmed,
            kind: None,
            work_item: None,
        };
        self.nodes.insert(id.to_owned(), node);

        self
    }

    fn gap(mut self, id: &str, state: GapState) -> FakeFacts {
        self.gaps.insert(id.to_owned(), state);

        self
    }

    fn divergence(mut self, id: &str, work_item: &str, resolution: &str, source_ref: &str) -> Self {
        let fact = DivergenceFact {
            work_item: work_item.to_owned(),
            resolution: resolution.to_owned(),
            source_ref: source_ref.to_owned(),
        };
        self.divergences.insert(id.to_owned(), fact);

        self
    }

    fn approved(mut self, work_item: &str, amendment: &str) -> FakeFacts {
        self.approved
            .push((work_item.to_owned(), amendment.to_owned()));

        self
    }
}

impl SpecFacts for FakeFacts {
    fn exists(&self, id: &str) -> bool {
        self.nodes.contains_key(id)
    }

    fn confirmed(&self, id: &str) -> bool {
        self.nodes.get(id).is_some_and(|node| node.confirmed)
    }

    fn kind(&self, id: &str) -> Option<String> {
        self.nodes.get(id)?.kind.map(str::to_owned)
    }

    fn work_item(&self, id: &str) -> Option<String> {
        self.nodes.get(id)?.work_item.map(str::to_owned)
    }

    fn gap_state(&self, gap: &str) -> GapState {
        self.gaps.get(gap).copied().unwrap_or(GapState::Open)
    }

    fn divergence(&self, id: &str) -> Option<DivergenceFact> {
        self.divergences.get(id).cloned()
    }

    fn amendment_approved(&self, work_item: &str, amendment: &str) -> bool {
        self.approved
            .iter()
            .any(|(item, name)| item == work_item && name == amendment)
    }
}

/// O que `valid_part` e `valid_amendment` pedem: a premissa de negócio confirmada do `W1`, uma
/// diretiva confirmada e a divergência aberta do `W1` cujo `source_ref` é `CMP-001`.
fn base_facts() -> FakeFacts {
    FakeFacts::default()
        .premise(BUSINESS_PREMISE, "business", W1, true)
        .guideline("guidelines.tone", true)
        .divergence(DIVERGENCE, W1, "open", "CMP-001")
}

fn default_config() -> SpecsConfig {
    WorkflowConfig::default().specs
}

/// Cada violação vira `"<arquivo>:<linha> <código>"`, na ordem devolvida.
fn graph_violations(
    part_lines: &[&str],
    amendment_lines: Option<&[&str]>,
    config: &SpecsConfig,
    facts: &FakeFacts,
) -> Vec<String> {
    let part_text = text(part_lines);
    let amendment_text = amendment_lines.map(text);
    let parts = [SpecFile {
        name: "parte-01.md",
        text: &part_text,
    }];
    let amendments: Vec<SpecFile> = amendment_text
        .iter()
        .map(|text| SpecFile {
            name: "emenda-01.md",
            text,
        })
        .collect();

    validate_graph(W1, &parts, &amendments, config, facts)
        .iter()
        .map(|violation| format!("{} {}", violation.path, violation.code))
        .collect()
}

fn part_violations(lines: &[&str], facts: &FakeFacts) -> Vec<String> {
    graph_violations(lines, None, &default_config(), facts)
}

fn amendment_violations(lines: &[&str], facts: &FakeFacts) -> Vec<String> {
    graph_violations(&valid_part(), Some(lines), &default_config(), facts)
}

fn answered() -> GapState {
    GapState::Answered {
        by: Answerer::Human,
        criterion: true,
    }
}

// Partes: ids citados

#[test]
fn it_should_accept_a_spec_whose_citations_and_amendment_all_resolve() {
    let violations = graph_violations(
        &valid_part(),
        Some(&valid_amendment()),
        &default_config(),
        &base_facts(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_a_cited_id_that_does_not_exist() {
    let lines = with_line(valid_part(), PART_SECOND_CITATION_LINE, "- premises.ghost");

    let violations = part_violations(&lines, &base_facts());

    assert_eq!(violations, vec!["parte-01.md:8 cited-id-unknown"]);
}

#[test]
fn it_should_reject_a_cited_premise_that_is_not_confirmed() {
    let facts = base_facts().premise("premises.draft", "technical", W1, false);
    let lines = with_line(valid_part(), PART_SECOND_CITATION_LINE, "- premises.draft");

    let violations = part_violations(&lines, &facts);

    assert_eq!(violations, vec!["parte-01.md:8 cited-id-unconfirmed"]);
}

#[test]
fn it_should_reject_a_cited_directive_that_is_not_confirmed() {
    let facts = base_facts().guideline("guidelines.draft", false);
    let lines = with_line(
        valid_part(),
        PART_SECOND_CITATION_LINE,
        "- guidelines.draft",
    );

    let violations = part_violations(&lines, &facts);

    assert_eq!(violations, vec!["parte-01.md:8 cited-id-unconfirmed"]);
}

#[test]
fn it_should_accept_a_cited_confirmed_directive() {
    let lines = with_line(valid_part(), PART_SECOND_CITATION_LINE, "- guidelines.tone");

    let violations = part_violations(&lines, &base_facts());

    assert_eq!(violations, Vec::<String>::new());
}

// Partes: premissa de negócio do próprio work item

fn directive_only_part() -> Vec<&'static str> {
    with_line(valid_part(), PART_FIRST_CITATION_LINE, "- guidelines.tone")
}

#[test]
fn it_should_reject_a_spec_without_a_confirmed_business_premise_when_the_rule_is_on() {
    let violations = part_violations(&directive_only_part(), &base_facts());

    assert_eq!(violations, vec!["parte-01.md:5 business-premise-missing"]);
}

#[test]
fn it_should_accept_a_spec_without_a_business_premise_when_the_rule_is_off() {
    let config = SpecsConfig {
        cite_business_premise: false,
        ..default_config()
    };

    let violations = graph_violations(&directive_only_part(), None, &config, &base_facts());

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_not_count_a_business_premise_of_another_work_item() {
    let facts = FakeFacts::default().premise(BUSINESS_PREMISE, "business", W2, true);

    let violations = part_violations(&valid_part(), &facts);

    assert_eq!(violations, vec!["parte-01.md:5 business-premise-missing"]);
}

#[test]
fn it_should_not_count_a_technical_premise_as_business() {
    let facts = FakeFacts::default().premise(BUSINESS_PREMISE, "technical", W1, true);

    let violations = part_violations(&valid_part(), &facts);

    assert_eq!(violations, vec!["parte-01.md:5 business-premise-missing"]);
}

#[test]
fn it_should_count_a_business_premise_cited_in_another_part() {
    let part_one = text(&directive_only_part());
    let part_two = text(&[
        "## Objetivo",
        "",
        "Encerrar a sessão.",
        "",
        "## Diretivas e premissas aplicáveis",
        "",
        "- premises.login-rule",
        "",
        "## Comportamentos",
        "",
        "- CMP-002: o sistema encerra a sessão.",
        "",
        "## Critérios de aceitação",
        "",
        "- CA-002: logout invalida a sessão.",
        "",
        "## Fora de escopo",
        "",
        "Login.",
    ]);
    let parts = [
        SpecFile {
            name: "parte-01.md",
            text: &part_one,
        },
        SpecFile {
            name: "parte-02.md",
            text: &part_two,
        },
    ];

    let violations = validate_graph(W1, &parts, &[], &default_config(), &base_facts());

    assert!(violations.is_empty(), "violations: {violations:?}");
}

// Partes e emendas: [PENDENTE: gaps.x]

fn pending_marker_part() -> Vec<&'static str> {
    with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "Permitir o login [PENDENTE: gaps.pay-day].",
    )
}

#[test]
fn it_should_reject_a_pending_marker_that_points_to_an_open_gap() {
    let facts = base_facts().gap("gaps.pay-day", GapState::Open);

    let violations = part_violations(&pending_marker_part(), &facts);

    assert_eq!(violations, vec!["parte-01.md:3 pending-gap-unresolved"]);
}

#[test]
fn it_should_reject_a_pending_marker_that_points_to_a_gap_that_needs_a_human() {
    let facts = base_facts().gap("gaps.pay-day", GapState::NeedsHuman);

    let violations = part_violations(&pending_marker_part(), &facts);

    assert_eq!(violations, vec!["parte-01.md:3 pending-gap-unresolved"]);
}

#[test]
fn it_should_reject_a_pending_marker_that_points_to_a_gap_that_does_not_exist() {
    let violations = part_violations(&pending_marker_part(), &base_facts());

    assert_eq!(violations, vec!["parte-01.md:3 pending-gap-unresolved"]);
}

#[test]
fn it_should_accept_a_pending_marker_that_points_to_an_answered_gap() {
    let facts = base_facts().gap("gaps.pay-day", answered());

    let violations = part_violations(&pending_marker_part(), &facts);

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_a_pending_marker_inside_an_amendment_that_points_to_an_open_gap() {
    let facts = base_facts().gap("gaps.pay-day", GapState::Open);
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_MOTIVE_BODY_LINE,
        "Depende de [PENDENTE: gaps.pay-day].",
    );

    let violations = amendment_violations(&lines, &facts);

    assert_eq!(violations, vec!["emenda-01.md:11 pending-gap-unresolved"]);
}

// Emendas: divergência citada

#[test]
fn it_should_reject_an_amendment_that_cites_a_divergence_that_does_not_exist() {
    let facts = FakeFacts::default().premise(BUSINESS_PREMISE, "business", W1, true);

    let violations = amendment_violations(&valid_amendment(), &facts);

    assert_eq!(
        violations,
        vec!["emenda-01.md:3 amendment-divergence-unknown"]
    );
}

#[test]
fn it_should_reject_an_amendment_that_cites_a_divergence_of_another_work_item() {
    let facts = base_facts().divergence(DIVERGENCE, W2, "open", "CMP-001");

    let violations = amendment_violations(&valid_amendment(), &facts);

    assert_eq!(
        violations,
        vec!["emenda-01.md:3 amendment-divergence-foreign"]
    );
}

#[test]
fn it_should_reject_an_amendment_whose_divergence_is_code_fixed() {
    let facts = base_facts().divergence(DIVERGENCE, W1, "code-fixed", "CMP-001");

    let violations = amendment_violations(&valid_amendment(), &facts);

    assert_eq!(
        violations,
        vec!["emenda-01.md:3 amendment-divergence-closed"]
    );
}

#[test]
fn it_should_reject_an_amendment_whose_divergence_is_accepted() {
    let facts = base_facts().divergence(DIVERGENCE, W1, "accepted", "CMP-001");

    let violations = amendment_violations(&valid_amendment(), &facts);

    assert_eq!(
        violations,
        vec!["emenda-01.md:3 amendment-divergence-closed"]
    );
}

#[test]
fn it_should_accept_an_amendment_whose_divergence_is_already_spec_stale() {
    let facts = base_facts().divergence(DIVERGENCE, W1, "spec-stale", "CMP-001");

    let violations = amendment_violations(&valid_amendment(), &facts);

    assert_eq!(violations, Vec::<String>::new());
}

// Emendas: o item de origem da divergência é corrigido

#[test]
fn it_should_reject_an_amendment_that_does_not_correct_the_cmp_of_the_divergence() {
    let facts = base_facts().divergence(DIVERGENCE, W1, "open", "CMP-002");

    let violations = amendment_violations(&valid_amendment(), &facts);

    assert_eq!(
        violations,
        vec!["emenda-01.md:3 amendment-correction-missing"]
    );
}

#[test]
fn it_should_reject_an_amendment_that_does_not_correct_the_ca_of_the_divergence() {
    let facts = base_facts().divergence(DIVERGENCE, W1, "open", "CA-001");

    let violations = amendment_violations(&valid_amendment(), &facts);

    assert_eq!(
        violations,
        vec!["emenda-01.md:3 amendment-correction-missing"]
    );
}

#[test]
fn it_should_not_require_a_correction_when_the_divergence_points_to_a_directive() {
    let facts = base_facts().divergence(DIVERGENCE, W1, "open", BUSINESS_PREMISE);

    let violations = amendment_violations(&valid_amendment(), &facts);

    assert_eq!(violations, Vec::<String>::new());
}

// Estado da emenda

#[test]
fn it_should_call_an_amendment_approved_when_a_spec_stale_divergence_names_it() {
    let facts = base_facts().approved(W1, "emenda-01");

    let state = amendment_state(W1, "emenda-01.md", &facts);

    assert_eq!(state, AmendmentState::Approved);
}

#[test]
fn it_should_call_an_amendment_pending_when_no_divergence_names_it() {
    let state = amendment_state(W1, "emenda-01.md", &base_facts());

    assert_eq!(state, AmendmentState::Pending);
}

#[test]
fn it_should_call_an_amendment_pending_when_the_approval_belongs_to_another_work_item() {
    let facts = base_facts().approved(W2, "emenda-01");

    let state = amendment_state(W1, "emenda-01.md", &facts);

    assert_eq!(state, AmendmentState::Pending);
}

// Camada de CLI: `omhl spec validate work-items.<id>` sobre um banco real

/// Repositório git real com o marcador commitado, os work items `w1` e `w2`, a premissa de negócio
/// confirmada `premises.login-rule` do `w1` e a divergência aberta `divergences.login-cmp-001`.
struct Fixture {
    project: Project,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture::with_files(&[])
    }

    fn with_files(files: &[(&str, &str)]) -> Fixture {
        let project = Project::new("acme");
        for (rel, content) in files {
            let target = project.dir().join(rel);
            fs::create_dir_all(target.parent().expect("the file has a parent")).expect("parent");
            fs::write(target, content).expect("file is written");
        }
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
        fixture.append(
            [W1, W2]
                .iter()
                .map(|id| node(id, "work-item", work_item_data("t")))
                .collect(),
        );
        fixture.append(vec![
            premise(BUSINESS_PREMISE, "business", W1, true),
            node(DIVERGENCE, "divergence", divergence_data("open")),
        ]);

        fixture
    }

    fn run(&self, args: &[&str]) -> Child {
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

    fn validate(&self) -> Child {
        self.run(&["spec", "validate", W1])
    }

    fn write(&self, name: &str, lines: &[&str]) {
        let main_dir = fs::canonicalize(self.project.dir()).expect("the main directory exists");
        let dir: PathBuf = main_dir.join(".omhl/specs").join(PART_DIR);
        fs::create_dir_all(&dir).expect("spec directory is created");
        fs::write(dir.join(name), text(lines)).expect("spec file is written");
    }

    /// A aprovação da emenda: nova revisão da divergência, em `spec-stale`, com `base`.
    fn approve(&self, amendment: &str) {
        let current = self.run(&["get", DIVERGENCE]);
        assert_eq!(current.code, Some(0), "stderr: {}", current.stderr);
        let base = json_line(&current.stdout)["node"]["record_id"].clone();
        let mut data = divergence_data("spec-stale");
        data["confirmed_by"] = confirmed_by();
        data["amendment"] = json!(amendment);
        let mut revision = node(DIVERGENCE, "divergence", data);
        revision["base"] = base;

        self.append(vec![revision]);
    }
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

fn premise(id: &str, kind: &str, work_item: &str, confirmed: bool) -> Value {
    let mut data = json!({
        "text": "t", "kind": kind, "work_item": work_item,
        "status": if confirmed { "confirmed" } else { "assumed" },
    });
    if confirmed {
        data["confirmed_by"] = confirmed_by();
    }

    node(id, "premise", data)
}

fn confirmed_guideline(id: &str) -> Value {
    node(
        id,
        "guideline",
        json!({ "text": "t", "kind": "business", "confirmed_by": confirmed_by() }),
    )
}

fn gap(id: &str) -> Value {
    node(
        id,
        "gap",
        json!({ "question": "q", "context": "c", "work_item": W1 }),
    )
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

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_code(output: &Child) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
}

/// Cada detalhe do erro vira `"<path> <code>"`.
fn error_details(output: &Child) -> Vec<String> {
    json_line(&output.stderr)["error"]["details"]
        .as_array()
        .expect("details is an array")
        .iter()
        .map(|detail| {
            format!(
                "{} {}",
                detail["path"].as_str().expect("detail path"),
                detail["code"].as_str().expect("detail code")
            )
        })
        .collect()
}

fn assert_valid(output: &Child) {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["ok"], json!(true));
}

fn assert_refused_with(output: &Child, details: &[&str]) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(output), json!("spec-invalid"));
    assert_eq!(error_details(output), details);
}

// CLI: ids citados

#[test]
fn it_should_accept_a_spec_that_cites_the_confirmed_business_premise_of_the_work_item() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());

    let output = fixture.validate();

    assert_valid(&output);
}

#[test]
fn it_should_refuse_a_cited_id_that_is_not_in_the_graph() {
    let fixture = Fixture::new();
    fixture.write(
        "parte-01.md",
        &with_line(valid_part(), PART_SECOND_CITATION_LINE, "- premises.ghost"),
    );

    let output = fixture.validate();

    assert_refused_with(&output, &["parte-01.md:8 cited-id-unknown"]);
}

#[test]
fn it_should_refuse_a_cited_premise_that_has_no_confirmed_by() {
    let fixture = Fixture::new();
    fixture.append(vec![premise("premises.draft", "technical", W1, false)]);
    fixture.write(
        "parte-01.md",
        &with_line(valid_part(), PART_SECOND_CITATION_LINE, "- premises.draft"),
    );

    let output = fixture.validate();

    assert_refused_with(&output, &["parte-01.md:8 cited-id-unconfirmed"]);
}

#[test]
fn it_should_accept_a_cited_directive_that_has_confirmed_by() {
    let fixture = Fixture::new();
    fixture.append(vec![confirmed_guideline("guidelines.review")]);
    fixture.write(
        "parte-01.md",
        &with_line(
            valid_part(),
            PART_SECOND_CITATION_LINE,
            "- guidelines.review",
        ),
    );

    let output = fixture.validate();

    assert_valid(&output);
}

#[test]
fn it_should_accept_a_cited_directive_loaded_from_an_extension() {
    let fixture = Fixture::new();
    ExtTree::at(fixture.project.dir()).guideline("tone", "kind = \"business\"\ntext = \"t\"\n");
    let loaded = fixture.run(&["load"]);
    fixture.write(
        "parte-01.md",
        &with_line(valid_part(), PART_SECOND_CITATION_LINE, "- guidelines.tone"),
    );

    let output = fixture.validate();

    assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
    assert_valid(&output);
}

#[test]
fn it_should_accept_a_cited_native_directive() {
    let fixture = Fixture::new();
    let citation = format!("- {NATIVE_DIRECTIVE}");
    fixture.write(
        "parte-01.md",
        &with_line(valid_part(), PART_SECOND_CITATION_LINE, &citation),
    );

    let output = fixture.validate();

    assert_valid(&output);
}

// CLI: premissa de negócio do próprio work item

fn cites_only(id: &str) -> Vec<&str> {
    with_line(valid_part(), PART_FIRST_CITATION_LINE, id)
}

#[test]
fn it_should_refuse_a_spec_that_cites_only_a_business_premise_of_another_work_item() {
    let fixture = Fixture::new();
    fixture.append(vec![premise("premises.other", "business", W2, true)]);
    fixture.write("parte-01.md", &cites_only("- premises.other"));

    let output = fixture.validate();

    assert_refused_with(
        &output,
        &[&format!(
            "parte-01.md:{PART_DIRECTIVES_HEADING_LINE} business-premise-missing"
        )],
    );
}

#[test]
fn it_should_refuse_a_spec_that_cites_only_directives_and_technical_premises() {
    let fixture = Fixture::new();
    fixture.append(vec![
        premise("premises.stack", "technical", W1, true),
        confirmed_guideline("guidelines.review"),
    ]);
    fixture.write(
        "parte-01.md",
        &with_line(
            cites_only("- premises.stack"),
            PART_SECOND_CITATION_LINE,
            "- guidelines.review",
        ),
    );

    let output = fixture.validate();

    assert_refused_with(&output, &["parte-01.md:5 business-premise-missing"]);
}

#[test]
fn it_should_accept_a_spec_without_a_business_premise_when_the_workflow_file_turns_the_rule_off() {
    let fixture = Fixture::with_files(&[(
        WORKFLOW_PATH,
        "version = 1\n\n[specs]\ncite_business_premise = false\n",
    )]);
    fixture.append(vec![confirmed_guideline("guidelines.review")]);
    fixture.write("parte-01.md", &cites_only("- guidelines.review"));

    let output = fixture.validate();

    assert_valid(&output);
}

// CLI: [PENDENTE: gaps.x]

#[test]
fn it_should_refuse_a_pending_marker_that_points_to_an_open_gap() {
    let fixture = Fixture::new();
    fixture.append(vec![gap("gaps.pay-day")]);
    fixture.write("parte-01.md", &pending_marker_part());

    let output = fixture.validate();

    assert_refused_with(&output, &["parte-01.md:3 pending-gap-unresolved"]);
}

#[test]
fn it_should_accept_the_pending_marker_once_a_premise_answers_the_gap() {
    let fixture = Fixture::new();
    fixture.append(vec![gap("gaps.pay-day")]);
    fixture.write("parte-01.md", &pending_marker_part());
    let before = fixture.validate();
    fixture.append(vec![premise("premises.pay-day", "technical", W1, false)]);
    fixture.append(vec![relation(
        "answers",
        "premises.pay-day",
        "gaps.pay-day",
    )]);

    let after = fixture.validate();

    assert_refused_with(&before, &["parte-01.md:3 pending-gap-unresolved"]);
    assert_valid(&after);
}

#[test]
fn it_should_refuse_a_pending_marker_whose_gap_answer_contradicts_a_directive() {
    let fixture = Fixture::new();
    fixture.append(vec![
        gap("gaps.pay-day"),
        premise("premises.pay-day", "technical", W1, false),
        confirmed_guideline("guidelines.review"),
    ]);
    fixture.append(vec![
        relation("answers", "premises.pay-day", "gaps.pay-day"),
        relation("contradicts", "premises.pay-day", "guidelines.review"),
    ]);
    fixture.write("parte-01.md", &pending_marker_part());

    let output = fixture.validate();

    assert_refused_with(&output, &["parte-01.md:3 pending-gap-unresolved"]);
}

// CLI: os três estados de emenda

fn amendments_of(output: &Child) -> Value {
    json_line(&output.stdout)["amendments"].clone()
}

#[test]
fn it_should_list_a_valid_amendment_without_an_approval_as_pending() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write("emenda-01.md", &valid_amendment());

    let output = fixture.validate();

    assert_valid(&output);
    assert_eq!(
        amendments_of(&output),
        json!([{ "name": "emenda-01.md", "state": "pending", "violations": [] }])
    );
}

#[test]
fn it_should_list_a_pending_amendment_with_a_graph_violation_without_failing_ok() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write(
        "emenda-01.md",
        &with_line(
            valid_amendment(),
            AMENDMENT_DIVERGENCE_LINE,
            "divergences.ghost",
        ),
    );

    let output = fixture.validate();

    assert_valid(&output);
    assert_eq!(
        amendments_of(&output),
        json!([{
            "name": "emenda-01.md",
            "state": "pending",
            "violations": [{ "path": "emenda-01.md:3", "code": "amendment-divergence-unknown" }],
        }])
    );
}

#[test]
fn it_should_list_a_valid_amendment_as_approved_once_its_divergence_is_spec_stale() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write("emenda-01.md", &valid_amendment());
    fixture.approve("emenda-01");

    let output = fixture.validate();

    assert_valid(&output);
    assert_eq!(
        amendments_of(&output),
        json!([{ "name": "emenda-01.md", "state": "approved", "violations": [] }])
    );
}

#[test]
fn it_should_keep_another_amendment_of_the_work_item_pending_when_one_is_approved() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write("emenda-01.md", &valid_amendment());
    fixture.write(
        "emenda-02.md",
        &with_line(
            valid_amendment(),
            AMENDMENT_CORRECTION_LINE,
            "- EMD-002 CMP-001: o sistema autentica e registra o acesso.",
        ),
    );
    fixture.approve("emenda-01");

    let output = fixture.validate();

    assert_eq!(
        amendments_of(&output)
            .as_array()
            .expect("amendments is an array")
            .iter()
            .map(|amendment| amendment["state"].clone())
            .collect::<Vec<_>>(),
        vec![json!("approved"), json!("pending")]
    );
}

#[test]
fn it_should_refuse_the_spec_when_an_approved_amendment_has_a_violation() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write("emenda-01.md", &valid_amendment());
    fixture.approve("emenda-01");
    fixture.write(
        "emenda-01.md",
        &with_line(
            valid_amendment(),
            AMENDMENT_MOTIVE_BODY_LINE,
            "Motivo TODO.",
        ),
    );

    let output = fixture.validate();

    assert_refused_with(&output, &["emenda-01.md:11 placeholder-found"]);
}

#[test]
fn it_should_refuse_the_spec_when_an_approved_amendment_has_a_graph_violation() {
    let fixture = Fixture::new();
    fixture.append(vec![gap("gaps.pay-day")]);
    fixture.write("parte-01.md", &valid_part());
    fixture.write("emenda-01.md", &valid_amendment());
    fixture.approve("emenda-01");
    fixture.write(
        "emenda-01.md",
        &with_line(
            valid_amendment(),
            AMENDMENT_MOTIVE_BODY_LINE,
            "Depende de [PENDENTE: gaps.pay-day].",
        ),
    );

    let output = fixture.validate();

    assert_refused_with(&output, &["emenda-01.md:11 pending-gap-unresolved"]);
}
