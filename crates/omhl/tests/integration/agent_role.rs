use crate::support;

use std::fs;

use hexlog::{AppendItem, AppendRequest, Author, RecordId};
use omhl::error::OmhlError;
use omhl::facade::Omhl;
use serde_json::{Value, json};
use support::omhl::{
    OmhlHarness, PR_7, RUN_TESTS, append_as, append_ok, append_ok_as, attestation_item, check_data,
    decision_data, node_item, premise_data, pull_request_data, record_id_of, refusal, request,
    retirement_item, revision_item, rule_code, seed_pull_request, work_item_data,
};
use support::pr_gate::{
    ANCHOR, CRITIC_DECISION, DECISION, PREMISE, anchor_decision, confirmed_premise_data,
    relation_item, relation_retirement_item, seed_contradicted_decision, seed_critic_contradiction,
};
use support::project::Project;
use support::run::{Run, run_omhl};
use support::{FakeProbe, agent_author, author, cli_author, loader_author, role_author, snap};

const DECISION_ID: &str = "decisions.x";

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

/// Um `decision`: tipo que o critic continua podendo gravar quando a matriz entrar.
fn append_decision(project: &Project, claude_code: bool, role: Option<&str>) -> Run {
    let request = json!({
        "items": [{ "body": {
            "kind": "node", "id": DECISION_ID, "type": "decision",
            "data": decision_data("Use WAL", None),
        } }]
    });

    run_omhl(
        project,
        &["append"],
        &request.to_string(),
        claude_code,
        role,
    )
}

/// `author` do node gravado; o `get` roda sem papel e fora do Claude Code.
fn recorded_author(project: &Project) -> Value {
    let fetched = run_omhl(project, &["get", DECISION_ID], "", false, None);

    json_line(&fetched.stdout)["node"]["author"].clone()
}

#[test]
fn it_should_record_omhl_critic_as_the_author_agent_when_omhl_role_is_omhl_critic() {
    let project = Project::new("acme");

    append_decision(&project, true, Some("omhl:critic"));

    assert_eq!(recorded_author(&project)["agent"], json!("omhl:critic"));
}

#[test]
fn it_should_keep_claude_as_the_author_agent_when_omhl_role_is_not_set_under_claude_code() {
    let project = Project::new("acme");

    append_decision(&project, true, None);

    assert_eq!(recorded_author(&project)["agent"], json!("claude"));
}

#[test]
fn it_should_refuse_an_omhl_role_outside_the_seven_roles() {
    let project = Project::new("acme");

    let appended = append_decision(&project, true, Some("omhl:wizard"));

    assert_eq!(
        json_line(&appended.stderr)["error"]["code"],
        json!("role-unknown")
    );
}

#[test]
fn it_should_refuse_an_omhl_role_outside_claude_code() {
    let project = Project::new("acme");

    let appended = append_decision(&project, false, Some("omhl:critic"));

    assert_eq!(
        json_line(&appended.stderr)["error"]["code"],
        json!("role-unknown")
    );
}

#[test]
fn it_should_refuse_an_empty_omhl_role_under_claude_code() {
    let project = Project::new("acme");

    let appended = append_decision(&project, true, Some(""));

    assert_eq!(
        json_line(&appended.stderr)["error"]["code"],
        json!("role-unknown")
    );
}

/// Carrega uma diretiva e apaga o arquivo: o node segue vigente, órfão, pronto para o `guideline retire`.
fn seed_orphaned_guideline(project: &Project, slug: &str) {
    let dir = project.dir().join(".omhl/guidelines");
    fs::create_dir_all(&dir).expect("the guidelines directory is created");
    let file = dir.join(format!("{slug}.toml"));
    fs::write(&file, "kind = \"business\"\ntext = \"Be brief\"\n").expect("the file is written");
    run_omhl(project, &["load"], "", false, None);
    fs::remove_file(file).expect("the file is removed");
}

#[test]
fn it_should_refuse_guideline_retire_when_omhl_role_is_set() {
    let project = Project::new("acme");
    seed_orphaned_guideline(&project, "brief");

    let retired = run_omhl(
        &project,
        &["guideline", "retire", "guidelines.brief"],
        "",
        true,
        Some("omhl:critic"),
    );

    assert_eq!(
        json_line(&retired.stderr)["error"]["code"],
        json!("role-retirement-forbidden")
    );
}

#[test]
fn it_should_keep_the_author_client_unchanged_when_omhl_role_is_set() {
    let project = Project::new("acme");

    append_decision(&project, true, Some("omhl:critic"));

    assert_eq!(recorded_author(&project)["client"], json!("cli"));
}

// Matriz de papéis no `append`, pelo `Omhl` (o mesmo caminho de `tests/integration/guard.rs`)

