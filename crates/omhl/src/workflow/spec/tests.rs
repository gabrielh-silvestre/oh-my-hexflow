use std::collections::HashMap;

use crate::workflow::config::{SpecsConfig, WorkflowConfig};
use crate::workflow::gap::{Answerer, GapState};
use crate::workflow::spec::{
    AmendmentState, DivergenceFact, SpecFacts, SpecFile, amendment_divergence_mismatch,
    amendment_state, is_amendment_id, validate_graph, validate_spec,
};

// Camada pura: `crate::workflow::spec::validate_spec`

// Uma parte válida de 19 linhas; o índice no vetor é a linha menos um.
const PART_OBJECTIVE_LINE: usize = 3;
const PART_CMP_LINE: usize = 11;
const PART_CA_LINE: usize = 15;

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

// Uma emenda válida de 11 linhas que corrige o `CMP-001` da parte acima.
const AMENDMENT_ITEM_LINE: usize = 7;
const AMENDMENT_MOTIVE_BODY_LINE: usize = 11;

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

fn padded_part(total_lines: usize) -> Vec<&'static str> {
    let mut lines = valid_part();
    lines.resize(total_lines, "Mais uma linha.");

    lines
}

fn default_config() -> SpecsConfig {
    WorkflowConfig::default().specs
}

type Files<'a> = &'a [(&'a str, Vec<&'a str>)];

/// Cada violação vira `"<arquivo>:<linha> <código>"`, na ordem devolvida.
fn validate(parts: Files, amendments: Files, config: &SpecsConfig) -> Vec<String> {
    let part_texts: Vec<String> = parts.iter().map(|(_, lines)| text(lines)).collect();
    let amendment_texts: Vec<String> = amendments.iter().map(|(_, lines)| text(lines)).collect();
    let part_files: Vec<SpecFile> = parts
        .iter()
        .zip(&part_texts)
        .map(|((name, _), text)| SpecFile { name, text })
        .collect();
    let amendment_files: Vec<SpecFile> = amendments
        .iter()
        .zip(&amendment_texts)
        .map(|((name, _), text)| SpecFile { name, text })
        .collect();

    validate_spec(&part_files, &amendment_files, config)
        .iter()
        .map(|violation| format!("{} {}", violation.path, violation.code))
        .collect()
}

fn validate_part(lines: Vec<&str>) -> Vec<String> {
    validate(&[("parte-01.md", lines)], &[], &default_config())
}

fn validate_amendment(lines: Vec<&str>) -> Vec<String> {
    validate(
        &[("parte-01.md", valid_part())],
        &[("emenda-01.md", lines)],
        &default_config(),
    )
}

fn with_line<'a>(mut lines: Vec<&'a str>, line: usize, replacement: &'a str) -> Vec<&'a str> {
    lines[line - 1] = replacement;

    lines
}

// Partes: aprovadas

