use crate::workflow::omhl_leaf::{Group, Leaf, resolve_leaf};

fn group(name: &str, subcommands: &[&str], takes_positionals: bool) -> Group {
    Group {
        name: name.to_owned(),
        subcommands: subcommands.iter().map(|sub| (*sub).to_owned()).collect(),
        takes_positionals,
    }
}

/// `spec` e `report` só têm subcomandos; `byproduct` é híbrido (argumentos próprios e o subcomando `prune`).
fn groups() -> Vec<Group> {
    vec![
        group("spec", &["init", "validate"], false),
        group("report", &["answers", "recurrences", "retro"], false),
        group("byproduct", &["prune"], true),
    ]
}

fn leaf_of(args: &[&str]) -> Option<Leaf> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();

    resolve_leaf(&args, &groups())
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
fn it_should_skip_the_format_option_given_as_two_words_before_the_leaf() {
    let args = ["--format", "json", "list"];

    let leaf = leaf_of(&args);

    assert_eq!(leaf, named("list"));
}

#[test]
fn it_should_skip_the_format_option_given_as_an_assignment_before_the_leaf() {
    let args = ["--format=json", "list"];

    let leaf = leaf_of(&args);

    assert_eq!(leaf, named("list"));
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

#[test]
fn it_should_resolve_the_reading_of_a_hybrid_group_to_the_group_leaf() {
    assert_eq!(
        leaf_of(&["byproduct", "checks.x", "work-items.y"]),
        named("byproduct")
    );
    assert_eq!(
        leaf_of(&["byproduct", "--tail", "40", "checks.x", "work-items.y"]),
        named("byproduct")
    );
}

#[test]
fn it_should_resolve_the_subcommand_of_a_hybrid_group_in_every_spelling() {
    let expected = named("byproduct prune");

    assert_eq!(leaf_of(&["byproduct", "prune"]), expected);
    assert_eq!(
        leaf_of(&["--format", "json", "byproduct", "prune"]),
        expected
    );
    assert_eq!(
        leaf_of(&["byproduct", "prune", "--format", "json"]),
        expected
    );
}

#[test]
fn it_should_resolve_a_subcommand_that_follows_an_option_of_a_hybrid_group_to_the_subcommand() {
    assert_eq!(
        leaf_of(&["byproduct", "--tail", "3", "prune"]),
        named("byproduct prune")
    );
}

#[test]
fn it_should_resolve_a_hybrid_group_without_a_following_word_to_the_group_leaf() {
    assert_eq!(leaf_of(&["byproduct"]), named("byproduct"));
    assert_eq!(leaf_of(&["--db", "x", "byproduct"]), named("byproduct"));
}

#[test]
fn it_should_resolve_the_help_of_a_hybrid_group_as_read_only() {
    let read_only = Some(Leaf::ReadOnly);

    assert_eq!(leaf_of(&["byproduct", "--help"]), read_only);
    assert_eq!(leaf_of(&["byproduct", "help", "prune"]), read_only);
}

#[test]
fn it_should_keep_resolving_a_group_that_is_not_hybrid_as_before() {
    assert_eq!(leaf_of(&["spec", "init"]), named("spec init"));
    assert_eq!(leaf_of(&["spec", "init", "--help"]), named("spec init"));
    assert_eq!(leaf_of(&["spec", "--tail", "3", "init"]), None);
}