const ANALYST: &str = "omhl:analyst";
const ARCHITECT: &str = "omhl:architect";
const CRITIC: &str = "omhl:critic";
const CODE_REVIEWER: &str = "omhl:code-reviewer";
const VERIFIER: &str = "omhl:verifier";
const ROLES: [&str; 7] = [
    ANALYST,
    "omhl:planner",
    ARCHITECT,
    CRITIC,
    CODE_REVIEWER,
    "omhl:security-reviewer",
    VERIFIER,
];
const RECORD_FORBIDDEN: &str = "role-record-forbidden";
const REVISION_FORBIDDEN: &str = "role-revision-forbidden";
const RETIREMENT_FORBIDDEN: &str = "role-retirement-forbidden";
const DIVERGENCE: &str = "divergences.report";
const GAP: &str = "gaps.deployer";
const PR_8: &str = "work-items.pr-8";
const GUIDELINE: &str = "guidelines.tone";
const GUIDELINE_RETIRE_CLIENT: &str = "guideline-retire";

fn with_field(mut data: Value, key: &str, value: Value) -> Value {
    data[key] = value;

    data
}

fn gap_data(kind: Option<&str>) -> Value {
    let data = json!({
        "question": "Who deploys?",
        "context": "the spec does not say",
        "work_item": PR_7,
    });

    match kind {
        Some(kind) => with_field(data, "kind", json!(kind)),
        None => data,
    }
}

fn technical_premise_data() -> Value {
    with_field(
        premise_data("The queue is durable", Some(PR_7)),
        "kind",
        json!("technical"),
    )
}

fn divergence_data(resolution: &str) -> Value {
    json!({
        "source_ref": "spec#ac-3",
        "kind": "missing",
        "text": "the spec asks for a report the code does not have",
        "evidence": [{ "source": "docs", "summary": "the docs say so" }],
        "resolution": resolution,
        "work_item": PR_7,
    })
}

/// `open` virou `code-fixed` e a evidência nova entrou no fim: a única revisão que o verifier pode fazer.
fn fixed_divergence_data() -> Value {
    let mut data = with_field(divergence_data("open"), "resolution", json!("code-fixed"));
    data["evidence"]
        .as_array_mut()
        .expect("evidence is a list")
        .push(json!({ "source": "git", "summary": "fixed in the last commit" }));

    data
}

fn spec_stale_divergence_data() -> Value {
    let data = with_field(
        divergence_data("spec-stale"),
        "confirmed_by",
        json!({ "by": "person:ana", "question": "Is the spec stale?", "answer": "Yes, it is." }),
    );

    with_field(data, "amendment", json!("emenda-01"))
}

/// Grava `item` como `role` e falha o teste se a guarda recusar: é o estado de partida, não o ato.
fn seed(omhl: &mut Omhl<'_>, role: &str, item: AppendItem) {
    append_ok_as(omhl, role_author(role), vec![item]);
}

fn append_relation(
    omhl: &mut Omhl<'_>,
    author: Author,
    relation_type: &str,
    from: &str,
    to: &str,
) -> RecordId {
    append_ok_as(omhl, author, vec![relation_item(relation_type, from, to)]).records[0]
        .id
        .clone()
}

/// Código da recusa; o teste falha se o `append` for aceito ou se o erro não for de regra.
fn refused_code(omhl: &mut Omhl<'_>, author: Author, items: Vec<AppendItem>) -> &'static str {
    rule_code(&append_as(omhl, author, items).expect_err("the append is refused"))
}

/// Um `append` por papel de `roles`, com os itens que `items_of` monta para cada um; o código de cada recusa.
fn refused_codes(
    omhl: &mut Omhl<'_>,
    roles: &[&str],
    items_of: impl Fn(&str) -> Vec<AppendItem>,
) -> Vec<&'static str> {
    roles
        .iter()
        .map(|role| refused_code(omhl, role_author(role), items_of(role)))
        .collect()
}

fn appended_count(
    omhl: &mut Omhl<'_>,
    author: Author,
    items: Vec<AppendItem>,
) -> Result<usize, OmhlError> {
    append_as(omhl, author, items).map(|outcome| outcome.records.len())
}

/// Contradição do critic contra o architect, com `PR_7` aberto; devolve o record da `contradicts`.
fn seed_contradiction(omhl: &mut Omhl<'_>) -> RecordId {
    seed_pull_request(omhl);

    seed_critic_contradiction(omhl, true)
}

fn contradiction_retirement(contradiction: &RecordId) -> AppendItem {
    relation_retirement_item("contradicts", CRITIC_DECISION, DECISION, contradiction)
}

// Tipo, relação e retirada fora da matriz

