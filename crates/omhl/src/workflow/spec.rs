//! Puro: recebe o texto dos arquivos e a configuração, devolve violações com `arquivo:linha`.

use std::collections::{HashMap, HashSet};

use super::config::SpecsConfig;
use super::gap::GapState;
use super::names::{FILE_SUFFIX, strip_amendment_prefix};
use crate::graphs::{DIVERGENCE_NAMESPACE, GAP_NAMESPACE, GUIDELINE_NAMESPACE, PREMISE_NAMESPACE};

pub const SPEC_STALE: &str = "spec-stale";
const HEADING_PREFIX: &str = "## ";
const ITEM_PREFIX: &str = "- ";
const CITED_NAMESPACES: [&str; 2] = [PREMISE_NAMESPACE, GUIDELINE_NAMESPACE];
const BUSINESS_KIND: &str = "business";
const CLOSED_RESOLUTIONS: [&str; 2] = ["code-fixed", "accepted"];

/// Cada arquivo é todo de um vocabulário, escolhido pelo primeiro título.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vocab {
    Portuguese,
    English,
}

impl Vocab {
    const ALL: [Vocab; 2] = [Vocab::Portuguese, Vocab::English];

    /// Título que não é de nenhum vocabulário cai no português, para o erro seguir o de sempre.
    fn of_first_title(title: Option<&str>) -> Vocab {
        match title {
            Some("Objective" | "Divergence") => Vocab::English,
            _ => Vocab::Portuguese,
        }
    }

    fn other(self) -> Vocab {
        match self {
            Vocab::Portuguese => Vocab::English,
            Vocab::English => Vocab::Portuguese,
        }
    }

    fn part_sections(self) -> [&'static str; 5] {
        match self {
            Vocab::Portuguese => [
                "Objetivo",
                "Diretivas e premissas aplicáveis",
                "Comportamentos",
                "Critérios de aceitação",
                "Fora de escopo",
            ],
            Vocab::English => [
                "Objective",
                "Applicable directives and premises",
                "Behaviors",
                "Acceptance criteria",
                "Out of scope",
            ],
        }
    }

    fn amendment_sections(self) -> [&'static str; 3] {
        match self {
            Vocab::Portuguese => ["Divergência", "Correções", "Motivo"],
            Vocab::English => ["Divergence", "Corrections", "Reason"],
        }
    }

    fn directives(self) -> &'static str {
        self.part_sections()[1]
    }

    fn behaviors(self) -> &'static str {
        self.part_sections()[2]
    }

    fn criteria(self) -> &'static str {
        self.part_sections()[3]
    }

    fn divergence(self) -> &'static str {
        self.amendment_sections()[0]
    }

    fn corrections(self) -> &'static str {
        self.amendment_sections()[1]
    }

    /// O início do marcador, sem os dois-pontos.
    fn pending_marker(self) -> &'static str {
        match self {
            Vocab::Portuguese => "[PENDENTE",
            Vocab::English => "[PENDING",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpecFile<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// `<arquivo>:<linha>`, com a linha contada a partir de 1.
    pub path: String,
    pub code: &'static str,
}

/// A ordem das variantes é a ordem de desempate de duas violações na mesma linha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Code {
    SectionsInvalid,
    SectionEmpty,
    SectionWithoutItems,
    ItemIdInvalid,
    ItemIdDuplicate,
    FileTooLong,
    PlaceholderFound,
    PendingMarkerInvalid,
    DivergenceRefInvalid,
    CorrectionInvalid,
    CorrectionTargetUnknown,
    AmendmentDefinesItem,
    AmendmentNameInvalid,
    AmendmentNumberDuplicate,
    CitedIdUnknown,
    CitedIdUnconfirmed,
    BusinessPremiseMissing,
    MainBusinessPremiseMissing,
    PendingGapUnresolved,
    AmendmentDivergenceUnknown,
    AmendmentDivergenceForeign,
    AmendmentDivergenceClosed,
    AmendmentCorrectionMissing,
}

