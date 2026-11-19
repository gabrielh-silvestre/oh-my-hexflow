use crate::workflow::config::{RequireAtom, WorkflowConfig};
use crate::workflow::matrix::{PremiseFact, PremiseKind, missing_atoms};

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

/// Só os átomos de premissa: a spec entra como válida.
fn missing(prefix: &str, premises: &[PremiseFact]) -> Option<Vec<RequireAtom>> {
    missing_with_spec(prefix, premises, true)
}

fn missing_with_spec(
    prefix: &str,
    premises: &[PremiseFact],
    spec_valid: bool,
) -> Option<Vec<RequireAtom>> {
    missing_atoms(prefix, &WorkflowConfig::default(), premises, spec_valid)
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
fn it_should_not_report_the_spec_atom_as_missing_for_feat_and_fix_when_the_spec_is_valid() {
    let missing_for_feat = missing_with_spec("feat", &[], true);

    assert_eq!(missing_for_feat, Some(vec![RequireAtom::BusinessPremise]));
}

#[test]
fn it_should_report_the_spec_atom_as_missing_for_feat_and_fix_when_the_spec_is_not_valid() {
    let premises = [confirmed_business()];

    let missing_for_each: Vec<Option<Vec<RequireAtom>>> = BUSINESS_PREFIXES
        .iter()
        .map(|prefix| missing_with_spec(prefix, &premises, false))
        .collect();

    assert_eq!(missing_for_each, vec![Some(vec![RequireAtom::Spec]); 2]);
}

#[test]
fn it_should_not_ask_for_a_spec_from_a_prefix_whose_require_has_no_spec() {
    let premises = [confirmed_business(), confirmed_technical()];
    let prefixes = ["refactor", "tests", "bugfix", "chore", "docs"];

    let missing_for_each: Vec<Option<Vec<RequireAtom>>> = prefixes
        .iter()
        .map(|prefix| missing_with_spec(prefix, &premises, false))
        .collect();

    assert_eq!(missing_for_each, vec![Some(vec![]); prefixes.len()]);
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

fn config_requiring_only_spec() -> WorkflowConfig {
    let mut config = WorkflowConfig::default();
    config
        .prefixes
        .insert("feat".to_owned(), vec![RequireAtom::Spec]);

    config
}

#[test]
fn it_should_report_the_spec_atom_when_it_is_the_only_one_required_and_the_spec_is_not_valid() {
    let missing_for_feat = missing_atoms("feat", &config_requiring_only_spec(), &[], false);

    assert_eq!(missing_for_feat, Some(vec![RequireAtom::Spec]));
}

#[test]
fn it_should_report_nothing_when_the_spec_is_the_only_atom_required_and_the_spec_is_valid() {
    let missing_for_feat = missing_atoms("feat", &config_requiring_only_spec(), &[], true);

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

    let missing_for_perf = missing_atoms("perf", &config, &[], false);

    assert_eq!(
        missing_for_perf,
        Some(vec![
            RequireAtom::AgentPremise,
            RequireAtom::Spec,
            RequireAtom::BusinessPremise
        ])
    );
}

#[test]
fn it_should_skip_a_valid_spec_in_the_middle_of_the_require_list() {
    let mut config = WorkflowConfig::default();
    config.prefixes.insert(
        "perf".to_owned(),
        vec![
            RequireAtom::AgentPremise,
            RequireAtom::Spec,
            RequireAtom::BusinessPremise,
        ],
    );

    let missing_for_perf = missing_atoms("perf", &config, &[], true);

    assert_eq!(
        missing_for_perf,
        Some(vec![
            RequireAtom::AgentPremise,
            RequireAtom::BusinessPremise
        ])
    );
}

// A tabela completa da spec: 7 prefixos x 5 estados de premissa x spec válida ou não.
// Cada célula é escrita à mão, sem derivar uma tabela da outra.

const NONE: &[RequireAtom] = &[];
const BUSINESS: &[RequireAtom] = &[RequireAtom::BusinessPremise];
const BUSINESS_AND_SPEC: &[RequireAtom] = &[RequireAtom::BusinessPremise, RequireAtom::Spec];
const SPEC: &[RequireAtom] = &[RequireAtom::Spec];
const TECHNICAL: &[RequireAtom] = &[RequireAtom::TechnicalPremiseConfirmed];
const AGENT: &[RequireAtom] = &[RequireAtom::AgentPremise];

const ALL_PREFIXES: [&str; 7] = [
    "feat", "fix", "refactor", "tests", "bugfix", "chore", "docs",
];

/// Sem premissa, negócio pendente, negócio confirmada, técnica autônoma, técnica confirmada.
fn premise_states() -> [Vec<PremiseFact>; 5] {
    [
        vec![],
        vec![pending_business()],
        vec![confirmed_business()],
        vec![autonomous_technical()],
        vec![confirmed_technical()],
    ]
}

/// Por prefixo, o que falta em cada estado de `premise_states`, na mesma ordem.
type Row = (&'static str, [&'static [RequireAtom]; 5]);

const TABLE_WITH_A_VALID_SPEC: [Row; 7] = [
    ("feat", [BUSINESS, BUSINESS, NONE, BUSINESS, BUSINESS]),
    ("fix", [BUSINESS, BUSINESS, NONE, BUSINESS, BUSINESS]),
    (
        "refactor",
        [TECHNICAL, TECHNICAL, TECHNICAL, TECHNICAL, NONE],
    ),
    ("tests", [TECHNICAL, TECHNICAL, TECHNICAL, TECHNICAL, NONE]),
    ("bugfix", [AGENT, AGENT, AGENT, NONE, NONE]),
    ("chore", [NONE, NONE, NONE, NONE, NONE]),
    ("docs", [NONE, NONE, NONE, NONE, NONE]),
];

const TABLE_WITHOUT_A_VALID_SPEC: [Row; 7] = [
    (
        "feat",
        [
            BUSINESS_AND_SPEC,
            BUSINESS_AND_SPEC,
            SPEC,
            BUSINESS_AND_SPEC,
            BUSINESS_AND_SPEC,
        ],
    ),
    (
        "fix",
        [
            BUSINESS_AND_SPEC,
            BUSINESS_AND_SPEC,
            SPEC,
            BUSINESS_AND_SPEC,
            BUSINESS_AND_SPEC,
        ],
    ),
    (
        "refactor",
        [TECHNICAL, TECHNICAL, TECHNICAL, TECHNICAL, NONE],
    ),
    ("tests", [TECHNICAL, TECHNICAL, TECHNICAL, TECHNICAL, NONE]),
    ("bugfix", [AGENT, AGENT, AGENT, NONE, NONE]),
    ("chore", [NONE, NONE, NONE, NONE, NONE]),
    ("docs", [NONE, NONE, NONE, NONE, NONE]),
];

fn computed_cells(spec_valid: bool) -> Vec<(String, Option<Vec<RequireAtom>>)> {
    let mut cells = Vec::new();
    for (index, premises) in premise_states().iter().enumerate() {
        for prefix in ALL_PREFIXES {
            cells.push((
                format!("{prefix} / premise state {index}"),
                missing_with_spec(prefix, premises, spec_valid),
            ));
        }
    }

    cells
}

fn expected_cells(table: &[Row; 7]) -> Vec<(String, Option<Vec<RequireAtom>>)> {
    let mut cells = Vec::new();
    for index in 0..5 {
        for (prefix, row) in table {
            cells.push((
                format!("{prefix} / premise state {index}"),
                Some(row[index].to_vec()),
            ));
        }
    }

    cells
}

#[test]
fn it_should_require_in_every_cell_exactly_what_the_matrix_says_when_the_spec_is_valid() {
    assert_eq!(
        computed_cells(true),
        expected_cells(&TABLE_WITH_A_VALID_SPEC)
    );
}

#[test]
fn it_should_require_in_every_cell_exactly_what_the_matrix_says_when_the_spec_is_not_valid() {
    assert_eq!(
        computed_cells(false),
        expected_cells(&TABLE_WITHOUT_A_VALID_SPEC)
    );
}
