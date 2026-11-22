use hexlog::SemanticId;

pub fn order(ids: &[SemanticId], after: &[(SemanticId, SemanticId)]) -> Vec<SemanticId> {
    let mut remaining: Vec<&SemanticId> = ids.iter().collect();
    remaining.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    remaining.dedup();
    let mut ordered = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        // Aresta para fora de `ids` nunca está em `remaining`, então não prende ninguém.
        let (free, waiting): (Vec<&SemanticId>, Vec<&SemanticId>) =
            remaining.iter().copied().partition(|id| {
                !after
                    .iter()
                    .any(|(from, to)| from == *id && remaining.contains(&to))
            });
        if free.is_empty() {
            // Ciclo: os restantes saem em ordem alfabética e a função termina.
            ordered.extend(waiting.into_iter().cloned());
            break;
        }
        ordered.extend(free.into_iter().cloned());
        remaining = waiting;
    }
    ordered
}

#[cfg(test)]
mod tests;