impl Code {
    fn as_str(self) -> &'static str {
        match self {
            Code::SectionsInvalid => "sections-invalid",
            Code::SectionEmpty => "section-empty",
            Code::SectionWithoutItems => "section-without-items",
            Code::ItemIdInvalid => "item-id-invalid",
            Code::ItemIdDuplicate => "item-id-duplicate",
            Code::FileTooLong => "file-too-long",
            Code::PlaceholderFound => "placeholder-found",
            Code::PendingMarkerInvalid => "pending-marker-invalid",
            Code::DivergenceRefInvalid => "divergence-ref-invalid",
            Code::CorrectionInvalid => "correction-invalid",
            Code::CorrectionTargetUnknown => "correction-target-unknown",
            Code::AmendmentDefinesItem => "amendment-defines-item",
            Code::AmendmentNameInvalid => "amendment-name-invalid",
            Code::AmendmentNumberDuplicate => "amendment-number-duplicate",
            Code::CitedIdUnknown => "cited-id-unknown",
            Code::CitedIdUnconfirmed => "cited-id-unconfirmed",
            Code::BusinessPremiseMissing => "business-premise-missing",
            Code::MainBusinessPremiseMissing => "main-business-premise-missing",
            Code::PendingGapUnresolved => "pending-gap-unresolved",
            Code::AmendmentDivergenceUnknown => "amendment-divergence-unknown",
            Code::AmendmentDivergenceForeign => "amendment-divergence-foreign",
            Code::AmendmentDivergenceClosed => "amendment-divergence-closed",
            Code::AmendmentCorrectionMissing => "amendment-correction-missing",
        }
    }
}

struct Found {
    line: usize,
    code: Code,
}

struct Section<'a> {
    title: &'a str,
    /// Linha do título, a partir de 1; o corpo vai da linha seguinte até o próximo título.
    line: usize,
    body: Vec<&'a str>,
}

impl Section<'_> {
    fn is_blank(&self) -> bool {
        self.body.iter().all(|line| line.trim().is_empty())
    }
}

struct Item<'a> {
    line: usize,
    section: &'a str,
    /// O que vem depois de `- `.
    text: &'a str,
}

struct Doc<'a> {
    name: &'a str,
    vocab: Vocab,
    lines: Vec<&'a str>,
    sections: Vec<Section<'a>>,
}

impl<'a> Doc<'a> {
    fn new(file: &SpecFile<'a>) -> Doc<'a> {
        let lines: Vec<&str> = file.text.lines().collect();
        let mut sections: Vec<Section> = Vec::new();
        for (index, line) in lines.iter().copied().enumerate() {
            match line.strip_prefix(HEADING_PREFIX) {
                Some(title) => sections.push(Section {
                    title: title.trim_end(),
                    line: index + 1,
                    body: Vec::new(),
                }),
                None => {
                    if let Some(section) = sections.last_mut() {
                        section.body.push(line);
                    }
                }
            }
        }

        Doc {
            name: file.name,
            vocab: Vocab::of_first_title(sections.first().map(|section| section.title)),
            lines,
            sections,
        }
    }

    fn section(&self, title: &str) -> Option<&Section<'a>> {
        self.sections.iter().find(|section| section.title == title)
    }

    fn directives_section(&self) -> Option<&Section<'a>> {
        self.section(self.vocab.directives())
    }

    fn divergence_section(&self) -> Option<&Section<'a>> {
        self.section(self.vocab.divergence())
    }

    fn correction_items(&self) -> Vec<Item<'a>> {
        let corrections = self.vocab.corrections();

        self.items()
            .into_iter()
            .filter(|item| item.section == corrections)
            .collect()
    }

    /// Item é a linha que começa na coluna 0 com `- `; linha indentada é continuação.
    fn items(&self) -> Vec<Item<'a>> {
        let mut items = Vec::new();
        for section in &self.sections {
            for (offset, line) in section.body.iter().copied().enumerate() {
                if let Some(text) = line.strip_prefix(ITEM_PREFIX) {
                    items.push(Item {
                        line: section.line + 1 + offset,
                        section: section.title,
                        text,
                    });
                }
            }
        }

        items
    }

    fn into_violations(self, mut found: Vec<Found>) -> Vec<Violation> {
        found.sort_by_key(|found| (found.line, found.code));

        found
            .into_iter()
            .map(|found| Violation {
                path: format!("{}:{}", self.name, found.line),
                code: found.code.as_str(),
            })
            .collect()
    }
}

