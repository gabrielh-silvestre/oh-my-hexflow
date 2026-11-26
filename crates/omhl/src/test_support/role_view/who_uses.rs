//! A regra 8: a tabela "quem usa" de cada página migrada contra o que os YAML dos papéis e as skills de workflow
//! projetam para a folha, e a classe de cada página contra a `WHO_USES_ANCHOR`, escrita à mão.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

use crate::workflow::omhl_leaf::Group;

use super::{
    CITATION_PATTERN, SESSION_ROLE, cli_groups, collect_findings, command_leaf, finding,
    parse_view, skill_body, view_commands,
};

pub const WHO_USES_HEADING: &str = "## Quem usa, em qual etapa e com qual filtro";
pub const NO_STAGE_SENTENCE: &str = "Nenhuma etapa do ciclo a roda.";
const REFERENCE_TABLE_HEADING: &str = "## Comando para referência";
const RULES_LABEL: &str = "regras";
const STAGE_SEPARATOR: &str = ", ";
const TABLE_HEADER_ROLE: &str = "Papel";

/// Cópia de `tests/e2e/skills/workflow.rs`: as folhas da base, que não levam o bloco "quem usa".
pub const BASE_LEAVES: [&str; 5] = [
    "append",
    "spec init",
    "spec validate",
    "gate-worktree",
    "guideline retire",
];

/// O que a projeção de uma folha diz sobre quem a roda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhoUsesClass {
    /// Nenhuma linha: nem papel nem sessão a roda.
    NoRole,
    /// Ao menos uma linha `sessao`.
    Session,
    /// Só linhas de papel.
    RolesOnly,
}

/// Escrita à mão a partir da tabela de condensação do plano: perder um prefixo `omhl <folha>` numa skill de
/// workflow muda a classe da página e reprova, até a âncora ser editada no mesmo diff.
pub const WHO_USES_ANCHOR: [(&str, WhoUsesClass); 20] = [
    ("render", WhoUsesClass::NoRole),
    ("verify", WhoUsesClass::NoRole),
    ("hooks post-commit", WhoUsesClass::NoRole),
    ("check sweep", WhoUsesClass::NoRole),
    ("report sessions", WhoUsesClass::NoRole),
    ("list", WhoUsesClass::Session),
    ("get", WhoUsesClass::Session),
    ("report answers", WhoUsesClass::Session),
    ("project", WhoUsesClass::Session),
    ("load", WhoUsesClass::Session),
    ("attest", WhoUsesClass::Session),
    ("byproduct", WhoUsesClass::Session),
    ("byproduct prune", WhoUsesClass::Session),
    ("check retire", WhoUsesClass::Session),
    ("hooks install", WhoUsesClass::Session),
    ("check run", WhoUsesClass::Session),
    ("gate-pr", WhoUsesClass::Session),
    ("close", WhoUsesClass::Session),
    ("report retro", WhoUsesClass::Session),
    ("report recurrences", WhoUsesClass::RolesOnly),
];

/// Uma linha da tabela: papel, etapas e o comando inteiro, sem as crases.
type Row = (String, String, String);

/// Onde uma citação mora: a skill na ordem dada, o passo (as regras por último) e o rótulo da coluna.
type Stage = (usize, u32, String);

fn row_text((role, stages, command): &Row) -> String {
    format!("{role} | {stages} | {command}")
}

fn joined(items: impl Iterator<Item = String>) -> String {
    items.collect::<Vec<_>>().join(STAGE_SEPARATOR)
}

