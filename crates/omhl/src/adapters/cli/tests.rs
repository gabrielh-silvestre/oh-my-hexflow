use super::*;

fn parsed(args: &[&str]) -> Result<Cli, clap::Error> {
    command()
        .try_get_matches_from(args)
        .and_then(|matches| Cli::from_arg_matches(&matches))
}

#[test]
fn it_should_have_one_command_key_for_each_clap_leaf_command() {
    let mut leaves = command_leaves(&Cli::command());
    let mut keyed: Vec<String> = CommandKey::ALL
        .into_iter()
        .map(|key| command_path(key).to_owned())
        .collect();
    leaves.sort();
    keyed.sort();

    assert_eq!(keyed, leaves);
}

#[test]
fn it_should_list_a_hybrid_group_as_a_leaf_and_descend_into_its_subcommand() {
    let leaves = command_leaves(&Cli::command());

    assert!(leaves.contains(&"byproduct".to_owned()));
    assert!(leaves.contains(&"byproduct prune".to_owned()));
    assert!(!leaves.contains(&"check".to_owned()));
}

#[test]
fn it_should_mark_only_byproduct_as_a_group_with_positionals() {
    let groups = command_groups(&Cli::command());
    let with_positionals: Vec<&str> = groups
        .iter()
        .filter(|group| group.takes_positionals)
        .map(|group| group.name.as_str())
        .collect();
    let byproduct = groups.iter().find(|group| group.name == "byproduct");

    assert_eq!(with_positionals, ["byproduct"]);
    assert_eq!(
        byproduct.map(|group| group.subcommands.clone()),
        Some(vec!["prune".to_owned()])
    );
}

#[test]
fn it_should_read_the_prune_subcommand_as_its_own_command_key() {
    let cli = parsed(&["omhl", "byproduct", "prune"]).expect("prune parses");

    assert_eq!(command_key(&cli.command), CommandKey::ByproductPrune);
}

#[test]
fn it_should_keep_reading_a_check_and_a_work_item_as_the_byproduct_command() {
    let cli = parsed(&[
        "omhl",
        "byproduct",
        "--tail",
        "3",
        "checks.x",
        "work-items.y",
    ])
    .expect("the read parses");

    assert_eq!(command_key(&cli.command), CommandKey::Byproduct);
}

#[test]
fn it_should_refuse_byproduct_without_arguments_as_a_usage_error() {
    let refused = parsed(&["omhl", "byproduct"]);

    assert_eq!(
        refused.map(|_| ()).map_err(|usage| usage.kind()),
        Err(ErrorKind::MissingRequiredArgument)
    );
}

#[test]
fn it_should_not_turn_an_option_before_prune_into_the_prune_action() {
    let parsed = parsed(&["omhl", "byproduct", "--tail", "3", "prune"]);

    let key = parsed.as_ref().ok().map(|cli| command_key(&cli.command));

    assert_ne!(key, Some(CommandKey::ByproductPrune));
}

#[test]
fn it_should_refuse_a_byproduct_without_check_and_work_item_that_clap_did_not_catch() {
    let cli = Cli {
        db: None,
        project_dir: None,
        format: None,
        command: Command::Action(ActionCommand::Byproduct {
            command: None,
            check: None,
            work_item: None,
            tail: 50,
        }),
    };

    let usage = byproduct_misuse(&cli);

    assert_eq!(
        usage.map(|error| error.kind()),
        Some(ErrorKind::MissingRequiredArgument)
    );
}