/// As partes e as emendas chegam já ordenadas pelo nome.
pub fn validate_spec(
    parts: &[SpecFile<'_>],
    amendments: &[SpecFile<'_>],
    config: &SpecsConfig,
) -> Vec<Violation> {
    let mut violations = Vec::new();
    let mut part_ids = HashSet::new();
    for file in parts {
        let doc = Doc::new(file);
        let found = check_part(&doc, config, &mut part_ids);
        violations.extend(doc.into_violations(found));
    }

    let docs: Vec<Doc> = amendments.iter().map(Doc::new).collect();
    let mut context = AmendmentContext {
        part_ids,
        emd_numbers: defined_emds(&docs),
        seen_emds: HashSet::new(),
        seen_numbers: HashSet::new(),
    };
    for doc in docs {
        let found = check_amendment(&doc, config, &mut context);
        violations.extend(doc.into_violations(found));
    }

    violations
}

fn check_part<'a>(
    doc: &Doc<'a>,
    config: &SpecsConfig,
    defined: &mut HashSet<&'a str>,
) -> Vec<Found> {
    let mut found = check_shared(doc, &doc.vocab.part_sections(), config);
    let items = doc.items();
    let (behaviors, criteria) = (doc.vocab.behaviors(), doc.vocab.criteria());
    for title in [behaviors, criteria] {
        let Some(section) = doc.section(title) else {
            continue;
        };
        // Seção em branco já é `section-empty`; não duplica a violação na mesma linha.
        if !section.is_blank() && items.iter().all(|item| item.section != title) {
            found.push(Found {
                line: section.line,
                code: Code::SectionWithoutItems,
            });
        }
    }
    for item in items {
        let prefix = if item.section == behaviors {
            "CMP-"
        } else if item.section == criteria {
            "CA-"
        } else {
            continue;
        };
        match item_id(&item, prefix) {
            None => found.push(Found {
                line: item.line,
                code: Code::ItemIdInvalid,
            }),
            Some(token) if !defined.insert(token) => found.push(Found {
                line: item.line,
                code: Code::ItemIdDuplicate,
            }),
            Some(_) => {}
        }
    }

    found
}

struct AmendmentContext<'a> {
    part_ids: HashSet<&'a str>,
    emd_numbers: HashMap<&'a str, u32>,
    seen_emds: HashSet<&'a str>,
    seen_numbers: HashSet<u32>,
}

fn check_amendment<'a>(
    doc: &Doc<'a>,
    config: &SpecsConfig,
    context: &mut AmendmentContext<'a>,
) -> Vec<Found> {
    let mut found = check_shared(doc, &doc.vocab.amendment_sections(), config);
    let number = amendment_number(doc.name);
    match number {
        None => found.push(Found {
            line: 1,
            code: Code::AmendmentNameInvalid,
        }),
        Some(number) if !context.seen_numbers.insert(number) => found.push(Found {
            line: 1,
            code: Code::AmendmentNumberDuplicate,
        }),
        Some(_) => {}
    }
    if let Some(section) = doc.divergence_section() {
        let references: usize = section
            .body
            .iter()
            .map(|line| count_divergence_refs(line))
            .sum();
        if references != 1 {
            found.push(Found {
                line: section.line,
                code: Code::DivergenceRefInvalid,
            });
        }
    }
    for item in doc.items() {
        let token = first_token(item.text);
        if token.starts_with("CMP-") || token.starts_with("CA-") {
            found.push(Found {
                line: item.line,
                code: Code::AmendmentDefinesItem,
            });
        } else if item.section == doc.vocab.corrections() {
            check_correction(&item, number, context, &mut found);
        }
    }

    found
}

fn check_correction<'a>(
    item: &Item<'a>,
    number: Option<u32>,
    context: &mut AmendmentContext<'a>,
    found: &mut Vec<Found>,
) {
    let Some(token) = item_id(item, "EMD-") else {
        found.push(Found {
            line: item.line,
            code: Code::ItemIdInvalid,
        });
        return;
    };
    if !context.seen_emds.insert(token) {
        found.push(Found {
            line: item.line,
            code: Code::ItemIdDuplicate,
        });
    }
    let Some(target) = correction_target(item, token) else {
        found.push(Found {
            line: item.line,
            code: Code::CorrectionInvalid,
        });
        return;
    };
    let known = if target.starts_with("EMD-") {
        number
            .zip(context.emd_numbers.get(target))
            .is_some_and(|(current, defined_in)| *defined_in < current)
    } else {
        context.part_ids.contains(target)
    };
    if !known {
        found.push(Found {
            line: item.line,
            code: Code::CorrectionTargetUnknown,
        });
    }
}

