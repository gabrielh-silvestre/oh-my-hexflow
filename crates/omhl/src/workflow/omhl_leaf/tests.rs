use crate::workflow::omhl_leaf::{Leaf, resolve_leaf};

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
