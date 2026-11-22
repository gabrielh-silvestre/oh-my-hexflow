use std::collections::{HashMap, HashSet};

/// Premissa vigente com os campos que entram na chave de agrupamento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Premise {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub work_item: String,
}

/// Grupo de premissas com o mesmo `kind` e o mesmo texto normalizado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurrence {
    pub kind: String,
    /// Texto normalizado: é a chave do grupo.
    pub text: String,
    /// Distintos, na ordem da primeira aparição.
    pub work_items: Vec<String>,
    /// Todas as premissas do grupo, na ordem da entrada.
    pub premises: Vec<String>,
}

pub fn normalize(text: &str) -> String {
    let collapsed = text
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    collapsed
        .trim_end_matches(|c: char| c.is_whitespace() || ".,;:!?".contains(c))
        .to_owned()
}

pub fn recurrences(premises: &[Premise], directives: &[String], min: u32) -> Vec<Recurrence> {
    let directives: HashSet<String> = directives.iter().map(|text| normalize(text)).collect();
    let mut index: HashMap<(&str, String), usize> = HashMap::new();
    let mut groups: Vec<Recurrence> = Vec::new();
    for premise in premises {
        let text = normalize(&premise.text);
        let slot = *index
            .entry((&premise.kind, text.clone()))
            .or_insert_with(|| {
                groups.push(Recurrence {
                    kind: premise.kind.clone(),
                    text,
                    work_items: Vec::new(),
                    premises: Vec::new(),
                });
                groups.len() - 1
            });
        let group = &mut groups[slot];
        if !group.work_items.contains(&premise.work_item) {
            group.work_items.push(premise.work_item.clone());
        }
        group.premises.push(premise.id.clone());
    }

    groups
        .into_iter()
        .filter(|group| {
            u32::try_from(group.work_items.len()).is_ok_and(|count| count >= min)
                && !directives.contains(&group.text)
        })
        .collect()
}

#[cfg(test)]
mod tests;
