use super::*;

fn leaf_paths(command: &clap::Command, prefix: &str) -> Vec<String> {
    command
        .get_subcommands()
        .flat_map(|sub| {
            let path = format!("{prefix}{}", sub.get_name());
            match sub.has_subcommands() {
                true => leaf_paths(sub, &format!("{path} ")),
                false => vec![path],
            }
        })
        .collect()
}

#[test]
fn it_should_have_one_command_key_for_each_clap_leaf_command() {
    let mut leaves = leaf_paths(&Cli::command(), "");
    let mut keyed: Vec<String> = CommandKey::ALL
        .into_iter()
        .map(|key| command_path(key).to_owned())
        .collect();
    leaves.sort();
    keyed.sort();

    assert_eq!(keyed, leaves);
}