#[test]
fn it_should_refuse_a_divergence_appended_by_omhl_critic() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let divergence = node_item(DIVERGENCE, "divergence", divergence_data("open"));

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![divergence]);

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_refuse_a_decision_appended_by_omhl_verifier() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let decision = node_item(DECISION, "decision", decision_data("Use SQLite", None));

    let code = refused_code(&mut omhl, role_author(VERIFIER), vec![decision]);

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_accept_a_divergence_appended_by_omhl_verifier() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let divergence = node_item(DIVERGENCE, "divergence", divergence_data("open"));

    let result = appended_count(&mut omhl, role_author(VERIFIER), vec![divergence]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_refuse_a_gap_without_kind_appended_by_omhl_analyst() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let gap = node_item(GAP, "gap", gap_data(None));

    let code = refused_code(&mut omhl, role_author(ANALYST), vec![gap]);

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_accept_a_gap_with_kind_appended_by_omhl_analyst() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let gap = node_item(GAP, "gap", gap_data(Some("technical")));

    let result = appended_count(&mut omhl, role_author(ANALYST), vec![gap]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_refuse_a_work_item_appended_by_any_ported_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let work_item_of = |role: &str| {
        let id = format!("work-items.{}", role.trim_start_matches("omhl:"));

        vec![node_item(&id, "work-item", work_item_data("Ship it"))]
    };

    let codes = refused_codes(&mut omhl, &ROLES, work_item_of);

    assert_eq!(codes, vec![RECORD_FORBIDDEN; ROLES.len()]);
}

#[test]
fn it_should_refuse_an_attestation_appended_by_any_ported_role() {
    let harness = OmhlHarness::new(FakeProbe::clean(snap('a')));
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let attestation_of = |_role: &str| vec![attestation_item(&snap('a'), RUN_TESTS, PR_7)];

    let codes = refused_codes(&mut omhl, &ROLES, attestation_of);

    assert_eq!(codes, vec![RECORD_FORBIDDEN; ROLES.len()]);
}

#[test]
fn it_should_refuse_a_check_appended_by_any_ported_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let check_of = |_role: &str| {
        let data = check_data("before-pr", "Read the diff", true);

        vec![node_item("checks.read-diff", "check", data)]
    };

    let codes = refused_codes(&mut omhl, &ROLES, check_of);

    assert_eq!(codes, vec![RECORD_FORBIDDEN; ROLES.len()]);
}

#[test]
fn it_should_refuse_a_relation_type_outside_the_matrix_of_the_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ARCHITECT,
        node_item(
            DECISION,
            "decision",
            decision_data("Use SQLite", Some(PR_7)),
        ),
    );
    append_ok(
        &mut omhl,
        vec![node_item(
            ANCHOR,
            "premise",
            confirmed_premise_data("Users pay monthly"),
        )],
    );
    let supports = relation_item("supports", DECISION, ANCHOR);

    let code = refused_code(&mut omhl, role_author(ARCHITECT), vec![supports]);

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_refuse_a_relation_whose_origin_node_was_authored_by_another_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ARCHITECT,
        node_item(
            DECISION,
            "decision",
            decision_data("Use SQLite", Some(PR_7)),
        ),
    );
    seed(
        &mut omhl,
        CRITIC,
        node_item(
            CRITIC_DECISION,
            "decision",
            decision_data("Use Postgres", Some(PR_7)),
        ),
    );
    let reversed = relation_item("contradicts", DECISION, CRITIC_DECISION);

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![reversed]);

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_refuse_an_answers_relation_from_omhl_architect() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ARCHITECT,
        node_item(
            DECISION,
            "decision",
            decision_data("Use SQLite", Some(PR_7)),
        ),
    );
    append_ok(
        &mut omhl,
        vec![node_item(
            ANCHOR,
            "premise",
            confirmed_premise_data("Users pay monthly"),
        )],
    );
    let answers = relation_item("answers", DECISION, ANCHOR);

    let code = refused_code(&mut omhl, role_author(ARCHITECT), vec![answers]);

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_refuse_the_retirement_of_a_node_by_any_ported_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    append_ok(
        &mut omhl,
        vec![node_item(
            DECISION,
            "decision",
            decision_data("Use SQLite", None),
        )],
    );
    let base = record_id_of(&mut omhl, DECISION);

    let codes = refused_codes(&mut omhl, &ROLES, |_| {
        vec![retirement_item(DECISION, &base)]
    });

    assert_eq!(codes, vec![RETIREMENT_FORBIDDEN; ROLES.len()]);
}

#[test]
fn it_should_refuse_the_retirement_of_a_rests_on_answers_or_supports_relation_by_any_ported_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    append_ok(
        &mut omhl,
        vec![
            node_item(DECISION, "decision", decision_data("Use SQLite", None)),
            node_item(GAP, "gap", gap_data(Some("technical"))),
        ],
    );
    let rests_on = anchor_decision(&mut omhl);
    let answers = append_relation(&mut omhl, author(), "answers", ANCHOR, GAP);
    let supports = append_relation(&mut omhl, author(), "supports", ANCHOR, DECISION);
    let relations = [
        ("rests-on", DECISION, ANCHOR, rests_on),
        ("answers", ANCHOR, GAP, answers),
        ("supports", ANCHOR, DECISION, supports),
    ];

    let codes: Vec<Vec<&str>> = relations
        .iter()
        .map(|(relation_type, from, to, base)| {
            refused_codes(&mut omhl, &ROLES, |_| {
                vec![relation_retirement_item(relation_type, from, to, base)]
            })
        })
        .collect();

    assert_eq!(codes, vec![vec![RETIREMENT_FORBIDDEN; ROLES.len()]; 3]);
}

// Revisão