#[test]
fn it_should_accept_a_valid_part() {
    let violations = validate_part(valid_part());

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_a_part_of_exactly_max_lines() {
    let violations = validate_part(padded_part(300));

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_two_valid_parts_with_distinct_ids() {
    let second = with_line(
        with_line(
            valid_part(),
            PART_CMP_LINE,
            "- CMP-002: o sistema encerra a sessão.",
        ),
        PART_CA_LINE,
        "- CA-002: logout invalida a sessão.",
    );

    let violations = validate(
        &[("parte-01.md", valid_part()), ("parte-02.md", second)],
        &[],
        &default_config(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_a_well_formed_pending_marker() {
    let lines = with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "Depende de [PENDENTE: gaps.login-timeout].",
    );

    let violations = validate_part(lines);

    assert_eq!(violations, Vec::<String>::new());
}

// Partes: seções

#[test]
fn it_should_reject_a_part_that_misses_a_section_at_line_one() {
    let mut lines = valid_part();
    lines.truncate(16);

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:1 sections-invalid"]);
}

#[test]
fn it_should_reject_sections_out_of_order_at_the_first_misplaced_heading() {
    let lines = valid_part();
    let swapped = [&lines[..8], &lines[12..16], &lines[8..12], &lines[16..]].concat();

    let violations = validate_part(swapped);

    assert_eq!(violations, vec!["parte-01.md:9 sections-invalid"]);
}

#[test]
fn it_should_reject_an_extra_section_at_its_heading() {
    let mut lines = valid_part();
    lines.extend(["", "## Extra", "", "Texto."]);

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:21 sections-invalid"]);
}

#[test]
fn it_should_reject_an_empty_section_at_its_heading() {
    let lines = with_line(valid_part(), PART_OBJECTIVE_LINE, "   ");

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:1 section-empty"]);
}

#[test]
fn it_should_reject_a_behaviors_section_with_only_prose_at_its_heading() {
    let lines = with_line(
        valid_part(),
        PART_CMP_LINE,
        "O sistema autentica o usuário.",
    );

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:9 section-without-items"]);
}

#[test]
fn it_should_reject_a_criteria_section_with_only_prose_at_its_heading() {
    let lines = with_line(valid_part(), PART_CA_LINE, "Login válido devolve sessão.");

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:13 section-without-items"]);
}

#[test]
fn it_should_judge_the_items_of_each_part_on_its_own() {
    let prose_only_behaviors = with_line(
        with_line(valid_part(), PART_CMP_LINE, "O sistema encerra a sessão."),
        PART_CA_LINE,
        "- CA-002: logout invalida a sessão.",
    );

    let violations = validate(
        &[
            ("parte-01.md", valid_part()),
            ("parte-02.md", prose_only_behaviors),
        ],
        &[],
        &default_config(),
    );

    assert_eq!(violations, vec!["parte-02.md:9 section-without-items"]);
}

#[test]
fn it_should_report_an_empty_items_section_only_as_empty() {
    for (item_line, heading_line) in [(PART_CMP_LINE, 9), (PART_CA_LINE, 13)] {
        let lines = with_line(valid_part(), item_line, "");

        let violations = validate_part(lines);

        assert_eq!(
            violations,
            vec![format!("parte-01.md:{heading_line} section-empty")],
            "heading at line {heading_line}"
        );
    }
}

// Partes: ids dos itens

#[test]
fn it_should_reject_a_cmp_id_that_is_not_three_digits() {
    let lines = with_line(valid_part(), PART_CMP_LINE, "- CMP-1: o sistema autentica.");

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:11 item-id-invalid"]);
}

#[test]
fn it_should_reject_a_ca_id_that_is_not_three_digits() {
    let lines = with_line(valid_part(), PART_CA_LINE, "- CA-01: login válido.");

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:15 item-id-invalid"]);
}

#[test]
fn it_should_reject_a_ca_item_inside_the_behaviors_section() {
    let lines = with_line(
        valid_part(),
        PART_CMP_LINE,
        "- CA-002: critério no lugar errado.",
    );

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:11 item-id-invalid"]);
}

#[test]
fn it_should_reject_a_repeated_id_inside_one_part_at_the_second_occurrence() {
    let lines = with_line(valid_part(), 12, "- CMP-001: repetido.");

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:12 item-id-duplicate"]);
}

#[test]
fn it_should_reject_an_id_repeated_across_parts_at_the_later_part() {
    let violations = validate(
        &[("parte-01.md", valid_part()), ("parte-02.md", valid_part())],
        &[],
        &default_config(),
    );

    assert_eq!(
        violations,
        vec![
            "parte-02.md:11 item-id-duplicate",
            "parte-02.md:15 item-id-duplicate"
        ]
    );
}

// Partes: limite de linhas

#[test]
fn it_should_reject_a_part_of_301_lines_at_the_first_line_over_the_limit() {
    let violations = validate_part(padded_part(301));

    assert_eq!(violations, vec!["parte-01.md:301 file-too-long"]);
}

#[test]
fn it_should_apply_the_max_lines_from_the_config() {
    let config = SpecsConfig {
        max_lines: 10,
        ..default_config()
    };

    let violations = validate(&[("parte-01.md", valid_part())], &[], &config);

    assert_eq!(violations, vec!["parte-01.md:11 file-too-long"]);
}

// Partes: placeholders e marcador de pendência

#[test]
fn it_should_reject_every_default_placeholder_at_its_line() {
    for placeholder in ["TODO", "TBD", "FIXME", "???"] {
        let line = format!("Permitir o login {placeholder}.");

        let violations = validate_part(with_line(valid_part(), PART_OBJECTIVE_LINE, &line));

        assert_eq!(
            violations,
            vec!["parte-01.md:3 placeholder-found"],
            "placeholder {placeholder}"
        );
    }
}

#[test]
fn it_should_report_a_line_with_two_placeholders_once() {
    let lines = with_line(valid_part(), PART_OBJECTIVE_LINE, "TODO e TBD aqui.");

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:3 placeholder-found"]);
}

#[test]
fn it_should_use_the_placeholders_from_the_config() {
    let config = SpecsConfig {
        placeholders: vec!["XXX".to_owned()],
        ..default_config()
    };
    let lines = with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "TODO acabou, mas XXX não.",
    );

    let violations = validate(&[("parte-01.md", lines)], &[], &config);

    assert_eq!(violations, vec!["parte-01.md:3 placeholder-found"]);
}

#[test]
fn it_should_not_reject_a_default_placeholder_the_config_dropped() {
    let config = SpecsConfig {
        placeholders: vec!["XXX".to_owned()],
        ..default_config()
    };
    let lines = with_line(valid_part(), PART_OBJECTIVE_LINE, "TODO é só uma palavra.");

    let violations = validate(&[("parte-01.md", lines)], &[], &config);

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_each_malformed_pending_marker_at_its_line() {
    let malformed = [
        "[PENDENTE: gap-1]",
        "[PENDENTE gaps.x]",
        "[PENDENTE: gaps.]",
        "[PENDENTE: gaps.x",
        "[PENDENTE: gaps.X]",
    ];
    for marker in malformed {
        let line = format!("Depende de {marker}.");

        let violations = validate_part(with_line(valid_part(), PART_OBJECTIVE_LINE, &line));

        assert_eq!(
            violations,
            vec!["parte-01.md:3 pending-marker-invalid"],
            "marker {marker}"
        );
    }
}

#[test]
fn it_should_accept_a_pending_marker_with_a_multi_segment_gap_id() {
    let lines = with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "Depende de [PENDENTE: gaps.login.timeout].",
    );

    let violations = validate_part(lines);

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_a_pending_marker_whose_gap_id_has_a_dangling_or_invalid_segment() {
    let malformed = [
        "[PENDENTE: gaps.a.]",
        "[PENDENTE: gaps.a.B]",
        "[PENDENTE: gaps.a.-b]",
    ];
    for marker in malformed {
        let line = format!("Depende de {marker}.");

        let violations = validate_part(with_line(valid_part(), PART_OBJECTIVE_LINE, &line));

        assert_eq!(
            violations,
            vec!["parte-01.md:3 pending-marker-invalid"],
            "marker {marker}"
        );
    }
}

// Ordem das violações

#[test]
fn it_should_list_violations_by_part_then_line_then_amendments() {
    let late = with_line(valid_part(), PART_CA_LINE, "- CA-001: TODO.");
    let early = with_line(
        with_line(
            with_line(
                valid_part(),
                PART_CMP_LINE,
                "- CMP-002: o sistema encerra a sessão.",
            ),
            PART_CA_LINE,
            "- CA-002: logout invalida a sessão.",
        ),
        PART_OBJECTIVE_LINE,
        "TODO.",
    );
    let amendment = with_line(valid_amendment(), AMENDMENT_MOTIVE_BODY_LINE, "TODO.");
    let late_and_early = [("parte-01.md", late), ("parte-02.md", early)];

    let violations = validate(
        &late_and_early,
        &[("emenda-01.md", amendment)],
        &default_config(),
    );

    assert_eq!(
        violations,
        vec![
            "parte-01.md:15 placeholder-found",
            "parte-02.md:3 placeholder-found",
            "emenda-01.md:11 placeholder-found",
        ]
    );
}

// Emendas: aprovadas

#[test]
fn it_should_accept_a_valid_amendment() {
    let violations = validate_amendment(valid_amendment());

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_an_amendment_that_corrects_a_ca_item() {
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-001 CA-001: login válido devolve sessão e token.",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_an_amendment_that_corrects_an_emd_of_a_lower_numbered_amendment() {
    let second = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-002 EMD-001: ajuste do ajuste.",
    );

    let violations = validate(
        &[("parte-01.md", valid_part())],
        &[
            ("emenda-01.md", valid_amendment()),
            ("emenda-02.md", second),
        ],
        &default_config(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

// Emendas: seções

#[test]
fn it_should_reject_an_amendment_that_misses_a_section_at_line_one() {
    let mut lines = valid_amendment();
    lines.truncate(8);

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:1 sections-invalid"]);
}

#[test]
fn it_should_reject_amendment_sections_out_of_order_at_the_first_misplaced_heading() {
    let lines = valid_amendment();
    let swapped = [&lines[..4], &lines[8..], &[""], &lines[4..8]].concat();

    let violations = validate_amendment(swapped);

    assert_eq!(violations, vec!["emenda-01.md:5 sections-invalid"]);
}

#[test]
fn it_should_reject_an_empty_amendment_section_at_its_heading() {
    let lines = with_line(valid_amendment(), AMENDMENT_MOTIVE_BODY_LINE, "");

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:9 section-empty"]);
}

// Emendas: divergência citada

#[test]
fn it_should_reject_an_amendment_that_cites_no_divergence_id() {
    let lines = with_line(valid_amendment(), 3, "divergence.login-cmp-001");

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:1 divergence-ref-invalid"]);
}

#[test]
fn it_should_reject_an_amendment_that_cites_two_divergence_ids() {
    let lines = with_line(
        valid_amendment(),
        3,
        "divergences.login-a e divergences.login-b",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:1 divergence-ref-invalid"]);
}

#[test]
fn it_should_accept_an_amendment_that_cites_the_whole_multi_segment_divergence_id() {
    let lines = with_line(valid_amendment(), 3, "divergences.login.cmp-001");
    let text = text(&lines);
    let file = SpecFile {
        name: "emenda-01.md",
        text: &text,
    };

    let mismatch = amendment_divergence_mismatch(&file, "divergences.login.cmp-001");

    assert_eq!(mismatch, None);
}

#[test]
fn it_should_flag_an_amendment_that_cites_a_longer_id_than_the_reviewed_divergence() {
    let lines = with_line(valid_amendment(), 3, "divergences.login.cmp-001");
    let text = text(&lines);
    let file = SpecFile {
        name: "emenda-01.md",
        text: &text,
    };

    let mismatch = amendment_divergence_mismatch(&file, "divergences.login");

    assert_eq!(
        mismatch.map(|violation| format!("{} {}", violation.path, violation.code)),
        Some("emenda-01.md:3 amendment-divergence-mismatch".to_owned())
    );
}

// Emendas: itens de correção

#[test]
fn it_should_reject_an_emd_id_that_is_not_three_digits() {
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-1 CMP-001: novo valor.",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:7 item-id-invalid"]);
}

#[test]
fn it_should_reject_a_correction_item_without_the_colon() {
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-001 CMP-001 novo valor",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:7 correction-invalid"]);
}

#[test]
fn it_should_reject_a_correction_item_with_an_empty_new_value() {
    let lines = with_line(valid_amendment(), AMENDMENT_ITEM_LINE, "- EMD-001 CMP-001:");

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:7 correction-invalid"]);
}

#[test]
fn it_should_reject_a_correction_whose_target_is_not_defined_in_any_part() {
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-001 CMP-099: novo valor.",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:7 correction-target-unknown"]);
}

#[test]
fn it_should_reject_a_correction_whose_target_emd_does_not_exist() {
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-001 EMD-009: novo valor.",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:7 correction-target-unknown"]);
}

