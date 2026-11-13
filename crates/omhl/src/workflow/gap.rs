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
    let human = is_confirmed_premise(&premise.data);
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

/// Vale a primeira confirmada; sem nenhuma, a primeira gravada.
fn answering_premise<'a>(facts: &'a GapFacts, gap: &str) -> Option<&'a Fact> {
    let mut answers = facts
        .edges
        .iter()
        .filter(|edge| edge.relation == ANSWERS && edge.to == gap)
        .filter_map(|edge| node(facts, &edge.from))
        .filter(|fact| fact.type_name == PREMISE);
    let first = answers.next()?;

    Some(if is_confirmed_premise(&first.data) {
        first
    } else {
        answers
            .find(|fact| is_confirmed_premise(&fact.data))
            .unwrap_or(first)
    })
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
mod tests {
    use serde_json::json;

    use super::is_confirmed_premise;

    #[test]
    fn it_should_confirm_a_premise_only_with_confirmed_status_and_confirmed_by() {
        let by = json!({"by": "person:ana"});
        let confirmed = json!({"status": "confirmed", "confirmed_by": by});
        let legacy = json!({"status": "confirmed"});
        let assumed_with_confirmation = json!({"status": "assumed", "confirmed_by": by});

        assert!(is_confirmed_premise(&confirmed));
        assert!(!is_confirmed_premise(&legacy));
        assert!(!is_confirmed_premise(&assumed_with_confirmation));
    }
}