#[test]
fn it_should_refuse_a_revision_of_a_node_authored_by_another_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        ARCHITECT,
        node_item(DECISION, "decision", decision_data("Use SQLite", None)),
    );
    let base = record_id_of(&mut omhl, DECISION);
    let revision = revision_item(
        DECISION,
        "decision",
        decision_data("Use Postgres", None),
        &base,
    );

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![revision]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_accept_a_revision_of_a_node_authored_by_the_same_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed(
        &mut omhl,
        ARCHITECT,
        node_item(DECISION, "decision", decision_data("Use SQLite", None)),
    );
    let base = record_id_of(&mut omhl, DECISION);
    let revision = revision_item(
        DECISION,
        "decision",
        decision_data("Use Postgres", None),
        &base,
    );

    let result = appended_count(&mut omhl, role_author(ARCHITECT), vec![revision]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_refuse_a_revision_that_changes_the_kind_of_a_premise_or_a_gap() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ANALYST,
        node_item(PREMISE, "premise", technical_premise_data()),
    );
    seed(
        &mut omhl,
        ANALYST,
        node_item(GAP, "gap", gap_data(Some("technical"))),
    );
    let premise_base = record_id_of(&mut omhl, PREMISE);
    let gap_base = record_id_of(&mut omhl, GAP);
    let revisions = [
        revision_item(
            PREMISE,
            "premise",
            with_field(technical_premise_data(), "kind", json!("business")),
            &premise_base,
        ),
        revision_item(GAP, "gap", gap_data(Some("business")), &gap_base),
    ];

    let codes: Vec<&str> = revisions
        .into_iter()
        .map(|revision| refused_code(&mut omhl, role_author(ANALYST), vec![revision]))
        .collect();

    assert_eq!(codes, vec![REVISION_FORBIDDEN; 2]);
}