#[test]
fn it_should_reject_a_correction_whose_target_emd_is_in_a_higher_numbered_amendment() {
    let first = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-001 EMD-002: novo valor.",
    );
    let second = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-002 CMP-001: outro valor.",
    );

    let violations = validate(
        &[("parte-01.md", valid_part())],
        &[("emenda-01.md", first), ("emenda-02.md", second)],
        &default_config(),
    );

    assert_eq!(violations, vec!["emenda-01.md:7 correction-target-unknown"]);
}

#[test]
fn it_should_reject_a_correction_whose_target_emd_is_in_the_same_amendment() {
    let lines = with_line(valid_amendment(), 8, "- EMD-002 EMD-001: ajuste.");

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:8 correction-target-unknown"]);
}

// Emendas: ids EMD

#[test]
fn it_should_reject_an_emd_id_repeated_inside_one_amendment_at_the_second_occurrence() {
    let lines = with_line(valid_amendment(), 8, "- EMD-001 CA-001: repetido.");

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:8 item-id-duplicate"]);
}

#[test]
fn it_should_reject_an_emd_id_repeated_across_amendments_at_the_later_one() {
    let violations = validate(
        &[("parte-01.md", valid_part())],
        &[
            ("emenda-01.md", valid_amendment()),
            ("emenda-02.md", valid_amendment()),
        ],
        &default_config(),
    );

    assert_eq!(violations, vec!["emenda-02.md:7 item-id-duplicate"]);
}

