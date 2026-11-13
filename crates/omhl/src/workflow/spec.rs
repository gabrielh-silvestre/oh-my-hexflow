//! Validador estrutural das partes (`parte-NN.md`) e das emendas (`emenda-NN.md`) de uma spec.
//! Puro: recebe o texto dos arquivos e a configuração, devolve violações com `arquivo:linha`.

use std::collections::{HashMap, HashSet};

use super::config::SpecsConfig;
use super::gap::GapState;

const PART_SECTIONS: [&str; 5] = [
    "Objetivo",
    DIRECTIVES,
    "Comportamentos",
    "Critérios de aceitação",
    "Fora de escopo",
];
const AMENDMENT_SECTIONS: [&str; 3] = ["Divergência", "Correções", "Motivo"];
const BEHAVIORS: &str = "Comportamentos";
const CRITERIA: &str = "Critérios de aceitação";
const DIVERGENCE: &str = "Divergência";
const CORRECTIONS: &str = "Correções";
const HEADING_PREFIX: &str = "## ";
const ITEM_PREFIX: &str = "- ";
const PENDING_MARKER: &str = "[PENDENTE";
const GAP_PREFIX: &str = "gaps.";
const PENDING_PREFIX: &str = "[PENDENTE: gaps.";
const DIVERGENCE_REF_PREFIX: &str = "divergences.";
const AMENDMENT_FILE_PREFIX: &str = "emenda-";
const FILE_SUFFIX: &str = ".md";
const DIRECTIVES: &str = "Diretivas e premissas aplicáveis";
const CITED_PREFIXES: [&str; 2] = ["premises.", "guidelines."];
const BUSINESS_KIND: &str = "business";
const CLOSED_RESOLUTIONS: [&str; 2] = ["code-fixed", "accepted"];

#[derive(Debug, Clone, Copy)]
pub struct SpecFile<'a> {
    /// `parte-NN.md` ou `emenda-NN.md`.
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

struct Item<'a> {
    line: usize,
    section: &'a str,
    /// O que vem depois de `- `.
    text: &'a str,
}

struct Doc<'a> {
    name: &'a str,
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
            lines,
            sections,
        }
    }

    fn section(&self, title: &str) -> Option<&Section<'a>> {
        self.sections.iter().find(|section| section.title == title)
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

/// As partes e as emendas chegam já ordenadas pelo nome; a saída segue essa ordem (partes primeiro) e,
/// dentro de um arquivo, a linha crescente.
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
    let mut found = check_shared(doc, &PART_SECTIONS, config);
    for item in doc.items() {
        let prefix = match item.section {
            BEHAVIORS => "CMP-",
            CRITERIA => "CA-",
            _ => continue,
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
    /// `CMP-###` e `CA-###` definidos nas partes.
    part_ids: HashSet<&'a str>,
    /// `EMD-###` -> número da primeira emenda que o define.
    emd_numbers: HashMap<&'a str, u32>,
    seen_emds: HashSet<&'a str>,
    seen_numbers: HashSet<u32>,
}

fn check_amendment<'a>(
    doc: &Doc<'a>,
    config: &SpecsConfig,
    context: &mut AmendmentContext<'a>,
) -> Vec<Found> {
    let mut found = check_shared(doc, &AMENDMENT_SECTIONS, config);
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
    if let Some(section) = doc.section(DIVERGENCE) {
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
        } else if item.section == CORRECTIONS {
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

/// Seção fora de ordem ou a mais na linha dela; seção faltando no fim, na linha 1.
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

/// Regras comuns a parte e emenda: seções, tamanho, placeholders e marcador `[PENDENTE`.
fn check_shared(doc: &Doc, expected: &[&str], config: &SpecsConfig) -> Vec<Found> {
    let mut found = Vec::new();
    if let Some(violation) = section_order_violation(doc, expected) {
        found.push(violation);
    }
    for section in &doc.sections {
        if section.body.iter().all(|line| line.trim().is_empty()) {
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
        if has_malformed_pending_marker(line) {
            found.push(Found {
                line: index + 1,
                code: Code::PendingMarkerInvalid,
            });
        }
    }

    found
}

/// Primeiro token do item: termina em espaço, `:` ou no fim da linha.
fn first_token(text: &str) -> &str {
    text.split([' ', ':']).next().unwrap_or("")
}

/// Primeiro token do item, quando é um id bem formado com o prefixo dado.
fn item_id<'a>(item: &Item<'a>, prefix: &str) -> Option<&'a str> {
    Some(first_token(item.text)).filter(|token| is_id(token, prefix))
}

fn is_id(token: &str, prefix: &str) -> bool {
    token
        .strip_prefix(prefix)
        .is_some_and(|digits| digits.len() == 3 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

/// `emenda-NN.md` com exatamente dois dígitos.
fn amendment_number(name: &str) -> Option<u32> {
    let digits = name
        .strip_prefix(AMENDMENT_FILE_PREFIX)?
        .strip_suffix(FILE_SUFFIX)?;
    if digits.len() != 2 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    digits.parse().ok()
}

/// `EMD-###` -> número da primeira emenda, na ordem recebida, que o define.
fn defined_emds<'a>(docs: &[Doc<'a>]) -> HashMap<&'a str, u32> {
    let mut defined = HashMap::new();
    for doc in docs {
        let Some(number) = amendment_number(doc.name) else {
            continue;
        };
        for item in doc
            .items()
            .iter()
            .filter(|item| item.section == CORRECTIONS)
        {
            if let Some(token) = item_id(item, "EMD-") {
                defined.entry(token).or_insert(number);
            }
        }
    }

    defined
}