#[test]
fn it_should_refuse_a_ported_role_revising_a_divergence_to_accepted() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let accepted = revision_item(DIVERGENCE, "divergence", divergence_data("accepted"), &base);

    let code = refused_code(&mut omhl, role_author(CODE_REVIEWER), vec![accepted]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_accept_omhl_verifier_revising_a_divergence_of_another_reviewer_to_code_fixed() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let fixed = revision_item(DIVERGENCE, "divergence", fixed_divergence_data(), &base);

    let result = appended_count(&mut omhl, role_author(VERIFIER), vec![fixed]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_refuse_omhl_verifier_revising_a_divergence_to_accepted() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let accepted = revision_item(DIVERGENCE, "divergence", divergence_data("accepted"), &base);

    let code = refused_code(&mut omhl, role_author(VERIFIER), vec![accepted]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_omhl_verifier_changing_a_divergence_field_other_than_resolution_and_appended_evidence()
 {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let rewritten = with_field(fixed_divergence_data(), "text", json!("a different claim"));
    let revision = revision_item(DIVERGENCE, "divergence", rewritten, &base);

    let code = refused_code(&mut omhl, role_author(VERIFIER), vec![revision]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_a_ported_role_closing_its_own_divergence_as_spec_stale() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let closed = revision_item(
        DIVERGENCE,
        "divergence",
        spec_stale_divergence_data(),
        &base,
    );

    let code = refused_code(&mut omhl, role_author(CODE_REVIEWER), vec![closed]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_a_ported_role_closing_its_own_divergence_as_code_fixed() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let fixed = revision_item(DIVERGENCE, "divergence", fixed_divergence_data(), &base);

    let code = refused_code(&mut omhl, role_author(CODE_REVIEWER), vec![fixed]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_omhl_verifier_closing_its_own_divergence_outside_the_close_rule() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        VERIFIER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let closed_without_new_evidence =
        with_field(divergence_data("open"), "resolution", json!("code-fixed"));
    let revision = revision_item(DIVERGENCE, "divergence", closed_without_new_evidence, &base);

    let code = refused_code(&mut omhl, role_author(VERIFIER), vec![revision]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_omhl_verifier_revising_a_spec_stale_divergence() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    append_ok(
        &mut omhl,
        vec![node_item(
            DIVERGENCE,
            "divergence",
            spec_stale_divergence_data(),
        )],
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let fixed = revision_item(DIVERGENCE, "divergence", fixed_divergence_data(), &base);

    let code = refused_code(&mut omhl, role_author(VERIFIER), vec![fixed]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_a_revision_that_moves_a_contradicted_decision_out_of_the_work_item() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    append_ok(
        &mut omhl,
        vec![node_item(PR_8, "pull-request", pull_request_data("Other"))],
    );
    seed_contradicted_decision(&mut omhl);
    let base = record_id_of(&mut omhl, DECISION);
    let moved = revision_item(
        DECISION,
        "decision",
        decision_data("Use SQLite", Some(PR_8)),
        &base,
    );

    let error = refusal(&mut omhl, vec![moved]);

    assert_eq!(rule_code(&error), REVISION_FORBIDDEN);
}

// Retirada de `contradicts`

#[test]
fn it_should_accept_the_retirement_of_a_critic_contradicts_by_omhl_verifier() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let contradiction = seed_contradiction(&mut omhl);

    let result = appended_count(
        &mut omhl,
        role_author(VERIFIER),
        vec![contradiction_retirement(&contradiction)],
    );

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_the_retirement_of_a_critic_contradicts_by_user() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let contradiction = seed_contradiction(&mut omhl);

    let result = appended_count(
        &mut omhl,
        cli_author(),
        vec![contradiction_retirement(&contradiction)],
    );

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_refuse_the_retirement_of_a_contradicts_by_claude() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let contradiction = seed_contradiction(&mut omhl);

    let code = refused_code(
        &mut omhl,
        agent_author(),
        vec![contradiction_retirement(&contradiction)],
    );

    assert_eq!(code, RETIREMENT_FORBIDDEN);
}

#[test]
fn it_should_refuse_the_retirement_of_a_contradicts_by_any_ported_role_other_than_omhl_verifier() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let contradiction = seed_contradiction(&mut omhl);
    let others: Vec<&str> = ROLES.into_iter().filter(|role| *role != VERIFIER).collect();

    let codes = refused_codes(&mut omhl, &others, |_| {
        vec![contradiction_retirement(&contradiction)]
    });

    assert_eq!(codes, vec![RETIREMENT_FORBIDDEN; others.len()]);
}

#[test]
fn it_should_refuse_the_retirement_of_a_contradicts_by_the_role_that_appended_it() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    append_ok(
        &mut omhl,
        vec![
            node_item(DECISION, "decision", decision_data("Use SQLite", None)),
            node_item(
                CRITIC_DECISION,
                "decision",
                decision_data("Use Postgres", None),
            ),
        ],
    );
    // O verifier não grava `contradicts`; a relação é forjada direto no `hexlog`, sem a guarda.
    let forged = harness.forge(&AppendRequest {
        author: role_author(VERIFIER),
        ..request(vec![relation_item(
            "contradicts",
            CRITIC_DECISION,
            DECISION,
        )])
    });

    let code = refused_code(
        &mut omhl,
        role_author(VERIFIER),
        vec![contradiction_retirement(&forged.records[0].id)],
    );

    assert_eq!(code, RETIREMENT_FORBIDDEN);
}

#[test]
fn it_should_refuse_user_retiring_a_contradicts_that_user_appended() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    append_ok(
        &mut omhl,
        vec![
            node_item(DECISION, "decision", decision_data("Use SQLite", None)),
            node_item(
                CRITIC_DECISION,
                "decision",
                decision_data("Use Postgres", None),
            ),
        ],
    );
    let contradiction = append_relation(
        &mut omhl,
        cli_author(),
        "contradicts",
        CRITIC_DECISION,
        DECISION,
    );

    let code = refused_code(
        &mut omhl,
        cli_author(),
        vec![contradiction_retirement(&contradiction)],
    );

    assert_eq!(code, RETIREMENT_FORBIDDEN);
}

#[test]
fn it_should_refuse_claude_retiring_the_origin_decision_of_a_current_contradicts() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_contradiction(&mut omhl);
    let origin = record_id_of(&mut omhl, CRITIC_DECISION);

    let code = refused_code(
        &mut omhl,
        agent_author(),
        vec![retirement_item(CRITIC_DECISION, &origin)],
    );

    assert_eq!(code, RETIREMENT_FORBIDDEN);
}

#[test]
fn it_should_refuse_claude_retiring_the_target_decision_of_a_current_contradicts() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_contradiction(&mut omhl);
    let target = record_id_of(&mut omhl, DECISION);

    let code = refused_code(
        &mut omhl,
        agent_author(),
        vec![retirement_item(DECISION, &target)],
    );

    assert_eq!(code, RETIREMENT_FORBIDDEN);
}

#[test]
fn it_should_accept_user_retiring_the_origin_decision_of_a_current_contradicts() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_contradiction(&mut omhl);
    let origin = record_id_of(&mut omhl, CRITIC_DECISION);

    let result = appended_count(
        &mut omhl,
        cli_author(),
        vec![retirement_item(CRITIC_DECISION, &origin)],
    );

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_keep_guideline_retire_working_from_claude_on_a_guideline_contradicted_by_a_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    append_ok_as(
        &mut omhl,
        loader_author(),
        vec![node_item(
            GUIDELINE,
            "guideline",
            json!({ "text": "Keep pull requests small", "kind": "business" }),
        )],
    );
    append_ok(
        &mut omhl,
        vec![node_item(
            PREMISE,
            "premise",
            premise_data("Big pull requests are fine", None),
        )],
    );
    append_relation(&mut omhl, author(), "contradicts", PREMISE, GUIDELINE);
    let base = record_id_of(&mut omhl, GUIDELINE);
    let retirer = Author {
        client: GUIDELINE_RETIRE_CLIENT.to_owned(),
        ..agent_author()
    };

    let result = appended_count(&mut omhl, retirer, vec![retirement_item(GUIDELINE, &base)]);

    assert_eq!(result, Ok(1));
}

// Cadeira do critic: `confirmed_by.by = "critic"` e premissa técnica

const CONFIRMATION_INVALID: &str = "confirmation-invalid";
const ANSWER_FORBIDDEN: &str = "role-answer-forbidden";

fn confirmation(by: &str) -> Value {
    json!({ "by": by, "question": "Is the queue durable?", "answer": "Yes, it is." })
}

fn confirmed_technical_premise_data(by: &str) -> Value {
    let data = with_field(technical_premise_data(), "status", json!("confirmed"));

    with_field(data, "confirmed_by", confirmation(by))
}

fn confirmed_business_premise_data(by: &str) -> Value {
    with_field(
        confirmed_technical_premise_data(by),
        "kind",
        json!("business"),
    )
}

/// O critic revisa `PREMISE` para confirmada por ele; devolve o veredito da guarda.
fn confirm_as_critic(omhl: &mut Omhl<'_>) -> Result<usize, OmhlError> {
    let base = record_id_of(omhl, PREMISE);
    let confirmed = revision_item(
        PREMISE,
        "premise",
        confirmed_technical_premise_data("critic"),
        &base,
    );

    appended_count(omhl, role_author(CRITIC), vec![confirmed])
}

/// `PR_7` aberto e `PREMISE`, premissa técnica confirmada pelo critic, pronta para responder um gap.
fn seed_critic_premise(omhl: &mut Omhl<'_>) {
    seed_pull_request(omhl);
    seed(
        omhl,
        CRITIC,
        node_item(
            PREMISE,
            "premise",
            confirmed_technical_premise_data("critic"),
        ),
    );
}

#[test]
fn it_should_accept_a_technical_premise_confirmed_by_critic_when_the_author_is_omhl_critic() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let premise = node_item(
        PREMISE,
        "premise",
        confirmed_technical_premise_data("critic"),
    );

    let result = appended_count(&mut omhl, role_author(CRITIC), vec![premise]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_refuse_confirmed_by_critic_when_the_author_is_not_omhl_critic() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let premise = || {
        vec![node_item(
            PREMISE,
            "premise",
            confirmed_technical_premise_data("critic"),
        )]
    };

    let codes: Vec<&str> = [agent_author(), cli_author(), author()]
        .into_iter()
        .map(|refused| refused_code(&mut omhl, refused, premise()))
        .collect();

    assert_eq!(codes, vec![CONFIRMATION_INVALID; 3]);
}

#[test]
fn it_should_refuse_confirmed_by_critic_on_a_business_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let premise = node_item(
        PREMISE,
        "premise",
        confirmed_business_premise_data("critic"),
    );

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![premise]);

    assert_eq!(code, CONFIRMATION_INVALID);
}

