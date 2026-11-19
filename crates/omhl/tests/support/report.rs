use std::fs;

use serde_json::{Value, json};

use super::git::git_in;
use super::inproc::run_inproc;
use super::omhl::work_item_data;
use super::project::Project;

// Estados de fixture dos relatórios (`report answers|recurrences|retro`), montados pela CLI real em processo.
// Nenhum id nem texto repete um valor de vocabulário fechado (`open`, `partial`, `human`, `business`...).

pub const WORK_ITEM: &str = "work-items.payroll";
pub const OTHER_WORK_ITEM: &str = "work-items.invoice";
pub const THIRD_WORK_ITEM: &str = "work-items.ledger";
pub const ABSENT_WORK_ITEM: &str = "work-items.ghost";

pub const GAP_UNANSWERED: &str = "gaps.rounding";
pub const GAP_HUMAN: &str = "gaps.currency";
pub const GAP_CRITERION: &str = "gaps.timezone";
pub const GAP_CONTESTED: &str = "gaps.locale";

pub const PREMISE_HUMAN: &str = "premises.cents";
pub const PREMISE_AUTONOMOUS: &str = "premises.utc-rule";
pub const PREMISE_CONTESTED: &str = "premises.dst-rule";
pub const DIRECTIVE: &str = "guidelines.bind-params";

pub const DIVERGENCE_REPORT: &str = "divergences.report";
pub const DIVERGENCE_AUDIT: &str = "divergences.audit";
pub const DIVERGENCE_LAYOUT: &str = "divergences.layout";
pub const APPROVED_AMENDMENT: &str = "emenda-01.md";
pub const PENDING_AMENDMENT: &str = "emenda-02.md";

/// O texto repetido em três work items; cita um id que não é folha do relatório.
pub const RECURRING_TEXT: &str = "Money lives in cents, see decisions.journal-mode.";

const SPEC_FOLDER: &str = "wi-payroll__rollout";
const BUSINESS_PREMISE: &str = "premises.login-rule";

fn confirmed_by() -> Value {
    json!({ "by": "person:ana", "question": "Is it right?", "answer": "Yes, it is." })
}

fn node(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

fn relation(relation_type: &str, from: &str, to: &str) -> Value {
    json!({ "body": { "kind": "relation", "relation_type": relation_type, "from": from, "to": to } })
}

fn append(project: &Project, items: Vec<Value>) {
    let mut request = json!({});
    request["items"] = Value::Array(items);

    let appended = run_inproc(
        project,
        &["--format", "json", "append"],
        &request.to_string(),
    );

    assert_eq!(appended.stderr, "", "the fixture append failed");
}

fn work_item(id: &str) -> Value {
    node(id, "work-item", work_item_data("Payroll rollout"))
}

/// Só o work item: o banco existe e não há gap, premissa nem divergência.
pub fn bare_project() -> Project {
    let project = Project::new("acme");
    append(&project, vec![work_item(WORK_ITEM)]);

    project
}

fn gap(id: &str) -> Value {
    node(
        id,
        "gap",
        json!({ "question": "Which rule applies?", "context": "It is unclear.", "work_item": WORK_ITEM }),
    )
}

fn autonomous_premise(id: &str) -> Value {
    node(
        id,
        "premise",
        json!({ "text": "Assumed rule.", "status": "assumed", "kind": "technical", "work_item": WORK_ITEM }),
    )
}

fn human_premise(id: &str) -> Value {
    node(
        id,
        "premise",
        json!({
            "text": "Amounts are whole cents.", "status": "confirmed", "kind": "business",
            "work_item": WORK_ITEM, "confirmed_by": confirmed_by(),
        }),
    )
}

fn directive() -> Value {
    node(
        DIRECTIVE,
        "guideline",
        json!({ "text": "Every query binds its parameters.", "kind": "business", "confirmed_by": confirmed_by() }),
    )
}

/// Quatro gaps: um aberto, um respondido por pessoa sem diretiva, um respondido por agente com diretiva e um
/// respondido por agente contra a diretiva (precisa de humano).
pub fn answers_project() -> Project {
    let project = Project::new("acme");
    append(&project, vec![work_item(WORK_ITEM)]);
    append(
        &project,
        vec![
            gap(GAP_UNANSWERED),
            gap(GAP_HUMAN),
            gap(GAP_CRITERION),
            gap(GAP_CONTESTED),
            human_premise(PREMISE_HUMAN),
            autonomous_premise(PREMISE_AUTONOMOUS),
            autonomous_premise(PREMISE_CONTESTED),
            directive(),
        ],
    );
    append(
        &project,
        vec![
            relation("answers", PREMISE_HUMAN, GAP_HUMAN),
            relation("answers", PREMISE_AUTONOMOUS, GAP_CRITERION),
            relation("supports", DIRECTIVE, PREMISE_AUTONOMOUS),
            relation("answers", PREMISE_CONTESTED, GAP_CONTESTED),
            relation("contradicts", PREMISE_CONTESTED, DIRECTIVE),
        ],
    );

    project
}

fn business_premise(id: &str, work_item: &str) -> Value {
    node(
        id,
        "premise",
        json!({
            "text": RECURRING_TEXT, "status": "confirmed", "kind": "business",
            "work_item": work_item, "confirmed_by": confirmed_by(),
        }),
    )
}

/// A mesma premissa de negócio em três work items: um grupo com o mínimo padrão.
pub fn recurrences_project() -> Project {
    let project = Project::new("acme");
    let items = [
        (WORK_ITEM, "premises.cents-payroll"),
        (OTHER_WORK_ITEM, "premises.cents-invoice"),
        (THIRD_WORK_ITEM, "premises.cents-ledger"),
    ];
    append(
        &project,
        items.iter().map(|(id, _)| work_item(id)).collect(),
    );
    append(
        &project,
        items
            .iter()
            .map(|(id, premise)| business_premise(premise, id))
            .collect(),
    );

    project
}

fn divergence(id: &str, kind: &str) -> Value {
    node(
        id,
        "divergence",
        json!({
            "source_ref": "CMP-001",
            "kind": kind,
            "text": "The code differs from the spec.",
            "evidence": [{ "source": "docs", "summary": "The docs say so." }],
            "resolution": "open",
            "work_item": WORK_ITEM,
        }),
    )
}

fn spec_part() -> String {
    [
        "## Objetivo",
        "",
        "Permitir o login.",
        "",
        "## Diretivas e premissas aplicáveis",
        "",
        &format!("- {BUSINESS_PREMISE}"),
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
        "",
    ]
    .join("\n")
}

fn amendment(target: &str) -> String {
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
        "A spec omitia o registro de acesso.",
        "",
    ]
    .join("\n")
}

