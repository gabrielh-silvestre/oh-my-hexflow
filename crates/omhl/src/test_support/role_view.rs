//! A vista de cada papel (`plugin/skills/role-<papel>/SKILL.md`) e o ciclo (`cycle.md`) como dado, conferidos
//! contra o `omhl-allow` do agente, a CLI e a prosa das skills de workflow.

use std::collections::BTreeSet;

use clap::CommandFactory;
use regex::Regex;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::adapters::cli::{Cli, command_groups, command_leaves};
use crate::workflow::omhl_leaf::{Group, Leaf, resolve_leaf};
use crate::workflow::role_allow::{AppendAllow, RoleAllow};

pub const VIEW_TAG: &str = "omhl-role-view";
pub const CYCLE_TAG: &str = "omhl-cycle";

/// Como o `skills:` do agente grafa a skill da vista.
#[expect(
    dead_code,
    reason = "a grafia que não vale hoje fica declarada para quem trocar a constante"
)]
pub enum SkillsSpelling {
    /// `role-<papel>`.
    Bare,
    /// `oh-my-hexflow:role-<papel>`.
    Qualified,
}

/// Por onde a vista chega ao agente.
#[expect(
    dead_code,
    reason = "o canal que não vale hoje fica declarado para quem trocar a constante"
)]
pub enum Channel {
    /// O campo `skills:` do frontmatter do agente.
    SkillsField,
    /// O bloco no corpo do agente (plano B).
    AgentBody,
}

// Medido pelo G1, Claude Code 2.1.327, 2026-11-18; registro no ADR 0011 (PR 3 desta agregadora).
pub const SKILLS_SPELLING: SkillsSpelling = SkillsSpelling::Qualified;
pub const CHANNEL: Channel = Channel::SkillsField;