/// Os comandos dos papéis que a folha recebe, com as etapas em que cada papel os roda (`repete` entra na lista).
fn role_rows(
    leaf: &str,
    views: &[(&str, &str)],
    groups: &[Group],
) -> Result<BTreeSet<Row>, String> {
    let mut stages: BTreeMap<(String, String), BTreeSet<u32>> = BTreeMap::new();
    for (role, text) in views {
        let view = parse_view(text).map_err(|error| format!("{role}: {error}"))?;
        for step in &view.etapas {
            let source = step
                .repete
                .and_then(|target| view.etapas.iter().find(|other| other.etapa == target))
                .unwrap_or(step);
            for command in view_commands(source) {
                if command_leaf(command, groups).as_deref() == Some(leaf) {
                    stages
                        .entry(((*role).to_owned(), command.clone()))
                        .or_default()
                        .insert(step.etapa);
                }
            }
        }
    }

    Ok(stages
        .into_iter()
        .map(|((role, command), numbers)| {
            (role, joined(numbers.iter().map(u32::to_string)), command)
        })
        .collect())
}

/// O trecho de cada `omhl <folha>` citado na linha: da palavra `omhl` até a crase que fecha (ou o fim da linha).
fn cited_commands<'a>(line: &'a str, citation: &Regex) -> Vec<&'a str> {
    citation
        .find_iter(line)
        .filter_map(|found| {
            let start = found.start() + found.as_str().find("omhl")?;
            let rest = &line[start..];
            let end = if found.as_str().starts_with('`') {
                rest.find('`').unwrap_or(rest.len())
            } else {
                rest.len()
            };

            Some(rest[..end].trim_end())
        })
        .collect()
}

/// As citações da folha no corpo das skills de workflow (na ordem dada), fora das cercas e da tabela
/// "Comando para referência". Toda linha herda o passo numerado anterior até o próximo título.
fn session_rows(leaf: &str, workflow: &[(&str, &str)], groups: &[Group]) -> BTreeSet<Row> {
    let step_line = Regex::new(r"^(\d+)\. ").expect("regex");
    let citation = Regex::new(CITATION_PATTERN).expect("regex");
    let mut stages: BTreeMap<String, BTreeSet<Stage>> = BTreeMap::new();
    for (order, (skill, text)) in workflow.iter().enumerate() {
        let mut step: Option<u32> = None;
        let mut inside_fence = false;
        let mut inside_reference_table = false;
        for line in skill_body(text).lines() {
            if line.starts_with("```") {
                inside_fence = !inside_fence;
                continue;
            }
            if inside_fence {
                continue;
            }
            if line.starts_with('#') {
                step = None;
                inside_reference_table = line.starts_with(REFERENCE_TABLE_HEADING);
            }
            if let Some(captures) = step_line.captures(line) {
                step = captures[1].parse().ok();
            }
            if inside_reference_table {
                continue;
            }
            for command in cited_commands(line, &citation) {
                if command_leaf(command, groups).as_deref() != Some(leaf) {
                    continue;
                }
                let stage = match step {
                    Some(number) => (order, number, format!("{skill} {number}")),
                    None => (order, u32::MAX, format!("{skill} {RULES_LABEL}")),
                };
                stages.entry(command.to_owned()).or_default().insert(stage);
            }
        }
    }

    stages
        .into_iter()
        .map(|(command, places)| {
            let labels = places.into_iter().map(|(_, _, label)| label);

            (SESSION_ROLE.to_owned(), joined(labels), command)
        })
        .collect()
}

fn projection(
    leaf: &str,
    views: &[(&str, &str)],
    workflow: &[(&str, &str)],
) -> Result<BTreeSet<Row>, String> {
    let groups = cli_groups();
    let mut rows = role_rows(leaf, views, &groups)?;
    rows.extend(session_rows(leaf, workflow, &groups));

    Ok(rows)
}

fn class_of(rows: &BTreeSet<Row>) -> WhoUsesClass {
    if rows.is_empty() {
        WhoUsesClass::NoRole
    } else if rows.iter().any(|(role, ..)| role == SESSION_ROLE) {
        WhoUsesClass::Session
    } else {
        WhoUsesClass::RolesOnly
    }
}

