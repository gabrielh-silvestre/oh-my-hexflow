use crate::gate::QuestionClass;
use crate::workflow::gap::{Answerer, GapState};
use crate::workflow::retro::{
    AnsweredGap, ApprovedAmendment, DeliveryFailure, DivergenceFact, DivergenceGroup,
    FailedQuestion, GapFact, OpenGap, Retro, RetroFacts, retro,
};

const DIVERGENCE: &str = "divergences.login-cmp-001";
const OTHER_DIVERGENCE: &str = "divergences.login-cmp-002";
const GAP: &str = "gaps.g1";
const OTHER_GAP: &str = "gaps.g2";
const ANSWER: &str = "premises.p1";

// Camada pura: `crate::workflow::retro::retro`

fn divergence_fact(id: &str, kind: &str, resolution: &str) -> DivergenceFact {
    DivergenceFact {
        id: id.to_owned(),
        kind: kind.to_owned(),
        resolution: resolution.to_owned(),
        amendment: None,
    }
}

fn stale_fact(id: &str, amendment: &str) -> DivergenceFact {
    DivergenceFact {
        amendment: Some(amendment.to_owned()),
        ..divergence_fact(id, "partial", "spec-stale")
    }
}

fn gap_fact(id: &str, state: GapState, answered_by: Option<&str>) -> GapFact {
    GapFact {
        id: id.to_owned(),
        state,
        answered_by: answered_by.map(str::to_owned),
    }
}

fn answered(criterion: bool) -> GapState {
    GapState::Answered {
        by: Answerer::Autonomous,
        criterion,
    }
}

fn names(files: &[&str]) -> Vec<String> {
    files.iter().map(|file| (*file).to_owned()).collect()
}

fn group(kind: &str, resolution: &str, ids: &[&str]) -> DivergenceGroup {
    DivergenceGroup {
        kind: kind.to_owned(),
        resolution: resolution.to_owned(),
        ids: names(ids),
    }
}

fn pure_retro(
    divergences: Vec<DivergenceFact>,
    gaps: Vec<GapFact>,
    amendment_files: &[&str],
) -> Option<Retro> {
    retro(&RetroFacts {
        divergences,
        gaps,
        amendment_files: names(amendment_files),
        gate_override: None,
        delivery_failure: None,
    })
}

fn some_retro(
    divergences: Vec<DivergenceFact>,
    gaps: Vec<GapFact>,
    amendment_files: &[&str],
) -> Retro {
    pure_retro(divergences, gaps, amendment_files).expect("the facts call for a retro")
}

#[test]
fn it_should_produce_no_retro_when_there_is_nothing_to_report() {
    let result = pure_retro(vec![], vec![], &[]);

    assert_eq!(result, None);
}

#[test]
fn it_should_produce_no_retro_when_the_only_finding_is_an_open_gap() {
    let gaps = vec![gap_fact(GAP, GapState::Open, None)];

    let result = pure_retro(vec![], gaps, &[]);

    assert_eq!(result, None);
}

#[test]
fn it_should_produce_no_retro_when_the_only_finding_is_a_gap_that_needs_a_human() {
    let gaps = vec![gap_fact(GAP, GapState::NeedsHuman, Some(ANSWER))];

    let result = pure_retro(vec![], gaps, &[]);

    assert_eq!(result, None);
}

#[test]
fn it_should_produce_no_retro_when_every_answer_has_a_criterion() {
    let gaps = vec![gap_fact(GAP, answered(true), Some(ANSWER))];

    let result = pure_retro(vec![], gaps, &[]);

    assert_eq!(result, None);
}

#[test]
fn it_should_produce_no_retro_when_the_only_finding_is_a_pending_amendment_file() {
    let result = pure_retro(vec![], vec![], &["emenda-01.md"]);

    assert_eq!(result, None);
}

#[test]
fn it_should_produce_a_retro_when_there_is_one_divergence() {
    let divergences = vec![divergence_fact(DIVERGENCE, "partial", "open")];

    let result = pure_retro(divergences, vec![], &[]);

    assert!(result.is_some());
}

#[test]
fn it_should_produce_a_retro_when_the_only_finding_is_an_answer_without_criterion() {
    let gaps = vec![gap_fact(GAP, answered(false), Some(ANSWER))];

    let result = pure_retro(vec![], gaps, &[]);

    assert!(result.is_some());
}

#[test]
fn it_should_group_the_divergences_by_kind_and_resolution() {
    let divergences = vec![
        divergence_fact("divergences.a", "partial", "open"),
        divergence_fact("divergences.b", "missing", "accepted"),
        divergence_fact("divergences.c", "partial", "open"),
        divergence_fact("divergences.d", "partial", "code-fixed"),
    ];

    let result = some_retro(divergences, vec![], &[]);

    assert_eq!(
        result.divergences,
        vec![
            group("missing", "accepted", &["divergences.b"]),
            group("partial", "code-fixed", &["divergences.d"]),
            group("partial", "open", &["divergences.a", "divergences.c"]),
        ]
    );
}

