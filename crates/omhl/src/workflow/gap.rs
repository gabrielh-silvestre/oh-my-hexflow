use serde_json::Value;

const PREMISE: &str = "premise";
const GUIDELINE: &str = "guideline";
const ANSWERS: &str = "answers";
const SUPPORTS: &str = "supports";
const CONTRADICTS: &str = "contradicts";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answerer {
    Human,
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
    let human = has_confirmed_by(premise);
    if !human && contradicts_a_confirmed_directive(facts, &premise.id) {
        return GapState::NeedsHuman;
    }

    GapState::Answered {
        by: if human {
            Answerer::Human
        } else {
            Answerer::Autonomous
        },
        criterion: is_supported_by_a_confirmed_directive(facts, &premise.id),
    }
}

/// Id da premissa que responde o gap, no mesmo critério de `gap_state`.
pub fn answered_by(facts: &GapFacts, gap: &str) -> Option<String> {
    answering_premise(facts, gap).map(|premise| premise.id.clone())
}

/// Vale a primeira com `confirmed_by`; sem nenhuma, a primeira gravada.
fn answering_premise<'a>(facts: &'a GapFacts, gap: &str) -> Option<&'a Fact> {
    let mut answers = facts
        .edges
        .iter()
        .filter(|edge| edge.relation == ANSWERS && edge.to == gap)
        .filter_map(|edge| node(facts, &edge.from))
        .filter(|fact| fact.type_name == PREMISE);
    let first = answers.next()?;

    Some(if has_confirmed_by(first) {
        first
    } else {
        answers.find(|fact| has_confirmed_by(fact)).unwrap_or(first)
    })
}

fn node<'a>(facts: &'a GapFacts, id: &str) -> Option<&'a Fact> {
    facts.nodes.iter().find(|fact| fact.id == id)
}

fn has_confirmed_by(fact: &Fact) -> bool {
    fact.data.get("confirmed_by").is_some()
}

fn is_confirmed_directive(fact: &Fact) -> bool {
    fact.type_name == GUIDELINE && (fact.native || fact.loader || has_confirmed_by(fact))
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