fn page_row(line: &str) -> Option<Row> {
    let cells: Vec<&str> = line
        .trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect();

    match cells.as_slice() {
        [role, stages, command] => Some((
            (*role).to_owned(),
            (*stages).to_owned(),
            command.trim_matches('`').to_owned(),
        )),
        _ => None,
    }
}

/// As linhas da tabela da seção "quem usa" da página e se ela diz a frase da página vazia.
fn page_block(page: &str) -> Result<(BTreeSet<Row>, bool), String> {
    if !page.lines().any(|line| line.trim_end() == WHO_USES_HEADING) {
        return Err(format!("sem a seção \"{WHO_USES_HEADING}\""));
    }
    let section: Vec<&str> = page
        .lines()
        .skip_while(|line| line.trim_end() != WHO_USES_HEADING)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect();
    let mut rows = BTreeSet::new();
    for line in section
        .iter()
        .filter(|line| line.trim_start().starts_with('|'))
    {
        let row = page_row(line).ok_or_else(|| format!("linha malformada: {line}"))?;
        if row.0 != TABLE_HEADER_ROLE && !row.0.starts_with('-') {
            rows.insert(row);
        }
    }
    let says_empty = section.iter().any(|line| line.trim() == NO_STAGE_SENTENCE);

    Ok((rows, says_empty))
}

/// A regra 8 sobre uma página: as linhas dela contra a projeção da folha, nos dois sentidos.
pub fn who_uses(
    leaf: &str,
    page: &str,
    views: &[(&str, &str)],
    workflow: &[(&str, &str)],
) -> Vec<String> {
    let label = format!("quem usa da página {leaf}");
    let projected = match projection(leaf, views, workflow) {
        Ok(rows) => rows,
        Err(error) => return vec![format!("{label}: [{error}]")],
    };
    let (listed, says_empty) = match page_block(page) {
        Ok(block) => block,
        Err(error) => return vec![format!("{label}: [{error}]")],
    };
    let surplus_sentence =
        (says_empty && !projected.is_empty()).then(|| NO_STAGE_SENTENCE.to_owned());
    let missing_sentence =
        (!says_empty && projected.is_empty()).then(|| NO_STAGE_SENTENCE.to_owned());
    let surplus = listed
        .difference(&projected)
        .map(row_text)
        .chain(surplus_sentence)
        .map(|text| format!("sobra: {text}"));
    let missing = projected
        .difference(&listed)
        .map(row_text)
        .chain(missing_sentence)
        .map(|text| format!("falta: {text}"));

    finding(&label, surplus.chain(missing))
        .into_iter()
        .collect()
}

/// A classe de cada folha migrada contra a âncora, e a âncora contra as folhas da CLI menos a base.
pub fn anchor_findings(
    leaves: &[String],
    base_leaves: &[&str],
    anchor: &[(&str, WhoUsesClass)],
    views: &[(&str, &str)],
    workflow: &[(&str, &str)],
) -> Vec<String> {
    let migrated: BTreeSet<&str> = leaves
        .iter()
        .map(String::as_str)
        .filter(|leaf| !base_leaves.contains(leaf))
        .collect();
    let anchored: BTreeSet<&str> = anchor.iter().map(|(leaf, _)| *leaf).collect();
    let divergent: BTreeSet<String> = anchor
        .iter()
        .filter_map(|(leaf, expected)| {
            match projection(leaf, views, workflow).map(|rows| class_of(&rows)) {
                Ok(found) if found == *expected => None,
                Ok(found) => Some(format!(
                    "{leaf}: esperado {expected:?}, projetado {found:?}"
                )),
                Err(error) => Some(format!("{leaf}: {error}")),
            }
        })
        .collect();

    collect_findings([
        finding(
            "folhas fora da âncora",
            migrated
                .difference(&anchored)
                .map(|leaf| (*leaf).to_owned()),
        ),
        finding(
            "âncora sem folha na CLI",
            anchored
                .difference(&migrated)
                .map(|leaf| (*leaf).to_owned()),
        ),
        finding("classe divergente da âncora", divergent),
    ])
}