fn git(project: &Project, args: &[&str]) {
    let output = git_in(project.dir(), args);

    assert!(output.status.success(), "git {args:?} failed");
}

/// Um work item sem divergência nem gap: a retro é `null`.
pub fn retro_empty_project() -> Project {
    bare_project()
}

/// Três divergências (a `layout` aprovada pela `emenda-01`), uma emenda pendente, um gap respondido sem critério e
/// um gap aberto.
pub fn retro_project() -> Project {
    let project = Project::new("acme");
    for args in [
        &["init", "-b", "main"][..],
        &["config", "user.name", "Test Author"],
        &["config", "user.email", "author@example.com"],
        &["add", "-A"],
        &["commit", "-m", "add marker"],
    ] {
        git(&project, args);
    }
    let folder = fs::canonicalize(project.dir())
        .expect("the project directory exists")
        .join(".omhl/specs")
        .join(SPEC_FOLDER);
    fs::create_dir_all(&folder).expect("the spec folder is created");
    fs::write(folder.join("parte-01.md"), spec_part()).expect("the part is written");
    fs::write(
        folder.join(APPROVED_AMENDMENT),
        amendment(DIVERGENCE_LAYOUT),
    )
    .expect("the approved amendment is written");
    fs::write(folder.join(PENDING_AMENDMENT), amendment(DIVERGENCE_AUDIT))
        .expect("the pending amendment is written");

    append(&project, vec![work_item(WORK_ITEM)]);
    append(
        &project,
        vec![
            node(
                BUSINESS_PREMISE,
                "premise",
                json!({
                    "text": "Login is required.", "kind": "business", "work_item": WORK_ITEM,
                    "status": "confirmed", "confirmed_by": confirmed_by(),
                }),
            ),
            divergence(DIVERGENCE_REPORT, "partial"),
            divergence(DIVERGENCE_AUDIT, "missing"),
            divergence(DIVERGENCE_LAYOUT, "partial"),
            gap(GAP_HUMAN),
            gap(GAP_UNANSWERED),
            autonomous_premise(PREMISE_AUTONOMOUS),
        ],
    );
    append(
        &project,
        vec![relation("answers", PREMISE_AUTONOMOUS, GAP_HUMAN)],
    );
    approve_layout(&project);

    project
}

/// A revisão `spec-stale` da divergência `layout`, com a `emenda-01` e a confirmação de uma pessoa.
fn approve_layout(project: &Project) {
    let current = run_inproc(project, &["--format", "json", "get", DIVERGENCE_LAYOUT], "");
    let current: Value = serde_json::from_str(current.stdout.trim_end()).expect("the node is JSON");
    let data = json!({
        "source_ref": "CMP-001", "kind": "partial", "text": "The code differs from the spec.",
        "evidence": [{ "source": "docs", "summary": "The docs say so." }],
        "resolution": "spec-stale", "work_item": WORK_ITEM, "amendment": "emenda-01",
        "confirmed_by": confirmed_by(),
    });
    let mut revision = node(DIVERGENCE_LAYOUT, "divergence", data);
    revision["base"] = current["node"]["record_id"].clone();

    append(project, vec![revision]);
}
