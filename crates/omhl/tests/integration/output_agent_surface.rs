use crate::support;
use omhl::adapters::text::{CommandKey, deferred, renderer_for};
use serde_json::{Value, json};
use support::cycle;
use support::inproc::run_inproc;

// ---------------------------------------------------------------- fechamento (CA-8)

#[test]
fn it_should_give_a_renderer_to_exactly_the_commands_that_are_not_deferred() {
    let mismatched: Vec<CommandKey> = CommandKey::ALL
        .into_iter()
        .filter(|key| renderer_for(*key).is_none() != deferred(*key))
        .collect();

    assert_eq!(mismatched, []);
}

#[test]
fn it_should_defer_exactly_the_five_machine_read_commands() {
    let deferred_keys: Vec<CommandKey> = CommandKey::ALL
        .into_iter()
        .filter(|key| deferred(*key))
        .collect();

    assert_eq!(
        deferred_keys,
        [
            CommandKey::GateWorktree,
            CommandKey::HooksPostCommit,
            CommandKey::HookSessionStart,
            CommandKey::HookSpecLock,
            CommandKey::HookAgentRole,
        ]
    );
}

#[test]
fn it_should_give_a_renderer_to_twenty_one_of_the_twenty_six_commands() {
    let with_renderer = CommandKey::ALL
        .into_iter()
        .filter(|key| renderer_for(*key).is_some())
        .count();

    assert_eq!((with_renderer, CommandKey::ALL.len()), (21, 26));
}

// ---------------------------------------------------------------- chaves do `load` lidas pelo hook (CA-7 iv)

// O texto das notas que o hook `session-start` monta com elas está em `tests/hook.rs` (`PENDING_NOTE`, `ORPHAN_NOTE`).

fn load_report() -> Value {
    let project = cycle::load_full();
    let run = run_inproc(&project, &["--format", "json", "load"], "");

    serde_json::from_str(run.stdout.trim_end()).expect("the load prints JSON")
}

fn keys_of(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("the value is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();

    keys
}

#[test]
fn it_should_keep_exactly_the_ten_keys_of_the_load_report() {
    let report = load_report();

    assert_eq!(
        keys_of(&report),
        [
            "created",
            "dry_run",
            "orphan_relations",
            "orphans",
            "relations_created",
            "revised",
            "skipped",
            "tenant",
            "unchanged",
            "warnings",
        ]
    );
}

#[test]
fn it_should_keep_the_types_of_the_load_report_keys_the_session_hook_reads() {
    let report = load_report();

    let shapes = (
        report["tenant"].is_string(),
        [
            "created",
            "revised",
            "relations_created",
            "orphans",
            "warnings",
        ]
        .map(|key| {
            report[key]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        }),
    );

    assert_eq!(shapes, (true, [true; 5]));
}

#[test]
fn it_should_keep_the_ids_the_session_hook_counts_as_pending_and_orphan() {
    let report = load_report();

    assert_eq!(
        (
            &report["created"],
            &report["revised"],
            &report["relations_created"],
            &report["orphans"]
        ),
        (
            &json!(["checks.payroll-new"]),
            &json!(["checks.payroll-revise"]),
            &json!([{ "from": "checks.payroll-new", "to": "checks.payroll-keep" }]),
            &json!(["checks.payroll-orphan"]),
        )
    );
}

#[test]
fn it_should_keep_the_keys_of_each_warning_of_the_load_report() {
    let report = load_report();

    assert_eq!(keys_of(&report["warnings"][0]), ["code", "message", "path"]);
}
