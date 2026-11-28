use crate::support;

use serde_json::{Value, json};
use support::child::{Cmd, omhl_json};
use support::ext::ExtTree;
use support::git::git_ok;
use support::project::Project;

const BLOCKING: &str = r#"blocking = true
how = "Run lint"
run = ["true"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-commit"
"#;
const NON_BLOCKING: &str =
    "blocking = false\nhow = \"Run lint\"\n\n[when]\nevent = \"before-commit\"\n";
const TEXT_CHECK: &str = "blocking = false\nhow = \"Read it\"\n\n[when]\nevent = \"before-push\"\n";

fn omhl_in(p: &Project, args: &[&str]) -> Cmd {
    omhl_json(args).cwd(p.dir()).xdg(p.xdg())
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_code(stderr: &str) -> String {
    json_line(stderr)["error"]["code"]
        .as_str()
        .expect("the error has a code")
        .to_owned()
}

/// Check `lint` carregado como bloqueante e o arquivo reescrito sem `blocking`.
fn loosened_check_project() -> Project {
    let p = Project::new("acme");
    ExtTree::at(p.dir()).check("lint", BLOCKING);
    assert_eq!(omhl_in(&p, &["load"]).run().code, Some(0));
    ExtTree::at(p.dir()).check("lint", NON_BLOCKING);

    p
}

/// Check de texto carregado e arquivo apagado: o node segue vigente, órfão.
fn orphaned_check_project() -> Project {
    let p = Project::new("acme");
    ExtTree::at(p.dir()).check("gate", TEXT_CHECK);
    assert_eq!(omhl_in(&p, &["load"]).run().code, Some(0));
    std::fs::remove_file(p.dir().join(".omhl/checks/gate.toml")).expect("the file is removed");

    p
}

/// Work item `feat` gravado em repositório git na `develop`; devolve o id do registro.
fn feat_work_item_project() -> (Project, String) {
    let p = Project::new("acme");
    git_ok(p.dir(), &["init", "-q", "-b", "develop"]);
    git_ok(p.dir(), &["config", "user.name", "Test Author"]);
    git_ok(p.dir(), &["config", "user.email", "author@example.com"]);
    git_ok(p.dir(), &["commit", "-q", "--allow-empty", "-m", "base"]);
    let created = omhl_in(&p, &["append"])
        .stdin(append_request(None, "feat"))
        .run();
    assert_eq!(created.code, Some(0), "stderr: {}", created.stderr);
    let id = json_line(&created.stdout)["records"][0]["id"]
        .as_str()
        .expect("the record has an id")
        .to_owned();

    (p, id)
}

fn append_request(base: Option<&str>, type_value: &str) -> String {
    let mut item = json!({ "body": {
        "kind": "node", "id": "work-items.w1", "type": "work-item",
        "data": { "title": "t", "status": "open", "type": type_value }
    }});
    if let Some(base) = base {
        item["base"] = json!(base);
    }

    json!({ "items": [item] }).to_string()
}

fn change_type_to_chore(p: &Project, base: &str, extra: &[&str]) -> support::child::Output {
    let mut args = vec!["append"];
    args.extend_from_slice(extra);

    omhl_in(p, &args)
        .stdin(append_request(Some(base), "chore"))
        .run()
}

// load

#[test]
fn it_should_refuse_a_loosening_load_without_the_option() {
    let p = loosened_check_project();

    let output = omhl_in(&p, &["load"]).run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(error_code(&output.stderr), "check-loosening-refused");
}

#[test]
fn it_should_load_a_loosening_check_with_owner_override() {
    let p = loosened_check_project();

    let output = omhl_in(&p, &["load", "--owner-override"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_not_turn_the_option_on_from_an_environment_variable_named_like_it() {
    let p = loosened_check_project();

    let plain = omhl_in(&p, &["load"]).env("OWNER_OVERRIDE", "1").run();
    let prefixed = omhl_in(&p, &["load"]).env("OMHL_OWNER_OVERRIDE", "1").run();

    assert_eq!(plain.code, Some(1), "stderr: {}", plain.stderr);
    assert_eq!(prefixed.code, Some(1), "stderr: {}", prefixed.stderr);
}

// check retire

#[test]
fn it_should_retire_an_orphaned_check_with_owner_override() {
    let p = orphaned_check_project();

    let output = omhl_in(&p, &["check", "retire", "checks.gate", "--owner-override"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_refuse_to_retire_an_orphaned_check_with_the_option_in_the_environment() {
    let p = orphaned_check_project();

    let output = omhl_in(&p, &["check", "retire", "checks.gate"])
        .env("OWNER_OVERRIDE", "1")
        .run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(error_code(&output.stderr), "check-loosening-refused");
}

// append

#[test]
fn it_should_refuse_a_loosening_type_change_through_append_without_the_option() {
    let (p, base) = feat_work_item_project();

    let output = change_type_to_chore(&p, &base, &[]);

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(error_code(&output.stderr), "type-change-loosens");
}

#[test]
fn it_should_accept_a_loosening_type_change_through_append_with_owner_override() {
    let (p, base) = feat_work_item_project();

    let output = change_type_to_chore(&p, &base, &["--owner-override"]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

// cada recusa só no comando dela

#[test]
fn it_should_not_let_load_owner_override_retire_a_check() {
    let p = orphaned_check_project();

    let output = omhl_in(&p, &["load", "--owner-override"]).run();
    let retired = omhl_in(&p, &["check", "retire", "checks.gate"]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(error_code(&retired.stderr), "check-loosening-refused");
}

// sintaxe da opção

#[test]
fn it_should_exit_2_for_an_abbreviation_of_owner_override() {
    let p = loosened_check_project();

    for abbreviation in ["--owner", "--owner-over", "--override"] {
        let output = omhl_in(&p, &["load", abbreviation]).run();

        assert_eq!(output.code, Some(2), "{abbreviation}: {}", output.stderr);
    }
}

#[test]
fn it_should_exit_2_when_owner_override_gets_a_value() {
    let p = loosened_check_project();

    let output = omhl_in(&p, &["load", "--owner-override=true"]).run();

    assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
}

#[test]
fn it_should_exit_2_for_owner_override_outside_load_check_retire_and_append() {
    let p = Project::new("acme");

    for args in [
        &["verify", "--owner-override"][..],
        &["list", "--owner-override"],
        &["get", "checks.x", "--owner-override"],
        &["check", "run", "--owner-override"],
        &["guideline", "retire", "guidelines.x", "--owner-override"],
        &["gate-delivery", "work-items.w1", "--owner-override"],
        &["close", "work-items.w1", "--owner-override"],
    ] {
        let output = omhl_in(&p, args).run();

        assert_eq!(output.code, Some(2), "{args:?}: {}", output.stderr);
    }
}

#[test]
fn it_should_exit_2_for_base_outside_gate_delivery_and_close() {
    let p = Project::new("acme");

    for args in [
        &["verify", "--base", "develop"][..],
        &["load", "--base", "develop"],
        &["list", "--base", "develop"],
        &["check", "retire", "checks.x", "--base", "develop"],
        &["append", "--base", "develop"],
    ] {
        let output = omhl_in(&p, args).run();

        assert_eq!(output.code, Some(2), "{args:?}: {}", output.stderr);
    }
}

// guidance por comando

fn guidance_of(stderr: &str) -> (String, String) {
    let error = &json_line(stderr)["error"];
    let cause = error["message"].as_str().expect("a message").to_owned();
    let next_step = error["details"][0]["next_step"]
        .as_str()
        .expect("a next step")
        .to_owned();

    (cause, next_step)
}

#[test]
fn it_should_say_in_the_cause_that_retiring_a_check_is_the_owners_task() {
    let p = orphaned_check_project();

    let output = omhl_in(&p, &["check", "retire", "checks.gate"]).run();

    let (cause, _) = guidance_of(&output.stderr);
    assert!(cause.contains("retiring a check"), "{cause}");
    assert!(cause.contains("owner"), "{cause}");
}

#[test]
fn it_should_name_owner_override_as_the_owners_outside_claude_code_in_the_retire_next_step() {
    let p = orphaned_check_project();

    let output = omhl_in(&p, &["check", "retire", "checks.gate"]).run();

    let (_, next_step) = guidance_of(&output.stderr);
    assert!(next_step.contains("--owner-override"), "{next_step}");
    assert!(next_step.contains("owner"), "{next_step}");
    assert!(next_step.contains("outside Claude Code"), "{next_step}");
}

#[test]
fn it_should_not_tell_the_agent_to_retire_the_check_in_the_retire_next_step() {
    let p = orphaned_check_project();

    let output = omhl_in(&p, &["check", "retire", "checks.gate"]).run();

    let (cause, next_step) = guidance_of(&output.stderr);
    assert!(!next_step.contains("retire the check"), "{next_step}");
    assert!(!cause.contains("revised check"), "{cause}");
}

#[test]
fn it_should_keep_the_revised_check_cause_on_the_load_refusal() {
    let p = loosened_check_project();

    let output = omhl_in(&p, &["load"]).run();

    let (cause, _) = guidance_of(&output.stderr);
    assert!(cause.contains("revised check"), "{cause}");
}

#[test]
fn it_should_give_the_type_change_refusal_a_cause_about_the_type() {
    let (p, base) = feat_work_item_project();

    let output = change_type_to_chore(&p, &base, &[]);

    let (cause, _) = guidance_of(&output.stderr);
    assert!(cause.contains("type"), "{cause}");
}

#[test]
fn it_should_name_the_owner_and_owner_override_in_the_type_change_next_step() {
    let (p, base) = feat_work_item_project();

    let output = change_type_to_chore(&p, &base, &[]);

    let (_, next_step) = guidance_of(&output.stderr);
    assert!(next_step.contains("owner"), "{next_step}");
    assert!(next_step.contains("--owner-override"), "{next_step}");
}