/// O item corrigido por `- EMD-### <alvo>: <novo valor>`; o valor não pode ser vazio.
fn correction_target<'a>(item: &Item<'a>, emd: &str) -> Option<&'a str> {
    item.text[emd.len()..]
        .split_once(':')
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(target, _)| target.trim())
        .filter(|target| {
            ["CMP-", "CA-", "EMD-"]
                .iter()
                .any(|prefix| is_id(target, prefix))
        })
}

fn section_order_violation(doc: &Doc, expected: &[&str]) -> Option<Found> {
    let mismatch = doc
        .sections
        .iter()
        .map(|section| section.title)
        .zip(
            expected
                .iter()
                .copied()
                .map(Some)
                .chain(std::iter::repeat(None)),
        )
        .position(|(title, wanted)| Some(title) != wanted);
    if let Some(index) = mismatch {
        return Some(Found {
            line: doc.sections[index].line,
            code: Code::SectionsInvalid,
        });
    }
    (doc.sections.len() < expected.len()).then_some(Found {
        line: 1,
        code: Code::SectionsInvalid,
    })
}

fn check_shared(doc: &Doc, expected: &[&str], config: &SpecsConfig) -> Vec<Found> {
    let mut found = Vec::new();
    if let Some(violation) = section_order_violation(doc, expected) {
        found.push(violation);
    }
    for section in &doc.sections {
        if section.is_blank() {
            found.push(Found {
                line: section.line,
                code: Code::SectionEmpty,
            });
        }
    }
    let max_lines = config.max_lines as usize;
    if doc.lines.len() > max_lines {
        found.push(Found {
            line: max_lines + 1,
            code: Code::FileTooLong,
        });
    }
    for (index, line) in doc.lines.iter().enumerate() {
        let has_placeholder = config
            .placeholders
            .iter()
            .any(|placeholder| !placeholder.is_empty() && line.contains(placeholder.as_str()));
        if has_placeholder {
            found.push(Found {
                line: index + 1,
                code: Code::PlaceholderFound,
            });
        }
        if has_invalid_pending_marker(line, doc.vocab) {
            found.push(Found {
                line: index + 1,
                code: Code::PendingMarkerInvalid,
            });
        }
    }

    found
}

fn first_token(text: &str) -> &str {
    text.split([' ', ':']).next().unwrap_or("")
}

fn item_id<'a>(item: &Item<'a>, prefix: &str) -> Option<&'a str> {
    Some(first_token(item.text)).filter(|token| is_id(token, prefix))
}

fn is_id(token: &str, prefix: &str) -> bool {
    token
        .strip_prefix(prefix)
        .is_some_and(|digits| is_digits(digits, 3))
}

fn is_digits(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) fn is_amendment_id(id: &str) -> bool {
    strip_amendment_prefix(id).is_some_and(|number| is_digits(number, 2))
}

pub(crate) fn amendment_number(name: &str) -> Option<u32> {
    let digits = strip_amendment_prefix(name)?.strip_suffix(FILE_SUFFIX)?;

    if !is_digits(digits, 2) {
        return None;
    }

    digits.parse().ok()
}

fn defined_emds<'a>(docs: &[Doc<'a>]) -> HashMap<&'a str, u32> {
    let mut defined = HashMap::new();
    for doc in docs {
        let Some(number) = amendment_number(doc.name) else {
            continue;
        };
        for item in &doc.correction_items() {
            if let Some(token) = item_id(item, "EMD-") {
                defined.entry(token).or_insert(number);
            }
        }
    }

    defined
}

