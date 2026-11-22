use serde_json::Value;

const PREMISE: &str = "premise";
const GUIDELINE: &str = "guideline";
const ANSWERS: &str = "answers";
const SUPPORTS: &str = "supports";
const CONTRADICTS: &str = "contradicts";
const PERSON_PREFIX: &str = "person:";
/// Menor nome de pessoa, em caracteres visíveis, que vale como confirmação humana.
const MIN_VISIBLE_NAME_CHARS: usize = 3;
pub const CRITIC_CONFIRMATION: &str = "critic";

#[derive(Debug, Clone)]
pub struct Fact {
    pub id: String,
    pub type_name: String,
    pub native: bool,
    pub loader: bool,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub relation: String,
    pub from: String,
    pub to: String,
}

/// `edges` em ordem cronológica: com várias respostas, a ordem desempata.
#[derive(Debug, Clone)]
pub struct GapFacts {
    pub nodes: Vec<Fact>,
    pub edges: Vec<Edge>,
}

/// Em ordem de preferência: com várias respostas, vale a de quem vem primeiro aqui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Answerer {
    Human,
    Critic,
    Autonomous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapState {
    Open,
    Answered { by: Answerer, criterion: bool },
    NeedsHuman,
}

pub fn gap_state(facts: &GapFacts, gap: &str) -> GapState {
    let Some(premise) = answering_premise(facts, gap) else {
        return GapState::Open;
    };
    let by = answerer_of(&premise.data);
    if by != Answerer::Human && contradicts_a_confirmed_directive(facts, &premise.id) {
        return GapState::NeedsHuman;
    }

    GapState::Answered {
        by,
        criterion: is_supported_by_a_confirmed_directive(facts, &premise.id),
    }
}

pub fn answered_by(facts: &GapFacts, gap: &str) -> Option<String> {
    answering_premise(facts, gap).map(|premise| premise.id.clone())
}

fn answering_premise<'a>(facts: &'a GapFacts, gap: &str) -> Option<&'a Fact> {
    facts
        .edges
        .iter()
        .filter(|edge| edge.relation == ANSWERS && edge.to == gap)
        .filter_map(|edge| node(facts, &edge.from))
        .filter(|fact| fact.type_name == PREMISE)
        .min_by_key(|fact| answerer_of(&fact.data))
}

fn answerer_of(data: &Value) -> Answerer {
    let by = is_confirmed_premise(data)
        .then(|| confirmer(data))
        .flatten();
    match by {
        Some(by) if is_person_confirmer(by) => Answerer::Human,
        Some(CRITIC_CONFIRMATION) => Answerer::Critic,
        _ => Answerer::Autonomous,
    }
}

/// `person:<nome>` com ao menos 3 caracteres visíveis no nome.
pub fn is_person_confirmer(by: &str) -> bool {
    by.strip_prefix(PERSON_PREFIX).is_some_and(|name| {
        name.chars().filter(|c| is_visible(*c)).count() >= MIN_VISIBLE_NAME_CHARS
    })
}

/// O `is_whitespace` e o `is_control` deixam passar os invisíveis de largura zero, bidi e seletores; a lista os cobre.
fn is_visible(c: char) -> bool {
    !c.is_whitespace()
        && !c.is_control()
        && !matches!(
            c,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{115F}'
                | '\u{1160}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2069}'
                | '\u{2800}'
                | '\u{3164}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{FFA0}'
                | '\u{E0000}'..='\u{E007F}'
        )
}

pub fn confirmer(data: &Value) -> Option<&str> {
    data.get("confirmed_by")?.get("by")?.as_str()
}

fn node<'a>(facts: &'a GapFacts, id: &str) -> Option<&'a Fact> {
    facts.nodes.iter().find(|fact| fact.id == id)
}

pub fn is_confirmed_premise(data: &Value) -> bool {
    data.get("status").and_then(Value::as_str) == Some("confirmed")
        && data.get("confirmed_by").is_some()
}

pub fn is_confirmed_guideline(native: bool, loader: bool, data: &Value) -> bool {
    native || loader || data.get("confirmed_by").is_some()
}

fn is_confirmed_directive(fact: &Fact) -> bool {
    fact.type_name == GUIDELINE && is_confirmed_guideline(fact.native, fact.loader, &fact.data)
}

fn is_supported_by_a_confirmed_directive(facts: &GapFacts, premise: &str) -> bool {
    facts.edges.iter().any(|edge| {
        edge.relation == SUPPORTS
            && edge.to == premise
            && node(facts, &edge.from).is_some_and(is_confirmed_directive)
    })
}

fn contradicts_a_confirmed_directive(facts: &GapFacts, premise: &str) -> bool {
    facts.edges.iter().any(|edge| {
        edge.relation == CONTRADICTS
            && edge.from == premise
            && node(facts, &edge.to).is_some_and(is_confirmed_directive)
    })
}

#[cfg(test)]
mod tests;