#[test]
fn it_should_refuse_confirmed_by_critic_without_literal_question_and_answer() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let data = with_field(
        confirmed_technical_premise_data("critic"),
        "confirmed_by",
        json!({ "by": "critic" }),
    );

    let code = refused_code(
        &mut omhl,
        role_author(CRITIC),
        vec![node_item(PREMISE, "premise", data)],
    );

    assert_eq!(code, CONFIRMATION_INVALID);
}

#[test]
fn it_should_keep_refusing_a_confirmed_premise_whose_by_is_neither_person_nor_critic() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let premise = node_item(
        PREMISE,
        "premise",
        confirmed_technical_premise_data("robot"),
    );

    let code = refused_code(&mut omhl, agent_author(), vec![premise]);

    assert_eq!(code, CONFIRMATION_INVALID);
}

#[test]
fn it_should_refuse_confirmed_by_critic_on_a_guideline() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    let data = json!({
        "text": "Keep pull requests small",
        "kind": "business",
        "confirmed_by": confirmation("critic"),
    });

    let code = refused_code(
        &mut omhl,
        role_author(CRITIC),
        vec![node_item(GUIDELINE, "guideline", data)],
    );

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_refuse_confirmed_by_critic_on_a_spec_stale_divergence() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let data = with_field(
        spec_stale_divergence_data(),
        "confirmed_by",
        confirmation("critic"),
    );

    let code = refused_code(
        &mut omhl,
        role_author(CRITIC),
        vec![node_item(DIVERGENCE, "divergence", data)],
    );

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_refuse_a_premise_appended_by_omhl_critic_that_is_only_assumed() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let premise = node_item(PREMISE, "premise", technical_premise_data());

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![premise]);

    assert_eq!(code, RECORD_FORBIDDEN);
}

#[test]
fn it_should_refuse_a_premise_appended_by_omhl_critic_that_is_confirmed_by_a_person() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let premise = node_item(
        PREMISE,
        "premise",
        confirmed_technical_premise_data("person:ana"),
    );

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![premise]);

    assert_eq!(code, RECORD_FORBIDDEN);
}

// Revisão de premissa técnica pelo critic

