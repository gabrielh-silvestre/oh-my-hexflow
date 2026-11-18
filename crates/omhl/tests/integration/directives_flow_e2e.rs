#![cfg(unix)]

use crate::support;

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use support::directives_flow_e2e::BRANCH;
use tempfile::TempDir;

// O `case` do hook `pre-switch` do `wt`, sem o `wt`

const WT_CONFIG: &str = "../../.config/wt.toml";

/// O texto do comando `omhl-gate` de `[pre-switch]` em `.config/wt.toml`, na raiz do repositório.
fn pre_switch_template() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(WT_CONFIG);
    let text = fs::read_to_string(&path).expect("the wt project config is readable");
    let config: toml::Value = toml::from_str(&text).expect("the wt project config is valid TOML");

    config["pre-switch"]["omhl-gate"]
        .as_str()
        .expect("the omhl-gate hook is a string")
        .to_owned()
}

/// Executa o template sob `sh`, como o `wt` faz, num `PATH` que é uma pasta vazia: sem o `omhl`.
fn run_pre_switch(branch: &str) -> Option<i32> {
    let script = pre_switch_template().replace("{{ branch }}", branch);
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

#[test]
fn it_should_not_wrap_the_branch_placeholder_in_quotes_in_the_pre_switch_template() {
    let template = pre_switch_template();

    let parts: Vec<&str> = template.split("{{ branch }}").collect();

    let quoted = parts
        .windows(2)
        .any(|pair| pair[0].ends_with(['\'', '"']) || pair[1].starts_with(['\'', '"']));
    assert!(parts.len() > 2, "template: {template}");
    assert!(!quoted, "template: {template}");
}