/// Nome de gap: `[a-z0-9][a-z0-9-]*`.
fn gap_name_len(text: &str) -> usize {
    let len = text
        .bytes()
        .take_while(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
        .count();

    if text.starts_with('-') { 0 } else { len }
}

fn has_malformed_pending_marker(line: &str) -> bool {
    line.match_indices(PENDING_MARKER).any(|(start, _)| {
        let valid = line[start..]
            .strip_prefix(PENDING_PREFIX)
            .is_some_and(|rest| {
                let len = gap_name_len(rest);

                len > 0 && rest[len..].starts_with(']')
            });

        !valid
    })
}

fn count_divergence_refs(line: &str) -> usize {
    line.match_indices(DIVERGENCE_REF_PREFIX)
        .filter(|(start, _)| gap_name_len(&line[start + DIVERGENCE_REF_PREFIX.len()..]) > 0)
        .count()
}

/// Fatos do grafo de que as regras de `validate_graph` precisam; o app os monta do banco.
pub trait SpecFacts {
    fn exists(&self, id: &str) -> bool;
    /// Premissa: `confirmed` com `confirmed_by`. Diretiva: `confirmed_by`, de extension ou nativa.
    fn confirmed(&self, id: &str) -> bool;
    /// `business` ou `technical` (premissa).
    fn kind(&self, id: &str) -> Option<String>;
    fn work_item(&self, id: &str) -> Option<String>;
    /// Gap inexistente vale `Open`.
    fn gap_state(&self, gap: &str) -> GapState;
    fn divergence(&self, id: &str) -> Option<DivergenceFact>;
    /// Há `divergence` do work item em `spec-stale` com `amendment` igual a `amendment` (sem `.md`).
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

/// Só as regras que dependem do grafo; as estruturais ficam em `validate_spec`. A ordem é a de
/// `validate_spec`: arquivos como recebidos (partes, depois emendas) e, em cada um, a linha crescente.
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
    let cites_business = part_docs
        .iter()
        .any(|doc| cites_business_premise(doc, work_item, facts));
    if config.cite_business_premise
        && !cites_business
        && let (Some(doc), Some(found)) = (part_docs.first(), part_found.first_mut())
    {
        found.push(Found {
            line: doc.section(DIRECTIVES).map_or(1, |section| section.line),
            code: Code::BusinessPremiseMissing,
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
    let Some(section) = doc.section(DIVERGENCE) else {
        return found;
    };
    let refs = prefixed_ids(section, DIVERGENCE_REF_PREFIX);
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

/// Algum `EMD` da seção Correções da emenda corrige o item.
fn corrects(doc: &Doc, item_ref: &str) -> bool {
    doc.items()
        .iter()
        .filter(|item| item.section == CORRECTIONS)
        .filter_map(|item| item_id(item, "EMD-").map(|emd| correction_target(item, emd)))
        .any(|target| target == Some(item_ref))
}

/// Um `[PENDENTE: gaps.x]` válido por linha; o malformado é da validação estrutural.
fn check_pending_gaps(doc: &Doc, facts: &impl SpecFacts) -> Vec<Found> {
    doc.lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            pending_gaps(line)
                .any(|gap| matches!(facts.gap_state(gap), GapState::Open | GapState::NeedsHuman))
        })
        .map(|(index, _)| Found {
            line: index + 1,
            code: Code::PendingGapUnresolved,
        })
        .collect()
}

/// Os ids `gaps.<nome>` dos marcadores bem formados da linha.
fn pending_gaps(line: &str) -> impl Iterator<Item = &str> {
    line.match_indices(PENDING_PREFIX).filter_map(|(start, _)| {
        let name_start = start + PENDING_PREFIX.len();
        let len = gap_name_len(&line[name_start..]);
        let id_start = name_start - GAP_PREFIX.len();

        (len > 0 && line[name_start + len..].starts_with(']'))
            .then(|| &line[id_start..name_start + len])
    })
}

/// `(linha, id)` de cada `premises.x` e `guidelines.x` da seção de diretivas e premissas.
fn cited_ids<'a>(doc: &Doc<'a>) -> Vec<(usize, &'a str)> {
    let Some(section) = doc.section(DIRECTIVES) else {
        return Vec::new();
    };

    CITED_PREFIXES
        .iter()
        .flat_map(|prefix| prefixed_ids(section, prefix))
        .collect()
}

/// `(linha, id)` de cada `<prefixo><nome>` no corpo da seção.
fn prefixed_ids<'a>(section: &Section<'a>, prefix: &str) -> Vec<(usize, &'a str)> {
    let mut ids = Vec::new();
    for (offset, line) in section.body.iter().copied().enumerate() {
        for (start, _) in line.match_indices(prefix) {
            let len = gap_name_len(&line[start + prefix.len()..]);
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

fn cites_business_premise(doc: &Doc, work_item: &str, facts: &impl SpecFacts) -> bool {
    cited_ids(doc).into_iter().any(|(_, id)| {
        facts.confirmed(id)
            && facts.kind(id).as_deref() == Some(BUSINESS_KIND)
            && facts.work_item(id).as_deref() == Some(work_item)
    })
}
