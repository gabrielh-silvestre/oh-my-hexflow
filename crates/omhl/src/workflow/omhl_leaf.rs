pub const GLOBAL_OPTIONS: [&str; 3] = ["--db", "--project-dir", "--format"];
pub const INFO_FLAGS: [&str; 4] = ["-h", "--help", "-V", "--version"];
const HELP_WORD: &str = "help";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leaf {
    ReadOnly,
    Named(String),
}

/// Comando de primeiro nível com subcomandos. `takes_positionals` marca o grupo híbrido (`byproduct`): também é folha, com argumentos próprios.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub subcommands: Vec<String>,
    pub takes_positionals: bool,
}

impl Group {
    fn has_subcommand(&self, word: &str) -> bool {
        self.subcommands.iter().any(|sub| sub == word)
    }
}

fn find_group<'g>(groups: &'g [Group], name: &str) -> Option<&'g Group> {
    groups.iter().find(|group| group.name == name)
}

pub fn resolve_leaf(args: &[String], groups: &[Group]) -> Option<Leaf> {
    let mut informational = false;
    let mut words: Vec<&str> = Vec::new();
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        if GLOBAL_OPTIONS.contains(&arg) {
            rest.next();
        } else if INFO_FLAGS.contains(&arg) {
            informational = true;
        } else if is_global_assignment(arg) {
            continue;
        } else if arg.starts_with('-') {
            // Num grupo híbrido a opção é do grupo: encerra a leitura, e um subcomando entre as palavras restantes vale.
            let hybrid = match words.as_slice() {
                [name] => find_group(groups, name).filter(|group| group.takes_positionals),
                _ => None,
            };

            return hybrid.map(|group| match rest.find(|word| group.has_subcommand(word)) {
                Some(sub) => Leaf::Named(format!("{} {sub}", group.name)),
                None => Leaf::Named(group.name.clone()),
            });
        } else {
            words.push(arg);
            if words.len() == 2 || find_group(groups, words[0]).is_none() {
                break;
            }
        }
    }

    match words.as_slice() {
        [] => informational.then_some(Leaf::ReadOnly),
        [name] => match find_group(groups, name) {
            Some(_) if informational => Some(Leaf::ReadOnly),
            Some(group) => group
                .takes_positionals
                .then(|| Leaf::Named(group.name.clone())),
            None => Some(Leaf::Named((*name).to_owned())),
        },
        [_, HELP_WORD] => Some(Leaf::ReadOnly),
        [name, word, ..] => match find_group(groups, name) {
            // Num grupo híbrido, a segunda palavra que não é subcomando é argumento do próprio grupo.
            Some(group) if group.takes_positionals && !group.has_subcommand(word) => {
                Some(Leaf::Named(group.name.clone()))
            }
            _ => Some(Leaf::Named(format!("{name} {word}"))),
        },
    }
}

fn is_global_assignment(arg: &str) -> bool {
    GLOBAL_OPTIONS.iter().any(|option| {
        arg.strip_prefix(option)
            .is_some_and(|tail| tail.starts_with('='))
    })
}

#[cfg(test)]
mod tests;
