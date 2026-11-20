use crate::support;

use std::fs;

use serde_json::json;
use support::agent_examples::agent_file;
use support::fences::{FenceLine, lines_by_fence};
use support::hook::{bash_call, decision_of, run_agent_role_in};
use support::repo_checks::repo_root;

// Hook agent-role: cada comando `omhl` do prompt, como escrito, é liberado com o papel do agente

/// Cada bloco cercado do agente cujo texto começa com `omhl`, exatamente como escrito.
fn fenced_omhl_commands(role: &str) -> Vec<String> {
    let file = fs::read_to_string(agent_file(role)).expect("the agent file is readable");
    let mut commands = Vec::new();
    let mut block: Vec<&str> = Vec::new();

    for (place, line) in lines_by_fence(&file) {
        match place {
            FenceLine::Open => block.clear(),
            FenceLine::Inside => block.push(line),
            FenceLine::Close => {
                if block.first().is_some_and(|first| first.starts_with("omhl")) {
                    commands.push(block.join("\n"));
                }
            }
            FenceLine::Outside => {}
        }
    }

    commands
}

fn assert_hook_allows_every_fenced_command(role: &str) {
    let plugin_root = repo_root().join("plugin");
    let agent_type = format!("oh-my-hexflow:{role}");
    let commands = fenced_omhl_commands(role);
    assert!(!commands.is_empty(), "{role} shows no omhl command");

    for command in commands {
        let output = run_agent_role_in(&plugin_root, &bash_call(&command, Some(&agent_type)));
        let decision = decision_of(&output);

        assert_eq!(
            decision["permissionDecision"], "allow",
            "{role}: {command}\n{decision}"
        );
        assert_eq!(
            decision["updatedInput"]["command"],
            json!(format!("OMHL_ROLE=omhl:{role} {command}")),
            "{role}: {command}"
        );
    }
}

#[test]
fn it_should_allow_every_fenced_command_of_the_analyst_through_the_hook() {
    assert_hook_allows_every_fenced_command("analyst");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_architect_through_the_hook() {
    assert_hook_allows_every_fenced_command("architect");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_code_reviewer_through_the_hook() {
    assert_hook_allows_every_fenced_command("code-reviewer");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_security_reviewer_through_the_hook() {
    assert_hook_allows_every_fenced_command("security-reviewer");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_critic_through_the_hook() {
    assert_hook_allows_every_fenced_command("critic");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_verifier_through_the_hook() {
    assert_hook_allows_every_fenced_command("verifier");
}
