//! CA-059: os nomes novos do `int1` (CMP-045) usam termos do git ou do plugin, nunca `pr` solto nem
//! `gh`; `pull-request` só no tipo e nos códigos `pull-request-*`.

use crate::support;

use std::fs;

use serde_json::Value;
use support::child::omhl;
use support::repo_checks::repo_root;

/// A lista permitida desta spec: comandos e hooks novos (os eventos e o gate não têm ajuda própria).
const ALLOWED: [&str; 7] = [
    "merge",
    "post-merge",
    "branch-birth",
    "command-guard",
    "mcp-merge-guard",
    "mcp-write-warning",
    "branch-created",
];

fn has_forbidden_word(name: &str) -> bool {
    name.split(['-', ' ', '_'])
        .any(|word| word == "pr" || word == "gh")
}

/// Os nomes da seção `Commands:` da ajuda de `omhl <group> --help`.
fn help_commands(group: &[&str]) -> Vec<String> {
    let mut args = group.to_vec();
    args.push("--help");
    let output = omhl(&args).run();
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

    output
        .stdout
        .lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .filter(|name| name != "help")
        .collect()
}

/// O último argumento de cada hook do `plugin/hooks/hooks.json` (o nome passado ao `omhl-hook.sh`).
fn plugin_hooks() -> Vec<String> {
    let text = fs::read_to_string(repo_root().join("plugin/hooks/hooks.json"))
        .expect("hooks.json is read");
    let config: Value = serde_json::from_str(&text).expect("hooks.json is JSON");

    config["hooks"]
        .as_object()
        .expect("hooks is an object")
        .values()
        .flat_map(|matchers| matchers.as_array().cloned().unwrap_or_default())
        .flat_map(|matcher| matcher["hooks"].as_array().cloned().unwrap_or_default())
        .filter_map(|hook| {
            hook["args"]
                .as_array()
                .and_then(|args| args.last())
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

#[test]
fn it_should_name_no_command_or_hook_of_the_binary_with_pr_or_gh() {
    let names: Vec<String> = [&[][..], &["hook"], &["hooks"]]
        .iter()
        .flat_map(|group| help_commands(group))
        .collect();

    let forbidden: Vec<&String> = names
        .iter()
        .filter(|name| has_forbidden_word(name))
        .collect();
    assert!(forbidden.is_empty(), "{forbidden:?}");
}

#[test]
fn it_should_keep_pull_request_out_of_the_command_and_hook_names() {
    let names: Vec<String> = [&[][..], &["hook"], &["hooks"]]
        .iter()
        .flat_map(|group| help_commands(group))
        .chain(plugin_hooks())
        .collect();

    let named: Vec<&String> = names
        .iter()
        .filter(|name| name.contains("pull-request"))
        .collect();
    assert!(named.is_empty(), "{named:?}");
}

#[test]
fn it_should_offer_each_new_command_and_hook_in_the_binary_help() {
    let names: Vec<String> = [&[][..], &["hook"], &["hooks"]]
        .iter()
        .flat_map(|group| help_commands(group))
        .collect();

    let absent: Vec<&str> = ALLOWED
        .iter()
        .copied()
        .filter(|name| !names.iter().any(|offered| offered == name))
        .collect();
    assert!(absent.is_empty(), "{absent:?} not in {names:?}");
}

#[test]
fn it_should_name_no_plugin_hook_with_pr_or_gh() {
    let hooks = plugin_hooks();

    let forbidden: Vec<&String> = hooks
        .iter()
        .filter(|name| has_forbidden_word(name))
        .collect();
    assert!(forbidden.is_empty(), "{forbidden:?}");
}

/// Os códigos em crases da seção `## Errors` da página do `omhl merge` (onde a EMD-026 os põe).
fn merge_page_codes() -> Vec<String> {
    let text = fs::read_to_string(repo_root().join("plugin/skills/review/references/merge.md"))
        .expect("merge.md is read");
    let errors = text
        .split("## Errors")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("the merge page has an Errors section")
        .to_owned();

    errors
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| !span.is_empty() && span.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn it_should_name_no_error_code_of_the_merge_page_with_pr_or_gh() {
    let codes = merge_page_codes();

    let forbidden: Vec<&String> = codes
        .iter()
        .filter(|code| has_forbidden_word(code))
        .collect();
    assert!(!codes.is_empty());
    assert!(forbidden.is_empty(), "{forbidden:?}");
}

#[test]
fn it_should_use_pull_request_only_as_the_prefix_of_an_error_code_of_the_merge_page() {
    let codes = merge_page_codes();

    let misplaced: Vec<&String> = codes
        .iter()
        .filter(|code| *code != "pull-request")
        .filter(|code| code.contains("pull-request") && !code.starts_with("pull-request-"))
        .collect();
    assert!(misplaced.is_empty(), "{misplaced:?}");
}