// Emendas: nome do arquivo

#[test]
fn it_should_reject_an_amendment_number_that_is_not_two_digits() {
    for name in ["emenda-1.md", "emenda-001.md", "emenda-ab.md"] {
        let violations = validate(
            &[("parte-01.md", valid_part())],
            &[(name, valid_amendment())],
            &default_config(),
        );

        assert_eq!(
            violations,
            vec![format!("{name}:1 amendment-name-invalid")],
            "name {name}"
        );
    }
}

#[test]
fn it_should_reject_a_repeated_amendment_number_at_the_second_file() {
    let other = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-002 CMP-001: outro valor.",
    );

    let violations = validate(
        &[("parte-01.md", valid_part())],
        &[("emenda-01.md", valid_amendment()), ("emenda-01.md", other)],
        &default_config(),
    );

    assert_eq!(
        violations,
        vec!["emenda-01.md:1 amendment-number-duplicate"]
    );
}

// Emendas: limites e placeholders

#[test]
fn it_should_apply_max_lines_to_amendments() {
    let mut lines = valid_amendment();
    lines.resize(301, "Mais uma linha.");

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:301 file-too-long"]);
}

#[test]
fn it_should_apply_placeholders_to_amendments() {
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_MOTIVE_BODY_LINE,
        "Motivo TODO.",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:11 placeholder-found"]);
}

#[test]
fn it_should_apply_the_pending_marker_format_to_amendments() {
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_MOTIVE_BODY_LINE,
        "Depende de [PENDENTE: gap-1].",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:11 pending-marker-invalid"]);
}

// Emendas: não definem itens

#[test]
fn it_should_reject_a_cmp_item_defined_inside_the_corrections_section() {
    let lines = with_line(valid_amendment(), 8, "- CMP-004: comportamento novo.");

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:8 amendment-defines-item"]);
}

#[test]
fn it_should_reject_a_ca_item_defined_inside_the_motive_section() {
    let lines = with_line(
        valid_amendment(),
        AMENDMENT_MOTIVE_BODY_LINE,
        "- CA-009: critério novo.",
    );

    let violations = validate_amendment(lines);

    assert_eq!(violations, vec!["emenda-01.md:11 amendment-defines-item"]);
}

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";

const BUSINESS_PREMISE: &str = "premises.login-rule";
const DIVERGENCE: &str = "divergences.login-cmp-001";

const PART_FIRST_CITATION_LINE: usize = 7;
const PART_SECOND_CITATION_LINE: usize = 8;

// Camada pura: `crate::workflow::spec::{validate_graph, amendment_state}` com fatos de mentira