#[test]
fn it_should_accept_omhl_critic_confirming_a_technical_premise_authored_by_another_role() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ANALYST,
        node_item(PREMISE, "premise", technical_premise_data()),
    );

    let result = confirm_as_critic(&mut omhl);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_omhl_critic_confirming_a_technical_premise_authored_by_claude() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    append_ok_as(
        &mut omhl,
        agent_author(),
        vec![node_item(PREMISE, "premise", technical_premise_data())],
    );

    let result = confirm_as_critic(&mut omhl);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_refuse_omhl_critic_confirming_a_business_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let business = with_field(technical_premise_data(), "kind", json!("business"));
    seed(&mut omhl, ANALYST, node_item(PREMISE, "premise", business));
    let base = record_id_of(&mut omhl, PREMISE);
    let confirmed = revision_item(
        PREMISE,
        "premise",
        confirmed_business_premise_data("critic"),
        &base,
    );

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![confirmed]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_take_the_author_of_the_current_revision_as_the_author_for_the_own_role_checks() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ANALYST,
        node_item(PREMISE, "premise", technical_premise_data()),
    );
    seed(
        &mut omhl,
        ANALYST,
        node_item(GAP, "gap", gap_data(Some("technical"))),
    );
    confirm_as_critic(&mut omhl).expect("the critic confirms the premise of the analyst");
    let answers = relation_item("answers", PREMISE, GAP);

    let result = appended_count(&mut omhl, role_author(CRITIC), vec![answers]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_refuse_omhl_verifier_removing_or_changing_an_existing_evidence_item() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let new_evidence = json!({ "source": "git", "summary": "fixed in the last commit" });
    let removed = with_field(fixed_divergence_data(), "evidence", json!([new_evidence]));
    let changed = with_field(
        fixed_divergence_data(),
        "evidence",
        json!([{ "source": "docs", "summary": "the docs say otherwise" }, new_evidence]),
    );

    let codes: Vec<&str> = [removed, changed]
        .into_iter()
        .map(|data| {
            let revision = revision_item(DIVERGENCE, "divergence", data, &base);

            refused_code(&mut omhl, role_author(VERIFIER), vec![revision])
        })
        .collect();

    assert_eq!(codes, vec![REVISION_FORBIDDEN; 2]);
}

// `answers` do critic

#[test]
fn it_should_refuse_an_answers_relation_from_omhl_critic_to_a_business_gap() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_critic_premise(&mut omhl);
    append_ok(
        &mut omhl,
        vec![node_item(GAP, "gap", gap_data(Some("business")))],
    );
    let answers = relation_item("answers", PREMISE, GAP);

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![answers]);

    assert_eq!(code, ANSWER_FORBIDDEN);
}

#[test]
fn it_should_refuse_an_answers_relation_from_omhl_critic_to_a_gap_without_kind() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_critic_premise(&mut omhl);
    append_ok(&mut omhl, vec![node_item(GAP, "gap", gap_data(None))]);
    let answers = relation_item("answers", PREMISE, GAP);

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![answers]);

    assert_eq!(code, ANSWER_FORBIDDEN);
}

#[test]
fn it_should_refuse_an_answers_relation_from_omhl_critic_to_a_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_critic_premise(&mut omhl);
    append_ok(
        &mut omhl,
        vec![node_item(
            ANCHOR,
            "premise",
            confirmed_premise_data("Users pay monthly"),
        )],
    );
    let answers = relation_item("answers", PREMISE, ANCHOR);

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![answers]);

    assert_eq!(code, ANSWER_FORBIDDEN);
}

#[test]
fn it_should_accept_an_answers_relation_from_omhl_critic_to_a_technical_gap() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_critic_premise(&mut omhl);
    append_ok(
        &mut omhl,
        vec![node_item(GAP, "gap", gap_data(Some("technical")))],
    );
    let answers = relation_item("answers", PREMISE, GAP);

    let result = appended_count(&mut omhl, role_author(CRITIC), vec![answers]);

    assert_eq!(result, Ok(1));
}

// Divergência criada por papel portado: só `open`

#[test]
fn it_should_refuse_a_ported_role_creating_a_divergence_that_is_not_open() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let closed = [
        divergence_data("accepted"),
        fixed_divergence_data(),
        spec_stale_divergence_data(),
    ];
    let reviewers = [CODE_REVIEWER, "omhl:security-reviewer", VERIFIER];

    let codes: Vec<&str> = reviewers
        .iter()
        .flat_map(|role| closed.iter().map(move |data| (role, data)))
        .map(|(role, data)| {
            let divergence = node_item(DIVERGENCE, "divergence", data.clone());

            refused_code(&mut omhl, role_author(role), vec![divergence])
        })
        .collect();

    assert_eq!(codes, vec![RECORD_FORBIDDEN; 9]);
}

// Dono depois de confirmar ou fechar: o critic e o verifier não reescrevem

#[test]
fn it_should_refuse_omhl_critic_rewriting_a_premise_it_only_confirmed() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ANALYST,
        node_item(PREMISE, "premise", technical_premise_data()),
    );
    confirm_as_critic(&mut omhl).expect("the critic confirms the premise of the analyst");
    let base = record_id_of(&mut omhl, PREMISE);
    let rewritten = with_field(
        confirmed_technical_premise_data("critic"),
        "text",
        json!("The queue is lossy"),
    );
    let revision = revision_item(PREMISE, "premise", rewritten, &base);

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![revision]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_omhl_critic_revising_a_confirmed_premise_it_created() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_critic_premise(&mut omhl);
    let base = record_id_of(&mut omhl, PREMISE);
    let rewritten = with_field(
        confirmed_technical_premise_data("critic"),
        "text",
        json!("The queue is lossy"),
    );
    let revision = revision_item(PREMISE, "premise", rewritten, &base);

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![revision]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_omhl_verifier_rewriting_a_divergence_it_closed_as_code_fixed() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let open_base = record_id_of(&mut omhl, DIVERGENCE);
    seed(
        &mut omhl,
        VERIFIER,
        revision_item(
            DIVERGENCE,
            "divergence",
            fixed_divergence_data(),
            &open_base,
        ),
    );
    let closed_base = record_id_of(&mut omhl, DIVERGENCE);
    let rewritten = with_field(fixed_divergence_data(), "text", json!("a different claim"));
    let revision = revision_item(DIVERGENCE, "divergence", rewritten, &closed_base);

    let code = refused_code(&mut omhl, role_author(VERIFIER), vec![revision]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_accept_omhl_analyst_revising_its_own_assumed_premise() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ANALYST,
        node_item(PREMISE, "premise", technical_premise_data()),
    );
    let base = record_id_of(&mut omhl, PREMISE);
    let reworded = with_field(
        technical_premise_data(),
        "text",
        json!("The queue is durable on disk"),
    );
    let revision = revision_item(PREMISE, "premise", reworded, &base);

    let result = appended_count(&mut omhl, role_author(ANALYST), vec![revision]);

    assert_eq!(result, Ok(1));
}

