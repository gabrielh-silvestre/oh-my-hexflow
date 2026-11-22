pub const GLOBAL_OPTIONS: [&str; 3] = ["--db", "--project-dir", "--format"];
pub const INFO_FLAGS: [&str; 4] = ["-h", "--help", "-V", "--version"];
const HELP_WORD: &str = "help";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leaf {
    ReadOnly,
    Named(String),
}

pub fn resolve_leaf(args: &[String], groups: &[&str]) -> Option<Leaf> {
    let mut informational = false;
    let mut words: Vec<&str> = Vec::new();
    let mut rest = args.iter().map(String::as_str);
    while let Some(arg) = rest.next() {
        match arg {
            _ if GLOBAL_OPTIONS.contains(&arg) => {
                rest.next();
            }
            _ if INFO_FLAGS.contains(&arg) => informational = true,
            _ if is_global_assignment(arg) => {}
            _ if arg.starts_with('-') => return None,
            _ => {
                words.push(arg);
                if words.len() == 2 || !groups.contains(&words[0]) {
                    break;
                }
            }
        }
    }

    match words.as_slice() {
        [] => informational.then_some(Leaf::ReadOnly),
        [group] if groups.contains(group) => informational.then_some(Leaf::ReadOnly),
        [_, HELP_WORD] => Some(Leaf::ReadOnly),
        [leaf] => Some(Leaf::Named((*leaf).to_owned())),
        [group, leaf] => Some(Leaf::Named(format!("{group} {leaf}"))),
        _ => None,
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