struct FakeNode {
    confirmed: bool,
    kind: Option<&'static str>,
    work_item: Option<&'static str>,
    late: bool,
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
            late: false,
        };
        self.nodes.insert(id.to_owned(), node);

        self
    }

    fn late(mut self, id: &str) -> FakeFacts {
        self.nodes
            .get_mut(id)
            .expect("the premise was added before")
            .late = true;

        self
    }

    fn guideline(mut self, id: &str, confirmed: bool) -> FakeFacts {
        let node = FakeNode {
            confirmed,
            kind: None,
            work_item: None,
            late: false,
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

    fn late(&self, id: &str) -> bool {
        self.nodes.get(id).is_some_and(|node| node.late)
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

fn base_facts() -> FakeFacts {
    FakeFacts::default()
        .premise(BUSINESS_PREMISE, "business", W1, true)
        .guideline("guidelines.tone", true)
        .divergence(DIVERGENCE, W1, "open", "CMP-001")
}

/// Cada violação vira `"<arquivo>:<linha> <código>"`, na ordem devolvida.
fn graph_violations(
    part_lines: &[&str],
    amendment_lines: Option<&[&str]>,
    config: &SpecsConfig,
    facts: &FakeFacts,
) -> Vec<String> {
    graph_violations_named(
        ("parte-01.md", part_lines),
        amendment_lines.map(|lines| ("emenda-01.md", lines)),
        config,
        facts,
    )
}

fn graph_violations_named(
    part: (&str, &[&str]),
    amendment: Option<(&str, &[&str])>,
    config: &SpecsConfig,
    facts: &FakeFacts,
) -> Vec<String> {
    let part_text = text(part.1);
    let amendment_text = amendment.map(|(_, lines)| text(lines));
    let parts = [SpecFile {
        name: part.0,
        text: &part_text,
    }];
    let amendments: Vec<SpecFile> = amendment_text
        .iter()
        .zip(amendment)
        .map(|(text, (name, _))| SpecFile { name, text })
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
fn it_should_reject_a_spec_whose_only_cited_business_premise_is_late() {
    let facts = base_facts().late(BUSINESS_PREMISE);

    let violations = part_violations(&valid_part(), &facts);

    assert_eq!(
        violations,
        vec!["parte-01.md:5 main-business-premise-missing"]
    );
}

#[test]
fn it_should_accept_a_spec_that_cites_a_late_premise_next_to_a_main_one() {
    let facts = base_facts()
        .premise("premises.late-rule", "business", W1, true)
        .late("premises.late-rule");
    let lines = with_line(
        valid_part(),
        PART_SECOND_CITATION_LINE,
        "- premises.late-rule",
    );

    let violations = part_violations(&lines, &facts);

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

// Vocabulário inglês: fixtures com o mesmo leiaute das portuguesas, então as constantes de linha valem

fn english_part() -> Vec<&'static str> {
    vec![
        "## Objective",
        "",
        "Allow the login.",
        "",
        "## Applicable directives and premises",
        "",
        "- premises.login-rule",
        "",
        "## Behaviors",
        "",
        "- CMP-001: the system authenticates the user.",
        "",
        "## Acceptance criteria",
        "",
        "- CA-001: a valid login returns a session.",
        "",
        "## Out of scope",
        "",
        "Logout.",
    ]
}

fn english_amendment() -> Vec<&'static str> {
    vec![
        "## Divergence",
        "",
        "divergences.login-cmp-001",
        "",
        "## Corrections",
        "",
        "- EMD-001 CMP-001: the system authenticates the user and logs the access.",
        "",
        "## Reason",
        "",
        "The spec omitted the access log.",
    ]
}

fn validate_english_part(lines: Vec<&str>) -> Vec<String> {
    validate(&[("part-01.md", lines)], &[], &default_config())
}

fn validate_english_amendment(lines: Vec<&str>) -> Vec<String> {
    validate(
        &[("part-01.md", english_part())],
        &[("amendment-01.md", lines)],
        &default_config(),
    )
}

fn english_graph_violations(
    part_lines: &[&str],
    amendment_lines: Option<&[&str]>,
    facts: &FakeFacts,
) -> Vec<String> {
    graph_violations_named(
        ("part-01.md", part_lines),
        amendment_lines.map(|lines| ("amendment-01.md", lines)),
        &default_config(),
        facts,
    )
}

fn heading_lines(lines: &[&str]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("## "))
        .map(|(index, _)| index + 1)
        .collect()
}

fn sections_invalid(violations: &[String]) -> Vec<String> {
    violations
        .iter()
        .filter(|violation| violation.ends_with(" sections-invalid"))
        .cloned()
        .collect()
}

fn without_heading(mut lines: Vec<&'static str>, section: usize) -> Vec<&'static str> {
    lines.remove(heading_lines(&lines)[section] - 1);

    lines
}

fn with_swapped_headings(
    mut lines: Vec<&'static str>,
    first: usize,
    second: usize,
) -> Vec<&'static str> {
    let headings = heading_lines(&lines);
    lines.swap(headings[first] - 1, headings[second] - 1);

    lines
}

fn with_repeated_heading(mut lines: Vec<&'static str>, section: usize) -> Vec<&'static str> {
    let at = heading_lines(&lines)[section] - 1;
    lines.insert(at + 1, lines[at]);

    lines
}

macro_rules! heading_tests {
    (
        $fixture:ident, $validate:ident, $file:literal, $count:literal;
        $($section:literal => $missing:ident, $swapped:ident, $repeated:ident;)*
    ) => {
        $(
            #[test]
            fn $missing() {
                let section: usize = $section;
                let original = $fixture();
                let line = if section + 1 < $count { heading_lines(&original)[section + 1] - 1 } else { 1 };

                let violations = $validate(without_heading(original, section));

                assert_eq!(sections_invalid(&violations), vec![format!("{}:{line} sections-invalid", $file)]);
            }

            #[test]
            fn $swapped() {
                let section: usize = $section;
                let neighbor = if section + 1 < $count { section + 1 } else { section - 1 };
                let original = $fixture();
                let line = heading_lines(&original)[section.min(neighbor)];

                let violations = $validate(with_swapped_headings(original, section, neighbor));

                assert_eq!(sections_invalid(&violations), vec![format!("{}:{line} sections-invalid", $file)]);
            }

            #[test]
            fn $repeated() {
                let section: usize = $section;
                let original = $fixture();
                let line = heading_lines(&original)[section] + 1;

                let violations = $validate(with_repeated_heading(original, section));

                assert_eq!(sections_invalid(&violations), vec![format!("{}:{line} sections-invalid", $file)]);
            }
        )*
    };
}

heading_tests! {
    english_part, validate_english_part, "part-01.md", 5;
    0 => it_should_reject_an_english_part_without_the_objective_heading,
        it_should_reject_an_english_part_with_the_objective_heading_swapped,
        it_should_reject_an_english_part_with_the_objective_heading_repeated;
    1 => it_should_reject_an_english_part_without_the_directives_heading,
        it_should_reject_an_english_part_with_the_directives_heading_swapped,
        it_should_reject_an_english_part_with_the_directives_heading_repeated;
    2 => it_should_reject_an_english_part_without_the_behaviors_heading,
        it_should_reject_an_english_part_with_the_behaviors_heading_swapped,
        it_should_reject_an_english_part_with_the_behaviors_heading_repeated;
    3 => it_should_reject_an_english_part_without_the_acceptance_criteria_heading,
        it_should_reject_an_english_part_with_the_acceptance_criteria_heading_swapped,
        it_should_reject_an_english_part_with_the_acceptance_criteria_heading_repeated;
    4 => it_should_reject_an_english_part_without_the_out_of_scope_heading,
        it_should_reject_an_english_part_with_the_out_of_scope_heading_swapped,
        it_should_reject_an_english_part_with_the_out_of_scope_heading_repeated;
}

heading_tests! {
    english_amendment, validate_english_amendment, "amendment-01.md", 3;
    0 => it_should_reject_an_english_amendment_without_the_divergence_heading,
        it_should_reject_an_english_amendment_with_the_divergence_heading_swapped,
        it_should_reject_an_english_amendment_with_the_divergence_heading_repeated;
    1 => it_should_reject_an_english_amendment_without_the_corrections_heading,
        it_should_reject_an_english_amendment_with_the_corrections_heading_swapped,
        it_should_reject_an_english_amendment_with_the_corrections_heading_repeated;
    2 => it_should_reject_an_english_amendment_without_the_reason_heading,
        it_should_reject_an_english_amendment_with_the_reason_heading_swapped,
        it_should_reject_an_english_amendment_with_the_reason_heading_repeated;
}

#[test]
fn it_should_accept_a_valid_english_part() {
    let violations = validate_english_part(english_part());

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_a_valid_english_amendment() {
    let violations = validate_english_amendment(english_amendment());

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_an_english_cmp_id_that_is_not_three_digits_at_its_line() {
    let lines = with_line(
        english_part(),
        PART_CMP_LINE,
        "- CMP-x: the system logs in.",
    );

    let violations = validate_english_part(lines);

    assert_eq!(violations, vec!["part-01.md:11 item-id-invalid"]);
}

#[test]
fn it_should_reject_a_repeated_english_item_id_at_the_second_occurrence() {
    let lines = with_line(
        english_part(),
        PART_CMP_LINE + 1,
        "- CMP-001: the system logs out.",
    );

    let violations = validate_english_part(lines);

    assert_eq!(violations, vec!["part-01.md:12 item-id-duplicate"]);
}

#[test]
fn it_should_reject_an_english_behaviors_section_with_only_prose_at_its_heading() {
    let lines = with_line(
        english_part(),
        PART_CMP_LINE,
        "The system authenticates the user.",
    );

    let violations = validate_english_part(lines);

    assert_eq!(violations, vec!["part-01.md:9 section-without-items"]);
}

#[test]
fn it_should_reject_an_english_criteria_section_with_only_prose_at_its_heading() {
    let lines = with_line(
        english_part(),
        PART_CA_LINE,
        "A valid login returns a session.",
    );

    let violations = validate_english_part(lines);

    assert_eq!(violations, vec!["part-01.md:13 section-without-items"]);
}

#[test]
fn it_should_reject_a_part_whose_first_heading_is_of_neither_vocabulary_at_line_one() {
    let lines = with_line(english_part(), 1, "## Overview");

    let violations = validate_english_part(lines);

    assert_eq!(violations, vec!["part-01.md:1 sections-invalid"]);
}

// Vocabulário inglês: um título do outro vocabulário e arquivos de vocabulários diferentes

#[test]
fn it_should_reject_a_portuguese_heading_inside_an_english_part_at_its_line() {
    let lines = with_line(english_part(), 9, "## Comportamentos");

    let violations = validate_english_part(lines);

    assert_eq!(violations, vec!["part-01.md:9 sections-invalid"]);
}

#[test]
fn it_should_reject_an_english_heading_inside_a_portuguese_part_at_its_line() {
    let lines = with_line(valid_part(), 13, "## Acceptance criteria");

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:13 sections-invalid"]);
}

#[test]
fn it_should_reject_a_portuguese_heading_inside_an_english_amendment_at_its_line() {
    let lines = with_line(english_amendment(), 5, "## Correções");

    let violations = validate_english_amendment(lines);

    assert_eq!(violations, vec!["amendment-01.md:5 sections-invalid"]);
}

#[test]
fn it_should_accept_a_portuguese_part_next_to_an_english_part() {
    let second = with_line(
        with_line(
            english_part(),
            PART_CMP_LINE,
            "- CMP-002: the system ends the session.",
        ),
        PART_CA_LINE,
        "- CA-002: logout invalidates the session.",
    );

    let violations = validate(
        &[("parte-01.md", valid_part()), ("part-02.md", second)],
        &[],
        &default_config(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_an_english_and_a_portuguese_amendment_over_an_english_part() {
    let portuguese = with_line(
        valid_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-002 CA-001: login válido devolve sessão e registra o acesso.",
    );

    let violations = validate(
        &[("part-01.md", english_part())],
        &[
            ("amendment-01.md", english_amendment()),
            ("emenda-02.md", portuguese),
        ],
        &default_config(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_an_english_and_a_portuguese_amendment_over_a_portuguese_part() {
    let english = with_line(
        english_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-002 CA-001: a valid login returns a session and logs the access.",
    );

    let violations = validate(
        &[("parte-01.md", valid_part())],
        &[
            ("emenda-01.md", valid_amendment()),
            ("amendment-02.md", english),
        ],
        &default_config(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

// Vocabulário inglês: emendas

#[test]
fn it_should_reject_an_english_amendment_that_cites_no_divergence_id() {
    let lines = with_line(english_amendment(), 3, "divergence.login-cmp-001");

    let violations = validate_english_amendment(lines);

    assert_eq!(violations, vec!["amendment-01.md:1 divergence-ref-invalid"]);
}

#[test]
fn it_should_reject_an_english_amendment_that_cites_two_divergence_ids() {
    let lines = with_line(
        english_amendment(),
        3,
        "divergences.login-a and divergences.login-b",
    );

    let violations = validate_english_amendment(lines);

    assert_eq!(violations, vec!["amendment-01.md:1 divergence-ref-invalid"]);
}

#[test]
fn it_should_reject_an_english_correction_whose_target_is_not_defined_in_any_part() {
    let lines = with_line(
        english_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-001 CMP-099: new value.",
    );

    let violations = validate_english_amendment(lines);

    assert_eq!(
        violations,
        vec!["amendment-01.md:7 correction-target-unknown"]
    );
}

#[test]
fn it_should_accept_an_english_amendment_that_corrects_an_emd_of_a_lower_numbered_one() {
    let second = with_line(
        english_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-002 EMD-001: the final value.",
    );

    let violations = validate(
        &[("part-01.md", english_part())],
        &[
            ("amendment-01.md", english_amendment()),
            ("amendment-02.md", second),
        ],
        &default_config(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_an_amendment_item_that_defines_a_cmp_in_the_english_corrections() {
    let lines = with_line(
        english_amendment(),
        AMENDMENT_MOTIVE_BODY_LINE,
        "- CMP-009: new behavior.",
    );

    let violations = validate_english_amendment(lines);

    assert_eq!(
        violations,
        vec!["amendment-01.md:11 amendment-defines-item"]
    );
}

// Famílias de nome de arquivo

#[test]
fn it_should_accept_a_folder_with_only_portuguese_file_names() {
    let violations = validate(
        &[("parte-01.md", valid_part())],
        &[("emenda-01.md", valid_amendment())],
        &default_config(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_accept_a_folder_with_only_english_file_names() {
    let violations = validate(
        &[("part-01.md", english_part())],
        &[("amendment-01.md", english_amendment())],
        &default_config(),
    );

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_an_amendment_number_repeated_across_the_two_name_families() {
    let other = with_line(
        english_amendment(),
        AMENDMENT_ITEM_LINE,
        "- EMD-002 CMP-001: other value.",
    );

    let violations = validate(
        &[("part-01.md", english_part())],
        &[
            ("amendment-02.md", english_amendment()),
            ("emenda-02.md", other),
        ],
        &default_config(),
    );

    assert_eq!(
        violations,
        vec!["emenda-02.md:1 amendment-number-duplicate"]
    );
}

#[test]
fn it_should_reject_an_english_amendment_name_whose_number_has_one_digit() {
    let violations = validate(
        &[("part-01.md", english_part())],
        &[("amendment-1.md", english_amendment())],
        &default_config(),
    );

    assert_eq!(violations, vec!["amendment-1.md:1 amendment-name-invalid"]);
}

#[test]
fn it_should_reject_a_portuguese_amendment_name_whose_number_has_one_digit() {
    let violations = validate(
        &[("parte-01.md", valid_part())],
        &[("emenda-1.md", valid_amendment())],
        &default_config(),
    );

    assert_eq!(violations, vec!["emenda-1.md:1 amendment-name-invalid"]);
}

#[test]
fn it_should_recognize_both_families_as_amendment_ids() {
    let ids = ["amendment-01", "emenda-01", "amendment-1", "amend-01"];

    let recognized: Vec<bool> = ids.iter().map(|id| is_amendment_id(id)).collect();

    assert_eq!(recognized, vec![true, true, false, false]);
}

// Marcador de pendência nos dois vocabulários

fn english_pending_part(marker: &str) -> Vec<String> {
    let line = format!("Depends on {marker}.");

    validate_english_part(with_line(english_part(), PART_OBJECTIVE_LINE, &line))
}

#[test]
fn it_should_accept_a_well_formed_english_pending_marker() {
    let violations = english_pending_part("[PENDING: gaps.login-timeout]");

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_each_malformed_english_pending_marker_at_its_line() {
    let malformed = [
        "[PENDING: gap-1]",
        "[PENDING gaps.x]",
        "[PENDING: gaps.]",
        "[PENDING: gaps.x",
        "[PENDING: gaps.X]",
    ];
    for marker in malformed {
        let violations = english_pending_part(marker);

        assert_eq!(
            violations,
            vec!["part-01.md:3 pending-marker-invalid"],
            "marker {marker}"
        );
    }
}

#[test]
fn it_should_reject_a_well_formed_english_marker_inside_a_portuguese_part() {
    let lines = with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "Depende de [PENDING: gaps.pay-day].",
    );

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:3 pending-marker-invalid"]);
}

#[test]
fn it_should_reject_a_well_formed_portuguese_marker_inside_an_english_part() {
    let violations = english_pending_part("[PENDENTE: gaps.pay-day]");

    assert_eq!(violations, vec!["part-01.md:3 pending-marker-invalid"]);
}

#[test]
fn it_should_reject_a_malformed_english_marker_inside_a_portuguese_part() {
    let lines = with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "Depende de [PENDING gaps.x].",
    );

    let violations = validate_part(lines);

    assert_eq!(violations, vec!["parte-01.md:3 pending-marker-invalid"]);
}

#[test]
fn it_should_reject_an_english_pending_marker_that_points_to_an_open_gap() {
    let facts = base_facts().gap("gaps.pay-day", GapState::Open);
    let lines = with_line(
        english_part(),
        PART_OBJECTIVE_LINE,
        "Allow the login [PENDING: gaps.pay-day].",
    );

    let violations = english_graph_violations(&lines, None, &facts);

    assert_eq!(violations, vec!["part-01.md:3 pending-gap-unresolved"]);
}

#[test]
fn it_should_accept_an_english_pending_marker_that_points_to_an_answered_gap() {
    let facts = base_facts().gap("gaps.pay-day", answered());
    let lines = with_line(
        english_part(),
        PART_OBJECTIVE_LINE,
        "Allow the login [PENDING: gaps.pay-day].",
    );

    let violations = english_graph_violations(&lines, None, &facts);

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_an_english_marker_of_an_open_gap_inside_a_portuguese_part_with_both_codes() {
    let facts = base_facts().gap("gaps.pay-day", GapState::Open);
    let lines = with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "Permitir o login [PENDING: gaps.pay-day].",
    );

    let structural = validate_part(lines.clone());
    let graph = part_violations(&lines, &facts);

    assert_eq!(structural, vec!["parte-01.md:3 pending-marker-invalid"]);
    assert_eq!(graph, vec!["parte-01.md:3 pending-gap-unresolved"]);
}

#[test]
fn it_should_reject_a_portuguese_marker_of_an_open_gap_inside_an_english_part_with_both_codes() {
    let facts = base_facts().gap("gaps.pay-day", GapState::Open);
    let lines = with_line(
        english_part(),
        PART_OBJECTIVE_LINE,
        "Allow the login [PENDENTE: gaps.pay-day].",
    );

    let structural = validate_english_part(lines.clone());
    let graph = english_graph_violations(&lines, None, &facts);

    assert_eq!(structural, vec!["part-01.md:3 pending-marker-invalid"]);
    assert_eq!(graph, vec!["part-01.md:3 pending-gap-unresolved"]);
}

#[test]
fn it_should_reject_an_english_pending_marker_inside_an_english_amendment_that_points_to_an_open_gap()
 {
    let facts = base_facts().gap("gaps.pay-day", GapState::Open);
    let lines = with_line(
        english_amendment(),
        AMENDMENT_MOTIVE_BODY_LINE,
        "Depends on [PENDING: gaps.pay-day].",
    );

    let violations = english_graph_violations(&english_part(), Some(&lines), &facts);

    assert_eq!(
        violations,
        vec!["amendment-01.md:11 pending-gap-unresolved"]
    );
}

// Vocabulário inglês: verificações do grafo pelo título do arquivo

#[test]
fn it_should_reject_an_english_spec_without_a_confirmed_business_premise_at_the_directives_heading()
{
    let lines = with_line(
        english_part(),
        PART_FIRST_CITATION_LINE,
        "- guidelines.tone",
    );

    let violations = english_graph_violations(&lines, None, &base_facts());

    assert_eq!(violations, vec!["part-01.md:5 business-premise-missing"]);
}

#[test]
fn it_should_reject_an_english_spec_whose_only_cited_business_premise_is_late() {
    let facts = base_facts().late(BUSINESS_PREMISE);

    let violations = english_graph_violations(&english_part(), None, &facts);

    assert_eq!(
        violations,
        vec!["part-01.md:5 main-business-premise-missing"]
    );
}

#[test]
fn it_should_reject_a_cited_id_that_does_not_exist_inside_the_english_directives_section() {
    let lines = with_line(
        english_part(),
        PART_SECOND_CITATION_LINE,
        "- premises.unknown",
    );

    let violations = english_graph_violations(&lines, None, &base_facts());

    assert_eq!(violations, vec!["part-01.md:8 cited-id-unknown"]);
}

#[test]
fn it_should_accept_an_english_spec_whose_citations_and_amendment_all_resolve() {
    let violations =
        english_graph_violations(&english_part(), Some(&english_amendment()), &base_facts());

    assert_eq!(violations, Vec::<String>::new());
}

#[test]
fn it_should_reject_an_english_amendment_that_cites_a_divergence_that_does_not_exist() {
    let violations = english_graph_violations(
        &english_part(),
        Some(&english_amendment()),
        &FakeFacts::default().premise(BUSINESS_PREMISE, "business", W1, true),
    );

    assert_eq!(
        violations,
        vec!["amendment-01.md:3 amendment-divergence-unknown"]
    );
}

#[test]
fn it_should_reject_an_english_amendment_that_does_not_correct_the_cmp_of_the_divergence() {
    let facts = base_facts().divergence(DIVERGENCE, W1, "open", "CMP-002");

    let violations = english_graph_violations(&english_part(), Some(&english_amendment()), &facts);

    assert_eq!(
        violations,
        vec!["amendment-01.md:3 amendment-correction-missing"]
    );
}

#[test]
fn it_should_reject_an_english_amendment_that_does_not_correct_the_ca_of_the_divergence() {
    let facts = base_facts().divergence(DIVERGENCE, W1, "open", "CA-001");

    let violations = english_graph_violations(&english_part(), Some(&english_amendment()), &facts);

    assert_eq!(
        violations,
        vec!["amendment-01.md:3 amendment-correction-missing"]
    );
}

#[test]
fn it_should_report_the_english_amendment_divergence_that_differs_from_the_expected_one() {
    let text = text(&english_amendment());
    let file = SpecFile {
        name: "amendment-01.md",
        text: &text,
    };

    let mismatch = amendment_divergence_mismatch(&file, "divergences.other");

    assert_eq!(
        mismatch.map(|violation| format!("{} {}", violation.path, violation.code)),
        Some("amendment-01.md:3 amendment-divergence-mismatch".to_owned())
    );
}
