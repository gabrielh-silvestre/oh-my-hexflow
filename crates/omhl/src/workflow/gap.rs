use serde_json::Value;

const PREMISE: &str = "premise";
const GUIDELINE: &str = "guideline";
const ANSWERS: &str = "answers";
const SUPPORTS: &str = "supports";
const CONTRADICTS: &str = "contradicts";
pub const PERSON_PREFIX: &str = "person:";
/// `confirmed_by.by` da premissa técnica que o `omhl:critic` confirma.
pub const CRITIC_CONFIRMATION: &str = "critic";

/// Um node do grafo; `data` é `Null` quando o node é nativo.
#[derive(Debug, Clone)]
pub struct Fact {
    pub id: String,
    pub type_name: String,
    pub native: bool,
    /// Gravado pelo cliente `loader`: diretiva de extension, vinculante sem `confirmed_by`.
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

/// Id da premissa que responde o gap, no mesmo critério de `gap_state`.
pub fn answered_by(facts: &GapFacts, gap: &str) -> Option<String> {
    answering_premise(facts, gap).map(|premise| premise.id.clone())
}

/// Vale a resposta de pessoa, depois a do critic; empatando, a primeira gravada.
fn answering_premise<'a>(facts: &'a GapFacts, gap: &str) -> Option<&'a Fact> {
    facts
        .edges
        .iter()
        .filter(|edge| edge.relation == ANSWERS && edge.to == gap)
        .filter_map(|edge| node(facts, &edge.from))
        .filter(|fact| fact.type_name == PREMISE)
        .min_by_key(|fact| answerer_of(&fact.data))
}

/// Só `person:` conta como humano; o critic só vale confirmado por ele mesmo.
fn answerer_of(data: &Value) -> Answerer {
    let by = is_confirmed_premise(data)
        .then(|| confirmer(data))
        .flatten();
    match by {
        Some(by) if by.starts_with(PERSON_PREFIX) => Answerer::Human,
        Some(CRITIC_CONFIRMATION) => Answerer::Critic,
        _ => Answerer::Autonomous,
    }
}

/// `confirmed_by.by` do node, quando existe.
pub fn confirmer(data: &Value) -> Option<&str> {
    data.get("confirmed_by")?.get("by")?.as_str()
}

fn node<'a>(facts: &'a GapFacts, id: &str) -> Option<&'a Fact> {
    facts.nodes.iter().find(|fact| fact.id == id)
}

/// Premissa confirmada: `status` confirmado e `confirmed_by`; a legada `confirmed` sem `confirmed_by` não conta.
pub fn is_confirmed_premise(data: &Value) -> bool {
    data.get("status").and_then(Value::as_str) == Some("confirmed")
        && data.get("confirmed_by").is_some()
}

/// Diretiva confirmada: nativa, escrita pelo cliente `loader` ou com `confirmed_by`.
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
