//! Validador estrutural das partes (`parte-NN.md`) e das emendas (`emenda-NN.md`) de uma spec.
//! Puro: recebe o texto dos arquivos e a configuração, devolve violações com `arquivo:linha`.

use std::collections::{HashMap, HashSet};

use super::config::SpecsConfig;

const PART_SECTIONS: [&str; 5] = [
    "Objetivo",
    "Diretivas e premissas aplicáveis",
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
const PENDING_PREFIX: &str = "[PENDENTE: gaps.";
const DIVERGENCE_REF_PREFIX: &str = "divergences.";
const AMENDMENT_FILE_PREFIX: &str = "emenda-";
const FILE_SUFFIX: &str = ".md";

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
    let target = item.text[token.len()..]
        .split_once(':')
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(target, _)| target.trim())
        .filter(|target| {
            ["CMP-", "CA-", "EMD-"]
                .iter()
                .any(|prefix| is_id(target, prefix))
        });
    let Some(target) = target else {
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