fn name_len(text: &str) -> usize {
    let len = text
        .bytes()
        .take_while(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
        .count();

    if text.starts_with('-') { 0 } else { len }
}

/// Um `.` sem segmento válido depois é pontuação da frase e fica fora.
fn semantic_id_len(text: &str) -> usize {
    let mut len = name_len(text);
    if len == 0 {
        return 0;
    }
    while let Some(rest) = text[len..].strip_prefix('.') {
        let segment = name_len(rest);
        if segment == 0 {
            break;
        }
        len += 1 + segment;
    }

    len
}

fn id_prefix(namespace: &str) -> String {
    format!("{namespace}.")
}

fn divergence_ref_prefix() -> String {
    id_prefix(DIVERGENCE_NAMESPACE)
}

fn pending_prefix(vocab: Vocab) -> String {
    format!("{}: {}", vocab.pending_marker(), id_prefix(GAP_NAMESPACE))
}

/// Marcador do outro vocabulário (bem formado ou não) ou marcador malformado do próprio.
fn has_invalid_pending_marker(line: &str, vocab: Vocab) -> bool {
    let pending_prefix = pending_prefix(vocab);

    line.contains(vocab.other().pending_marker())
        || line
            .match_indices(vocab.pending_marker())
            .any(|(start, _)| {
                let valid = line[start..]
                    .strip_prefix(pending_prefix.as_str())
                    .is_some_and(|rest| {
                        let len = semantic_id_len(rest);

                        len > 0 && rest[len..].starts_with(']')
                    });

                !valid
            })
}

fn count_divergence_refs(line: &str) -> usize {
    let prefix = divergence_ref_prefix();

    line.match_indices(prefix.as_str())
        .filter(|(start, _)| semantic_id_len(&line[start + prefix.len()..]) > 0)
        .count()
}

pub trait SpecFacts {
    fn exists(&self, id: &str) -> bool;
    fn confirmed(&self, id: &str) -> bool;
    /// `business` ou `technical` (premissa).
    fn kind(&self, id: &str) -> Option<String>;
    fn late(&self, id: &str) -> bool;
    fn work_item(&self, id: &str) -> Option<String>;
    fn gap_state(&self, gap: &str) -> GapState;
    fn divergence(&self, id: &str) -> Option<DivergenceFact>;
    fn amendment_approved(&self, work_item: &str, amendment: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivergenceFact {
    pub work_item: String,
    pub resolution: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmendmentState {
    Approved,
    Pending,
}

pub fn amendment_state(
    work_item: &str,
    amendment_file: &str,
    facts: &impl SpecFacts,
) -> AmendmentState {
    let name = amendment_file
        .strip_suffix(FILE_SUFFIX)
        .unwrap_or(amendment_file);

    if facts.amendment_approved(work_item, name) {
        AmendmentState::Approved
    } else {
        AmendmentState::Pending
    }
}

pub fn validate_graph(
    work_item: &str,
    parts: &[SpecFile<'_>],
    amendments: &[SpecFile<'_>],
    config: &SpecsConfig,
    facts: &impl SpecFacts,
) -> Vec<Violation> {
    let part_docs: Vec<Doc> = parts.iter().map(Doc::new).collect();
    let mut part_found: Vec<Vec<Found>> = part_docs
        .iter()
        .map(|doc| check_part_graph(doc, facts))
        .collect();
    let cited_business: Vec<&str> = part_docs
        .iter()
        .flat_map(|doc| cited_business_premises(doc, work_item, facts))
        .collect();
    let premise_missing = if cited_business.is_empty() {
        Some(Code::BusinessPremiseMissing)
    } else if cited_business.iter().all(|id| facts.late(id)) {
        Some(Code::MainBusinessPremiseMissing)
    } else {
        None
    };
    if config.cite_business_premise
        && let Some(code) = premise_missing
        && let (Some(doc), Some(found)) = (part_docs.first(), part_found.first_mut())
    {
        found.push(Found {
            line: doc.directives_section().map_or(1, |section| section.line),
            code,
        });
    }

    let mut violations = Vec::new();
    for (doc, found) in part_docs.into_iter().zip(part_found) {
        violations.extend(doc.into_violations(found));
    }
    for file in amendments {
        let doc = Doc::new(file);
        let found = check_amendment_graph(&doc, work_item, facts);
        violations.extend(doc.into_violations(found));
    }

    violations
}

/// quando a citação já é `divergence-ref-invalid` da validação estrutural.
pub fn amendment_divergence_mismatch(file: &SpecFile<'_>, expected: &str) -> Option<Violation> {
    let doc = Doc::new(file);
    let refs = prefixed_ids(doc.divergence_section()?, &divergence_ref_prefix());

    match refs[..] {
        [(line, id)] if id != expected => Some(Violation {
            path: format!("{}:{line}", file.name),
            code: "amendment-divergence-mismatch",
        }),
        _ => None,
    }
}

fn check_part_graph(doc: &Doc, facts: &impl SpecFacts) -> Vec<Found> {
    let mut found = check_pending_gaps(doc, facts);
    for (line, id) in cited_ids(doc) {
        let code = if !facts.exists(id) {
            Code::CitedIdUnknown
        } else if !facts.confirmed(id) {
            Code::CitedIdUnconfirmed
        } else {
            continue;
        };
        found.push(Found { line, code });
    }

    found
}

fn check_amendment_graph(doc: &Doc, work_item: &str, facts: &impl SpecFacts) -> Vec<Found> {
    let mut found = check_pending_gaps(doc, facts);
    let Some(section) = doc.divergence_section() else {
        return found;
    };
    let refs = prefixed_ids(section, &divergence_ref_prefix());
    // Zero ou várias referências já são `divergence-ref-invalid`, da validação estrutural.
    if let [(line, id)] = refs[..]
        && let Some(code) = divergence_violation(doc, id, work_item, facts)
    {
        found.push(Found { line, code });
    }

    found
}

fn divergence_violation(
    doc: &Doc,
    id: &str,
    work_item: &str,
    facts: &impl SpecFacts,
) -> Option<Code> {
    let Some(divergence) = facts.divergence(id) else {
        return Some(Code::AmendmentDivergenceUnknown);
    };
    if divergence.work_item != work_item {
        return Some(Code::AmendmentDivergenceForeign);
    }
    if CLOSED_RESOLUTIONS.contains(&divergence.resolution.as_str()) {
        return Some(Code::AmendmentDivergenceClosed);
    }
    let source = divergence.source_ref.as_str();
    let is_spec_item = is_id(source, "CMP-") || is_id(source, "CA-");

    (is_spec_item && !corrects(doc, source)).then_some(Code::AmendmentCorrectionMissing)
}

fn corrects(doc: &Doc, item_ref: &str) -> bool {
    doc.correction_items()
        .iter()
        .filter_map(|item| item_id(item, "EMD-").map(|emd| correction_target(item, emd)))
        .any(|target| target == Some(item_ref))
}

fn check_pending_gaps(doc: &Doc, facts: &impl SpecFacts) -> Vec<Found> {
    doc.lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            pending_gaps(line)
                .into_iter()
                .any(|gap| matches!(facts.gap_state(gap), GapState::Open | GapState::NeedsHuman))
        })
        .map(|(index, _)| Found {
            line: index + 1,
            code: Code::PendingGapUnresolved,
        })
        .collect()
}

/// Os ids `gaps.<nome>` dos marcadores bem formados da linha.
fn pending_gaps(line: &str) -> Vec<&str> {
    let gap_prefix_len = id_prefix(GAP_NAMESPACE).len();

    Vocab::ALL
        .into_iter()
        .flat_map(|vocab| {
            let pending_prefix = pending_prefix(vocab);

            line.match_indices(pending_prefix.as_str())
                .filter_map(|(start, _)| {
                    let name_start = start + pending_prefix.len();
                    let len = semantic_id_len(&line[name_start..]);
                    let id_start = name_start - gap_prefix_len;

                    (len > 0 && line[name_start + len..].starts_with(']'))
                        .then(|| &line[id_start..name_start + len])
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn cited_ids<'a>(doc: &Doc<'a>) -> Vec<(usize, &'a str)> {
    let Some(section) = doc.directives_section() else {
        return Vec::new();
    };

    CITED_NAMESPACES
        .iter()
        .flat_map(|namespace| prefixed_ids(section, &id_prefix(namespace)))
        .collect()
}

fn prefixed_ids<'a>(section: &Section<'a>, prefix: &str) -> Vec<(usize, &'a str)> {
    let mut ids = Vec::new();
    for (offset, line) in section.body.iter().copied().enumerate() {
        for (start, _) in line.match_indices(prefix) {
            let len = semantic_id_len(&line[start + prefix.len()..]);
            if len > 0 {
                ids.push((
                    section.line + 1 + offset,
                    &line[start..start + prefix.len() + len],
                ));
            }
        }
    }

    ids
}

fn cited_business_premises<'a>(
    doc: &Doc<'a>,
    work_item: &str,
    facts: &impl SpecFacts,
) -> Vec<&'a str> {
    cited_ids(doc)
        .into_iter()
        .map(|(_, id)| id)
        .filter(|id| {
            facts.confirmed(id)
                && facts.kind(id).as_deref() == Some(BUSINESS_KIND)
                && facts.work_item(id).as_deref() == Some(work_item)
        })
        .collect()
}

#[cfg(test)]
mod tests;
