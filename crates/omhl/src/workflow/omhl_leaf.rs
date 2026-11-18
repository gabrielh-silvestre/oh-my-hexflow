//! A folha da CLI que um comando `omhl` pede: o nome que o `omhl-allow` do papel lista.

pub const GLOBAL_OPTIONS: [&str; 2] = ["--db", "--project-dir"];
pub const INFO_FLAGS: [&str; 4] = ["-h", "--help", "-V", "--version"];
const HELP_WORD: &str = "help";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leaf {
    /// `--help`, `-V`, `<grupo> help`: só imprimem, valem para todo papel.
    ReadOnly,
    /// `append`, `spec validate`: o texto que `commands` lista.
    Named(String),
}

/// `args` são as palavras depois do `omhl`; `groups` são os comandos que só existem com subcomando (`spec`, `check`...).
/// `None` quando a primeira palavra é uma opção que o hook não conhece ou falta a folha.
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

/// `--db=x`, `--project-dir=x`.
fn is_global_assignment(arg: &str) -> bool {
    GLOBAL_OPTIONS.iter().any(|option| {
        arg.strip_prefix(option)
            .is_some_and(|tail| tail.starts_with('='))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUPS: [&str; 2] = ["spec", "report"];

    fn leaf_of(args: &[&str]) -> Option<Leaf> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();

        resolve_leaf(&args, &GROUPS)
    }

    fn named(leaf: &str) -> Option<Leaf> {
        Some(Leaf::Named(leaf.to_owned()))
    }

    #[test]
    fn it_should_skip_global_options_before_the_leaf() {
        assert_eq!(leaf_of(&["--project-dir", ".", "append"]), named("append"));
        assert_eq!(
            leaf_of(&["--db=x", "--project-dir=.", "list"]),
            named("list")
        );
    }

    #[test]
    fn it_should_join_a_group_with_its_subcommand() {
        assert_eq!(
            leaf_of(&["spec", "--db", "x", "init", "--help"]),
            named("spec init")
        );
    }

    #[test]
    fn it_should_resolve_info_flags_and_group_help_as_read_only() {
        let read_only = Some(Leaf::ReadOnly);

        assert_eq!(leaf_of(&["--help"]), read_only);
        assert_eq!(leaf_of(&["--db", "x", "-V"]), read_only);
        assert_eq!(leaf_of(&["spec", "help", "init"]), read_only);
        assert_eq!(leaf_of(&["spec", "--help"]), read_only);
    }

    #[test]
    fn it_should_not_read_an_info_flag_after_the_leaf_as_read_only() {
        assert_eq!(leaf_of(&["attest", "--help"]), named("attest"));
        assert_eq!(leaf_of(&["get", "help"]), named("get"));
    }

    #[test]
    fn it_should_leave_unresolved_what_names_no_leaf() {
        assert_eq!(leaf_of(&[]), None);
        assert_eq!(leaf_of(&["spec"]), None);
        assert_eq!(leaf_of(&["--verbose", "list"]), None);
    }
}