#[test]
fn it_should_accept_a_reviewer_adding_evidence_to_its_own_open_divergence() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let mut more_evidence = divergence_data("open");
    more_evidence["evidence"]
        .as_array_mut()
        .expect("evidence is a list")
        .push(json!({ "source": "git", "summary": "seen in the last commit" }));
    let revision = revision_item(DIVERGENCE, "divergence", more_evidence, &base);

    let result = appended_count(&mut omhl, role_author(CODE_REVIEWER), vec![revision]);

    assert_eq!(result, Ok(1));
}

// O verifier fecha com evidência nova

#[test]
fn it_should_refuse_omhl_verifier_closing_a_divergence_without_new_evidence() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        CODE_REVIEWER,
        node_item(DIVERGENCE, "divergence", divergence_data("open")),
    );
    let base = record_id_of(&mut omhl, DIVERGENCE);
    let closed = with_field(divergence_data("open"), "resolution", json!("code-fixed"));
    let revision = revision_item(DIVERGENCE, "divergence", closed, &base);

    let code = refused_code(&mut omhl, role_author(VERIFIER), vec![revision]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

// Regras sem teste até aqui

#[test]
fn it_should_refuse_omhl_critic_confirming_a_premise_of_another_author_while_changing_its_text() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ANALYST,
        node_item(PREMISE, "premise", technical_premise_data()),
    );
    let base = record_id_of(&mut omhl, PREMISE);
    let rewritten = with_field(
        confirmed_technical_premise_data("critic"),
        "text",
        json!("The queue is lossy"),
    );
    let revision = revision_item(PREMISE, "premise", rewritten, &base);

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![revision]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_refuse_omhl_critic_confirming_a_premise_already_confirmed_by_a_person() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    append_ok_as(
        &mut omhl,
        agent_author(),
        vec![node_item(
            PREMISE,
            "premise",
            confirmed_technical_premise_data("person:ana"),
        )],
    );
    let base = record_id_of(&mut omhl, PREMISE);
    let reconfirmed = revision_item(
        PREMISE,
        "premise",
        confirmed_technical_premise_data("critic"),
        &base,
    );

    let code = refused_code(&mut omhl, role_author(CRITIC), vec![reconfirmed]);

    assert_eq!(code, REVISION_FORBIDDEN);
}

#[test]
fn it_should_accept_a_relation_from_a_node_created_earlier_in_the_same_batch() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    seed(
        &mut omhl,
        ANALYST,
        node_item(PREMISE, "premise", technical_premise_data()),
    );
    append_ok(
        &mut omhl,
        vec![node_item(GAP, "gap", gap_data(Some("technical")))],
    );
    let base = record_id_of(&mut omhl, PREMISE);
    let confirmed = revision_item(
        PREMISE,
        "premise",
        confirmed_technical_premise_data("critic"),
        &base,
    );
    let answers = relation_item("answers", PREMISE, GAP);

    let result = appended_count(&mut omhl, role_author(CRITIC), vec![confirmed, answers]);

    assert_eq!(result, Ok(2));
}

#[test]
fn it_should_refuse_omhl_verifier_closing_a_divergence_whose_resolution_is_not_open() {
    let harness = OmhlHarness::new(FakeProbe::failing());
    let mut omhl = harness.open();
    seed_pull_request(&mut omhl);
    let more_evidence = {
        let mut data = fixed_divergence_data();
        data["evidence"]
            .as_array_mut()
            .expect("evidence is a list")
            .push(json!({ "source": "ci", "summary": "green after the fix" }));
        data
    };
    let cases = [
        ("divergences.accepted", divergence_data("accepted")),
        ("divergences.fixed", fixed_divergence_data()),
    ];
    let bases: Vec<(&str, RecordId)> = cases
        .into_iter()
        .map(|(id, data)| {
            append_ok_as(
                &mut omhl,
                agent_author(),
                vec![node_item(id, "divergence", data)],
            );

            (id, record_id_of(&mut omhl, id))
        })
        .collect();

    let codes: Vec<&str> = bases
        .iter()
        .map(|(id, base)| {
            let revision = revision_item(id, "divergence", more_evidence.clone(), base);

            refused_code(&mut omhl, role_author(VERIFIER), vec![revision])
        })
        .collect();

    assert_eq!(codes, vec![REVISION_FORBIDDEN; 2]);
}
