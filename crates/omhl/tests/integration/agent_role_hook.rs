use std::collections::BTreeSet;

use clap::CommandFactory;
use omhl::adapters::cli::Cli;
use omhl::workflow::omhl_leaf::{GLOBAL_OPTIONS, INFO_FLAGS};

// Listas à mão contra o clap

#[test]
fn it_should_list_exactly_the_global_options_of_the_cli() {
    let mut cli = Cli::command();
    cli.build();
    let globals: BTreeSet<String> = cli
        .get_arguments()
        .filter(|argument| argument.is_global_set())
        .map(|argument| {
            format!(
                "--{}",
                argument
                    .get_long()
                    .expect("a global option has a long name")
            )
        })
        .collect();

    assert_eq!(
        globals,
        GLOBAL_OPTIONS
            .iter()
            .map(|option| (*option).to_owned())
            .collect()
    );
}

#[test]
fn it_should_list_exactly_the_informational_flags_of_the_cli() {
    let mut cli = Cli::command();
    cli.build();
    let flags: BTreeSet<String> = cli
        .get_arguments()
        .filter(|argument| matches!(argument.get_id().as_str(), "help" | "version"))
        .flat_map(|argument| {
            [
                argument.get_short().map(|short| format!("-{short}")),
                argument.get_long().map(|long| format!("--{long}")),
            ]
        })
        .flatten()
        .collect();

    assert_eq!(
        flags,
        INFO_FLAGS.iter().map(|flag| (*flag).to_owned()).collect()
    );
}