const PLUGIN_PREFIX: &str = "oh-my-hexflow:";
const OMHL_PREFIX: &str = "omhl ";
const SESSION_ROLE: &str = "sessao";
const SESSION_CHECK: &str = "A sessão confere";
const FRONTMATTER_FENCE: &str = "---";
const ALLOWED_PLACEHOLDERS: [&str; 3] = ["<id>", "<nome>", "<slug>"];
const FORBIDDEN_OPERATORS: [&str; 3] = ["|", "&&", ";"];
const FORM_LABEL: &str = "forma do bloco";
/// O mesmo padrão de `coverage.rs` (`citations`): a folha acaba na primeira opção.
const CITATION_PATTERN: &str = r"(?:^\s*\$ |`)omhl((?: [a-z][a-z0-9-]*)+)";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleView {
    role: String,
    etapas: Vec<ViewStep>,
    nao_usados: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewStep {
    etapa: u32,
    repete: Option<u32>,
    comandos: Option<Vec<String>>,
    grava: Option<AppendAllow>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle {
    etapas: Vec<CycleStep>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CycleStep {
    n: u32,
    nome: String,
    papeis: Vec<String>,
}

/// Só o `skills:` do frontmatter do agente; o `omhl-allow` é do `role_allow`.
#[derive(Debug, Deserialize)]
struct AgentFrontmatter {
    skills: Option<Vec<String>>,
}

/// Cópia de `tests/support/scan.rs`: prova um verificador contra entradas que ele deve e que não deve acusar.
pub fn prove_both_sides(
    count: impl Fn(&str) -> Result<usize, String>,
    flagged: &[&str],
    accepted: &[&str],
) -> Vec<String> {
    let mut failures = Vec::new();
    for snippet in flagged {
        match count(snippet) {
            Ok(0) => failures.push(format!("embedded snippet was not flagged: {snippet}")),
            Ok(_) => {}
            Err(error) => {
                failures.push(format!("embedded snippet did not lex ({error}): {snippet}"))
            }
        }
    }
    for snippet in accepted {
        match count(snippet) {
            Ok(0) => {}
            Ok(_) => failures.push(format!("embedded snippet was flagged: {snippet}")),
            Err(error) => {
                failures.push(format!("embedded snippet did not lex ({error}): {snippet}"))
            }
        }
    }

    failures
}

/// O corpo de cada cerca ```` ```yaml <tag> ````, na ordem em que aparecem.
pub fn extract_block(text: &str, tag: &str) -> Vec<String> {
    let opening = format!("```yaml {tag}");
    let mut blocks = Vec::new();
    let mut inside = false;
    let mut body = String::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if !inside {
            inside = trimmed == opening;
        } else if trimmed == "```" {
            blocks.push(std::mem::take(&mut body));
            inside = false;
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }

    blocks
}

fn parse_yaml<T: DeserializeOwned>(yaml: &str) -> Result<T, String> {
    // `no_schema`, como em `role_allow.rs`: um item como `1` ou `true` não vira texto sozinho.
    let options = serde_saphyr::options! { no_schema: true };

    serde_saphyr::from_str_with_options(yaml, options).map_err(|error| error.to_string())
}

fn single_block(text: &str, tag: &str) -> Result<String, String> {
    let mut blocks = extract_block(text, tag);
    if blocks.len() != 1 {
        return Err(format!(
            "cercas {tag}: esperada 1, achadas {}",
            blocks.len()
        ));
    }

    Ok(blocks.remove(0))
}

pub fn parse_cycle(cycle_md: &str) -> Result<Cycle, String> {
    // `strict_booleans`: só `true`/`false` são booleanos, então a chave `n` do bloco `omhl-cycle` não pede aspas.
    let options = serde_saphyr::options! { no_schema: true, strict_booleans: true };

    serde_saphyr::from_str_with_options(&single_block(cycle_md, CYCLE_TAG)?, options)
        .map_err(|error| error.to_string())
}

fn parse_view(view_file: &str) -> Result<RoleView, String> {
    parse_yaml(&single_block(view_file, VIEW_TAG)?).map_err(|error| format!("yaml: {error}"))
}

/// O `skills:` do frontmatter do agente (`None` quando ausente). Cópia do recorte de `role_allow.rs`.
pub fn agent_skills(agent_file: &str) -> Result<Option<Vec<String>>, String> {
    let yaml = frontmatter(agent_file).ok_or("the agent file has no frontmatter")?;
    let parsed: AgentFrontmatter = parse_yaml(yaml)?;

    Ok(parsed.skills)
}

/// O texto entre a primeira linha `---` e a seguinte (cópia de `role_allow.rs`).
fn frontmatter(agent_file: &str) -> Option<&str> {
    let after_fence = agent_file.strip_prefix(FRONTMATTER_FENCE)?;
    let body = after_fence
        .strip_prefix("\r\n")
        .or_else(|| after_fence.strip_prefix('\n'))?;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        if line.trim_end() == FRONTMATTER_FENCE {
            return Some(&body[..offset]);
        }
        offset += line.len();
    }

    None
}

/// Os comandos que só existem com subcomando (`spec`, `check`...) e o híbrido `byproduct`.
fn cli_groups() -> Vec<Group> {
    command_groups(&Cli::command())
}

/// Cada folha de ação da CLI (`"check run"`), sem o grupo `hook` e sem o `help` do clap.
fn cli_leaves() -> Vec<String> {
    let mut leaves: Vec<String> = command_leaves(&Cli::command())
        .into_iter()
        .filter(|leaf| leaf.split(' ').next() != Some("hook"))
        .collect();
    leaves.sort();

    leaves
}

/// A folha que o texto depois do `omhl ` pede; `None` quando o hook a negaria ou ela só imprime ajuda.
fn leaf_of(words: &[String], groups: &[Group]) -> Option<String> {
    match resolve_leaf(words, groups) {
        Some(Leaf::Named(name)) => Some(name),
        Some(Leaf::ReadOnly) | None => None,
    }
}

fn command_leaf(command: &str, groups: &[Group]) -> Option<String> {
    let words: Vec<String> = command
        .strip_prefix(OMHL_PREFIX)?
        .split_whitespace()
        .map(str::to_owned)
        .collect();

    leaf_of(&words, groups)
}

/// `rótulo: [a, b]` com os itens já ordenados; `None` sem itens.
fn finding(label: &str, items: impl IntoIterator<Item = String>) -> Option<String> {
    let items: Vec<String> = items.into_iter().collect();
    if items.is_empty() {
        return None;
    }

    Some(format!("{label}: [{}]", items.join(", ")))
}

/// Os achados das regras que acusaram, na ordem dada.
fn collect_findings<const N: usize>(items: [Option<String>; N]) -> Vec<String> {
    items.into_iter().flatten().collect()
}

fn view_commands(step: &ViewStep) -> &[String] {
    step.comandos.as_deref().unwrap_or_default()
}

fn runs_append(step: &ViewStep, groups: &[Group]) -> bool {
    view_commands(step)
        .iter()
        .any(|command| command_leaf(command, groups).as_deref() == Some("append"))
}

/// A regra 0: a forma de cada entrada e de cada comando.
fn form_violations(
    role: &str,
    view: &RoleView,
    allow: &RoleAllow,
    groups: &[Group],
) -> Vec<String> {
    let placeholder = Regex::new(r"<[^<>]*>").expect("regex");
    let mut violations: Vec<(u32, String)> = Vec::new();
    let mut seen = BTreeSet::new();
    if view.role != role {
        violations.push((0, format!("role {} ≠ {role}", view.role)));
    }
    for step in &view.etapas {
        let n = step.etapa;
        let mut push = |text: String| violations.push((n, text));
        if !seen.insert(n) {
            push(format!("etapa {n}: etapa repetida"));
        }
        match (step.repete, &step.comandos) {
            (Some(_), Some(_)) => push(format!("etapa {n}: repete com comandos")),
            (Some(_), None) if step.grava.is_some() => push(format!("etapa {n}: repete com grava")),
            (None, None) => push(format!("etapa {n}: sem repete e sem comandos")),
            _ => {}
        }
        if step.repete == Some(n) {
            push(format!("etapa {n}: repete a própria etapa"));
        } else if let Some(target) = step.repete
            && !view.etapas.iter().any(|other| other.etapa == target)
        {
            push(format!(
                "etapa {n}: repete a etapa {target}, que o papel não tem"
            ));
        }
        for command in view_commands(step) {
            if !command.starts_with(OMHL_PREFIX) {
                push(format!("etapa {n}: comando sem o prefixo omhl: {command}"));
            }
            for found in placeholder.find_iter(command) {
                if !ALLOWED_PLACEHOLDERS.contains(&found.as_str()) {
                    push(format!(
                        "etapa {n}: placeholder {} fora de <id>, <nome> e <slug>",
                        found.as_str()
                    ));
                }
            }
            for operator in FORBIDDEN_OPERATORS {
                if command.contains(operator) {
                    push(format!("etapa {n}: operador {operator} em {command}"));
                }
            }
        }
        if step.comandos.is_some() {
            violations.extend(grava_violation(n, step, allow, groups).map(|text| (n, text)));
        }
    }
    violations.sort();

    violations.into_iter().map(|(_, text)| text).collect()
}

fn grava_violation(n: u32, step: &ViewStep, allow: &RoleAllow, groups: &[Group]) -> Option<String> {
    match (
        step.grava.is_some(),
        runs_append(step, groups),
        allow.append.is_some(),
    ) {
        (true, false, _) => Some(format!("etapa {n}: grava sem omhl append nos comandos")),
        (true, true, false) => Some(format!("etapa {n}: grava, mas o omhl-allow não tem append")),
        (false, true, true) => Some(format!("etapa {n}: omhl append sem grava")),
        _ => None,
    }
}

/// As regras 1 a 5 sobre a vista de `role`; a regra 0 devolve só as suas linhas quando acusa.
pub fn check(
    role: &str,
    view_file: &str,
    allow: &RoleAllow,
    cycle: &Cycle,
    declared_skills: Option<&[String]>,
    skill_dirs: &[&str],
) -> Vec<String> {
    let groups = cli_groups();
    let view = match parse_view(view_file) {
        Ok(view) => view,
        Err(error) => return vec![format!("{FORM_LABEL}: [{error}]")],
    };
    let form = form_violations(role, &view, allow, &groups);
    if !form.is_empty() {
        return form
            .into_iter()
            .map(|violation| format!("{FORM_LABEL}: [{violation}]"))
            .collect();
    }

    [
        stage_findings(role, &view, cycle),
        command_findings(&view, allow, &groups),
        record_findings(&view, allow),
        skill_findings(role, declared_skills, skill_dirs),
    ]
    .concat()
}

/// A regra 1.
fn stage_findings(role: &str, view: &RoleView, cycle: &Cycle) -> Vec<String> {
    let acts_in: BTreeSet<u32> = cycle
        .etapas
        .iter()
        .filter(|step| step.papeis.iter().any(|papel| papel == role))
        .map(|step| step.n)
        .collect();
    let declared: BTreeSet<u32> = view.etapas.iter().map(|step| step.etapa).collect();

    collect_findings([
        finding(
            "etapas sem linha",
            acts_in.difference(&declared).map(u32::to_string),
        ),
        finding(
            "etapas fora do papel",
            declared.difference(&acts_in).map(u32::to_string),
        ),
    ])
}

/// As regras 2 e 3.
fn command_findings(view: &RoleView, allow: &RoleAllow, groups: &[Group]) -> Vec<String> {
    let mut used = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    for command in view.etapas.iter().flat_map(view_commands) {
        match command_leaf(command, groups) {
            Some(leaf) => used.insert(leaf),
            None => unresolved.insert(command.clone()),
        };
    }
    let allowed: BTreeSet<String> = allow.commands.iter().cloned().collect();
    let unused: BTreeSet<String> = view.nao_usados.iter().cloned().collect();
    let outside_allow: BTreeSet<String> = used
        .difference(&allowed)
        .cloned()
        .chain(unresolved)
        .collect();
    let without_line = allowed
        .iter()
        .filter(|leaf| used.contains(*leaf) == unused.contains(*leaf))
        .cloned();

    collect_findings([
        finding("comandos fora do omhl-allow", outside_allow),
        finding(
            "nao_usados fora do omhl-allow",
            unused.difference(&allowed).cloned(),
        ),
        finding("comandos do omhl-allow sem linha", without_line),
    ])
}

/// Os nós, as relações e as retiradas que uma lista `grava` declara, como `nó:decision`.
fn recorded(append: &AppendAllow) -> BTreeSet<String> {
    let tagged = |kind: &str, items: &[String]| -> Vec<String> {
        items.iter().map(|item| format!("{kind}:{item}")).collect()
    };

    [
        tagged("nó", &append.nodes),
        tagged("relação", &append.relations),
        tagged("retirada", &append.retire),
    ]
    .concat()
    .into_iter()
    .collect()
}

/// A regra 4.
fn record_findings(view: &RoleView, allow: &RoleAllow) -> Vec<String> {
    let declared: BTreeSet<String> = view
        .etapas
        .iter()
        .filter_map(|step| step.grava.as_ref())
        .flat_map(recorded)
        .collect();
    let allowed = allow.append.as_ref().map(recorded).unwrap_or_default();

    collect_findings([
        finding("grava sem linha", allowed.difference(&declared).cloned()),
        finding(
            "grava fora do omhl-allow",
            declared.difference(&allowed).cloned(),
        ),
    ])
}

/// A regra 5.
fn skill_findings(
    role: &str,
    declared_skills: Option<&[String]>,
    skill_dirs: &[&str],
) -> Vec<String> {
    let skill = format!("role-{role}");
    let expected = match (&CHANNEL, &SKILLS_SPELLING) {
        (Channel::AgentBody, _) => None,
        (Channel::SkillsField, SkillsSpelling::Bare) => Some(skill),
        (Channel::SkillsField, SkillsSpelling::Qualified) => {
            Some(format!("{PLUGIN_PREFIX}{skill}"))
        }
    };
    let found = declared_skills.unwrap_or_default();
    let mismatch = match (&expected, declared_skills) {
        (None, None) => None,
        (Some(name), Some([only])) if only == name => None,
        _ => Some(format!(
            "esperado {}, achado {}",
            expected.as_deref().unwrap_or("ausente"),
            declared_skills.map_or_else(|| "ausente".to_owned(), |skills| format!("{skills:?}")),
        )),
    };
    let without_file = found.iter().filter(|name| {
        let directory = name.strip_prefix(PLUGIN_PREFIX).unwrap_or(name);
        !skill_dirs.contains(&directory)
    });

    collect_findings([
        finding("skills do agente", mismatch),
        finding("skill do agente sem SKILL.md", without_file.cloned()),
    ])
}

/// `minúsculas, sem acento, espaço → "-"`, sem o ponto final: o título em negrito de uma linha do `cycle.md`.
fn normalize_title(title: &str) -> String {
    title
        .trim()
        .trim_end_matches('.')
        .to_lowercase()
        .chars()
        .map(|character| match character {
            'á' | 'ã' | 'â' | 'à' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'õ' | 'ô' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            ' ' => '-',
            other => other,
        })
        .collect()
}

/// A regra 6: os papéis e o nome de cada etapa do `omhl-cycle` contra a linha `N.` do `cycle.md`.
pub fn cycle_prose(cycle: &Cycle, cycle_md: &str) -> Vec<String> {
    let line = Regex::new(r"^(\d+)\. \*\*([^*]+)\*\*").expect("regex");
    let token = Regex::new(r"oh-my-hexflow:([a-z]+(?:-[a-z]+)*)").expect("regex");
    let mut items = Vec::new();
    for step in &cycle.etapas {
        let n = step.n;
        let Some((text, title)) = cycle_md.lines().find_map(|candidate| {
            let captures = line.captures(candidate)?;
            (captures[1].parse::<u32>() == Ok(n)).then(|| (candidate, captures[2].to_owned()))
        }) else {
            items.push((n, 0, format!("{n}: sem linha numerada")));
            continue;
        };
        let in_prose: BTreeSet<&str> = token
            .captures_iter(text)
            .filter_map(|captures| captures.get(1).map(|found| found.as_str()))
            .collect();
        let in_data: BTreeSet<&str> = step
            .papeis
            .iter()
            .map(String::as_str)
            .filter(|papel| *papel != SESSION_ROLE)
            .collect();
        if in_prose != in_data {
            items.push((
                n,
                1,
                format!("{n}: papeis {} ≠ {}", list_of(&in_prose), list_of(&in_data)),
            ));
        }
        if normalize_title(&title) != step.nome {
            items.push((
                n,
                2,
                format!("{n}: nome {} ≠ {}", normalize_title(&title), step.nome),
            ));
        }
    }
    items.sort();

    finding(
        "papeis do ciclo",
        items.into_iter().map(|(_, _, item)| item),
    )
    .into_iter()
    .collect()
}

fn list_of(roles: &BTreeSet<&str>) -> String {
    format!("[{}]", roles.iter().copied().collect::<Vec<_>>().join(", "))
}

/// O texto depois do frontmatter: a `description` cita comandos e não conta como passo.
fn skill_body(skill_text: &str) -> &str {
    skill_text.splitn(3, "---\n").nth(2).unwrap_or_default()
}

/// A regra 7: todo passo numerado traz a frase da conferência e um `omhl <folha>` que a CLI tem.
pub fn session_checks(skill_text: &str) -> Vec<String> {
    let step = Regex::new(r"^(\d+)\. ").expect("regex");
    let citation = Regex::new(CITATION_PATTERN).expect("regex");
    let groups = cli_groups();
    let leaves = cli_leaves();
    let mut items = Vec::new();
    let mut steps = 0;
    for line in skill_body(skill_text).lines() {
        let Some(captures) = step.captures(line) else {
            continue;
        };
        steps += 1;
        let n = &captures[1];
        if !line.contains(SESSION_CHECK) {
            items.push(format!("{n}: sem a frase \"{SESSION_CHECK}\""));
        }
        let cited: Vec<Vec<String>> = citation
            .captures_iter(line)
            .map(|found| found[1].split_whitespace().map(str::to_owned).collect())
            .collect();
        if cited.is_empty() {
            items.push(format!("{n}: sem comando omhl"));
        }
        for words in &cited {
            match leaf_of(words, &groups) {
                Some(leaf) if leaves.contains(&leaf) => {}
                _ => items.push(format!("{n}: folha inexistente {}", words.join(" "))),
            }
        }
    }

    // Sem passo nenhum (frontmatter ausente ou com CRLF, lista renumerada) a regra passaria em falso.
    if steps == 0 {
        items.push("sem passo numerado após o frontmatter".to_owned());
    }

    finding("passos sem conferência", items)
        .into_iter()
        .collect()
}

mod phrase_markers;
mod who_uses;

#[cfg(test)]
mod tests;
