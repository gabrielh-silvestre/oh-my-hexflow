#![cfg(unix)]

use crate::support;

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use support::directives_flow_e2e::BRANCH;
use tempfile::TempDir;

// O `case` dos hooks `pre-switch` e `post-start` do `wt`, sem o `wt`

const WT_CONFIG: &str = "../../.config/wt.toml";

/// O texto do comando `hook` de `[table]` em `.config/wt.toml`, na raiz do repositório.
fn hook_template(table: &str, hook: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(WT_CONFIG);
    let text = fs::read_to_string(&path).expect("the wt project config is readable");
    let config: toml::Value = toml::from_str(&text).expect("the wt project config is valid TOML");

    config[table][hook]
        .as_str()
        .expect("the hook is a string")
        .to_owned()
}

fn pre_switch_template() -> String {
    hook_template("pre-switch", "omhl-gate")
}

fn post_start_template() -> String {
    hook_template("post-start", "omhl-freeze")
}

/// Executa o template sob `sh`, como o `wt` faz, num `PATH` que é uma pasta vazia: sem o `omhl`.
fn run_template(template: &str, branch: &str) -> Option<i32> {
    let script = template.replace("{{ branch }}", branch);
    let empty = TempDir::new().expect("empty PATH directory is created");

    Command::new("/bin/sh")
        .args(["-c", &script])
        .env_clear()
        .env("PATH", empty.path())
        .stdin(Stdio::null())
        .output()
        .expect("sh runs")
        .status
        .code()
}

fn run_pre_switch(branch: &str) -> Option<i32> {
    run_template(&pre_switch_template(), branch)
}

/// Escapa o nome como o `wt` faz antes de entregá-lo ao `sh`: aspas simples, com `'` virando `'\''`.
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[test]
fn it_should_let_develop_through_the_pre_switch_hook_without_omhl_on_the_path() {
    let code = run_pre_switch("develop");

    assert_eq!(code, Some(0));
}

#[test]
fn it_should_let_main_through_the_pre_switch_hook_without_omhl_on_the_path() {
    let code = run_pre_switch("main");

    assert_eq!(code, Some(0));
}

#[test]
fn it_should_fail_the_pre_switch_hook_with_127_for_any_other_branch_when_omhl_is_missing() {
    let code = run_pre_switch(BRANCH);

    assert_eq!(code, Some(127));
}

fn assert_placeholder_unquoted(template: &str) {
    let parts: Vec<&str> = template.split("{{ branch }}").collect();

    let quoted = parts
        .windows(2)
        .any(|pair| pair[0].ends_with(['\'', '"']) || pair[1].starts_with(['\'', '"']));
    assert!(parts.len() > 2, "template: {template}");
    assert!(!quoted, "template: {template}");
}

#[test]
fn it_should_not_wrap_the_branch_placeholder_in_quotes_in_the_pre_switch_template() {
    assert_placeholder_unquoted(&pre_switch_template());
}

#[test]
fn it_should_let_develop_through_the_post_start_hook_without_omhl_on_the_path() {
    let code = run_template(&post_start_template(), "develop");

    assert_eq!(code, Some(0));
}

#[test]
fn it_should_let_main_through_the_post_start_hook_without_omhl_on_the_path() {
    let code = run_template(&post_start_template(), "main");

    assert_eq!(code, Some(0));
}

#[test]
fn it_should_not_wrap_the_branch_placeholder_in_quotes_in_the_post_start_template() {
    assert_placeholder_unquoted(&post_start_template());
}

#[test]
fn it_should_not_execute_a_hostile_branch_name_in_the_post_start_template() {
    let workdir = TempDir::new().expect("work directory is created");
    let hostile = format!("feat/wi-x__a;touch {}/pwned", workdir.path().display());
    let code = run_template(&post_start_template(), &shell_escape(&hostile));

    assert_eq!(code, Some(127));
    assert!(!workdir.path().join("pwned").exists());
}