#[test]
fn it_should_list_the_amendment_cited_by_a_spec_stale_divergence_as_approved() {
    let divergences = vec![stale_fact(DIVERGENCE, "emenda-01")];

    let result = some_retro(divergences, vec![], &["emenda-01.md"]);

    assert_eq!(
        result.approved,
        vec![ApprovedAmendment {
            name: "emenda-01.md".to_owned(),
            divergence: DIVERGENCE.to_owned(),
        }]
    );
}

#[test]
fn it_should_not_list_an_approved_amendment_as_pending() {
    let divergences = vec![stale_fact(DIVERGENCE, "emenda-01")];

    let result = some_retro(divergences, vec![], &["emenda-01.md"]);

    assert_eq!(result.pending, Vec::<String>::new());
}

#[test]
fn it_should_list_an_amendment_file_that_no_spec_stale_divergence_cites_as_pending() {
    let divergences = vec![
        stale_fact(DIVERGENCE, "emenda-01"),
        divergence_fact(OTHER_DIVERGENCE, "partial", "open"),
    ];

    let result = some_retro(divergences, vec![], &["emenda-01.md", "emenda-02.md"]);

    assert_eq!(result.pending, names(&["emenda-02.md"]));
}

#[test]
fn it_should_keep_the_pending_amendments_in_the_order_of_the_files() {
    let divergences = vec![divergence_fact(DIVERGENCE, "partial", "open")];

    let result = some_retro(divergences, vec![], &["emenda-01.md", "emenda-02.md"]);

    assert_eq!(result.pending, names(&["emenda-01.md", "emenda-02.md"]));
}

#[test]
fn it_should_list_no_amendment_when_the_work_item_has_no_amendment_file() {
    let divergences = vec![divergence_fact(DIVERGENCE, "partial", "open")];

    let result = some_retro(divergences, vec![], &[]);

    assert_eq!(
        (result.approved, result.pending),
        (vec![], Vec::<String>::new())
    );
}

#[test]
fn it_should_list_the_answers_without_criterion_with_the_answering_premise() {
    let gaps = vec![
        gap_fact(GAP, answered(false), Some(ANSWER)),
        gap_fact(OTHER_GAP, answered(true), Some("premises.p2")),
    ];

    let result = some_retro(vec![], gaps, &[]);

    assert_eq!(
        result.no_criterion,
        vec![AnsweredGap {
            gap: GAP.to_owned(),
            answered_by: ANSWER.to_owned(),
        }]
    );
}

#[test]
fn it_should_list_an_open_gap_and_a_gap_that_needs_a_human_as_open_gaps() {
    let gaps = vec![
        gap_fact(GAP, GapState::Open, None),
        gap_fact(OTHER_GAP, GapState::NeedsHuman, Some(ANSWER)),
        gap_fact("gaps.g3", answered(true), Some("premises.p2")),
    ];

    let result = some_retro(
        vec![divergence_fact(DIVERGENCE, "partial", "open")],
        gaps,
        &[],
    );

    assert_eq!(
        result.open_gaps,
        vec![
            OpenGap {
                gap: GAP.to_owned(),
                state: GapState::Open,
            },
            OpenGap {
                gap: OTHER_GAP.to_owned(),
                state: GapState::NeedsHuman,
            },
        ]
    );
}

#[test]
fn it_should_not_list_a_gap_that_needs_a_human_as_an_answer_without_criterion() {
    let gaps = vec![gap_fact(GAP, GapState::NeedsHuman, Some(ANSWER))];

    let result = some_retro(
        vec![divergence_fact(DIVERGENCE, "partial", "open")],
        gaps,
        &[],
    );

    assert_eq!(result.no_criterion, vec![]);
}

fn failed(label: &str, class: QuestionClass) -> FailedQuestion {
    FailedQuestion {
        label: label.to_owned(),
        class,
    }
}

fn delivery_failure() -> DeliveryFailure {
    DeliveryFailure {
        note: "authorized by the owner".to_owned(),
        failed: vec![
            failed("checks.run-tests", QuestionClass::Warn),
            failed("checks.ci-green-before-merge", QuestionClass::Refuse),
        ],
    }
}

fn retro_with_delivery_failure(delivery_failure: DeliveryFailure) -> Option<Retro> {
    retro(&RetroFacts {
        divergences: vec![],
        gaps: vec![],
        amendment_files: vec![],
        gate_override: None,
        delivery_failure: Some(delivery_failure),
    })
}

#[test]
fn it_should_produce_a_retro_for_a_delivery_failure_without_divergence_and_without_gap() {
    let result = retro_with_delivery_failure(delivery_failure());

    assert!(result.is_some());
}

#[test]
fn it_should_list_one_item_per_failed_question_with_its_class_and_the_note() {
    let result = retro_with_delivery_failure(delivery_failure()).expect("the retro exists");

    assert_eq!(result.delivery_failure, Some(delivery_failure()));
}
