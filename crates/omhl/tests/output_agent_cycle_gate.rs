mod support;

use omhl::adapters::text::{CommandKey, Layout, render_success};
use regex::Regex;
use serde_json::{Value, json};
use support::child::Output;
use support::cycle::{self, WORK_ITEM};
use support::project::Project;

const RECORD_ID: &str = r"acme:[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}";
const RESTS_ON_STEP: &str =
    "record a rests-on from the decision to a confirmed guideline or premise";
const CONFIRM_STEP: &str = "ask the human to confirm the target (premise: set status confirmed with confirmed_by) or rest the decision on a confirmed one";
const RETRO_STEP: &str =
    "propose one improvement per item to the owner and change nothing without the answer";
const DIRTY_ERROR: &str = "{\"error\":{\"code\":\"snapshot-dirty\",\"details\":[{\"code\":\"snapshot-dirty\",\"message\":\"snapshot-dirty\",\"path\":\"/snapshot\"}],\"message\":\"snapshot-dirty at /snapshot\"}}\n";

// ---------------------------------------------------------------- ajudantes

/// O repositório do ciclo; o `close` grava, então cada teste monta o seu.
fn run(project: &Project, format: Option<&str>, args: &[&str]) -> Output {
    let flag = format.into_iter().flat_map(|format| ["--format", format]);
    let args: Vec<&str> = flag.chain(args.iter().copied()).collect();

    support::child::omhl(&args)
        .cwd(project.dir())
        .xdg(project.xdg())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .run()
}

/// O stdout de um comando que tem de sair 0, sem nada no stderr.
fn printed(project: &Project, format: Option<&str>, args: &[&str]) -> String {
    let output = run(project, format, args);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");

    output.stdout
}

/// O stderr de um comando que tem de sair 1, sem nada no stdout.
fn refused(project: &Project, format: Option<&str>, args: &[&str]) -> String {
    let output = run(project, format, args);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(output.stdout, "");

    output.stderr
}

fn gate_pr(project: &Project, format: Option<&str>) -> String {
    printed(project, format, &["gate-pr", WORK_ITEM])
}

fn close(project: &Project, format: Option<&str>) -> String {
    printed(project, format, &["close", WORK_ITEM])
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn assert_text_starting_with(text: &str, symbol: &str) {
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    assert!(first_line(text).starts_with(symbol), "{text}");
}

fn assert_single_json_line(text: &str) {
    assert_eq!(text.trim_end().lines().count(), 1, "{text}");
    serde_json::from_str::<Value>(text.trim_end()).expect("the line is JSON");
}

fn parsed(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the output is JSON")
}

fn record_line() -> Regex {
    Regex::new(&format!("^- {RECORD_ID} · seq \\d+$")).expect("the record line pattern compiles")
}

// ---------------------------------------------------------------- CA-1: sem `--format`, com `human` e com `json`

/// Os seis testes de CA-1 de um comando: `agent` (sem flag), `human` e `json`, no sucesso e no erro.
macro_rules! default_formats {
    ($command:ident, $success:expr, $success_args:expr, $failure:expr, $failure_args:expr) => {
        mod $command {
            use super::*;

            #[test]
            fn it_should_print_agent_text_for_the_success_when_no_format_is_given() {
                let project = $success;

                let output = run(&project, None, $success_args);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_text_starting_with(&output.stdout, "✔");
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_human_text_for_the_success() {
                let project = $success;

                let output = run(&project, Some("human"), $success_args);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_text_starting_with(&output.stdout, "✔");
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_a_single_json_line_for_the_success_with_format_json() {
                let project = $success;

                let output = run(&project, Some("json"), $success_args);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_single_json_line(&output.stdout);
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_agent_error_text_when_no_format_is_given() {
                let project = $failure;

                let output = run(&project, None, $failure_args);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_text_starting_with(&output.stderr, "✘");
                assert_eq!(output.stdout, "");
            }

            #[test]
            fn it_should_print_human_error_text() {
                let project = $failure;

                let output = run(&project, Some("human"), $failure_args);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_text_starting_with(&output.stderr, "✘");
                assert_eq!(output.stdout, "");
            }

            #[test]
            fn it_should_print_a_single_json_error_line_with_format_json() {
                let project = $failure;

                let output = run(&project, Some("json"), $failure_args);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_single_json_line(&output.stderr);
                assert_eq!(output.stdout, "");
            }
        }
    };
}

default_formats!(
    gate_pr_command,
    cycle::gate_approved(),
    &["gate-pr", WORK_ITEM],
    cycle::gate_dirty(),
    &["gate-pr", WORK_ITEM]
);
default_formats!(
    close_command,
    cycle::gate_approved(),
    &["close", WORK_ITEM],
    cycle::gate_dirty(),
    &["close", WORK_ITEM]
);

// ---------------------------------------------------------------- `gate-pr`

#[test]
fn it_should_print_one_line_per_question_with_the_counts_when_the_gate_passes() {
    let project = cycle::gate_approved();

    let text = gate_pr(&project, None);

    assert_eq!(
        lines_of(&text),
        [
            "✔ gate-pr · pull-request · passed · 7 questions",
            "✔ checks.run-tests · occurred · matched 1",
            "✔ checks.ci-green-before-merge · occurred · matched 1",
            "✔ checks.payroll-format · occurred · matched 1",
            "✔ spec-digest-match:checks.payroll-format · occurred · matched 1",
            "✔ no-open-contradiction · matched 0",
            "✔ decisions-anchored · anchored · matched 0",
            "✔ anchors-confirmed · anchored · matched 0",
        ]
    );
}

#[test]
fn it_should_print_the_failing_ids_and_the_next_step_of_each_failed_question_in_the_agent_layout() {
    let project = cycle::gate_failed();

    let text = gate_pr(&project, None);

    assert_eq!(
        lines_of(&text),
        [
            "✘ gate-pr · pull-request · not passed · 5 questions · 2 failed".to_owned(),
            "✔ checks.run-tests · occurred · matched 1".to_owned(),
            "✔ checks.ci-green-before-merge · occurred · matched 1".to_owned(),
            "✔ no-open-contradiction · matched 2".to_owned(),
            "✘ decisions-anchored · anchored · matched 2 · failing decisions.ledger-split"
                .to_owned(),
            "✘ anchors-confirmed · anchored · matched 2 · failing decisions.bonus-cap".to_owned(),
            format!("  next_step: {CONFIRM_STEP}"),
            format!("Next: {RESTS_ON_STEP}."),
        ]
    );
}

#[test]
fn it_should_open_a_human_block_for_each_header_list_and_closing_of_a_failed_gate() {
    let project = cycle::gate_failed();

    let text = gate_pr(&project, Some("human"));

    assert_eq!(
        text,
        format!(
            "✘ gate-pr · pull-request · not passed · 5 questions · 2 failed\n\n✔ checks.run-tests · occurred · matched 1\n✔ checks.ci-green-before-merge · occurred · matched 1\n✔ no-open-contradiction · matched 2\n✘ decisions-anchored · anchored · matched 2 · failing decisions.ledger-split\n\n✘ anchors-confirmed · anchored · matched 2 · failing decisions.bonus-cap\n  next_step: {CONFIRM_STEP}\n\nNext: {RESTS_ON_STEP}.\n"
        )
    );
}

#[test]
fn it_should_not_suggest_a_next_step_when_the_gate_passes() {
    let project = cycle::gate_approved();

    let text = gate_pr(&project, None);

    assert!(!text.contains("Next:"), "{text}");
}

#[test]
fn it_should_not_suggest_a_next_step_when_the_failed_questions_carry_none() {
    let project = cycle::gate_unattested();

    let text = gate_pr(&project, None);

    assert_eq!(
        lines_of(&text),
        [
            "✘ gate-pr · pull-request · not passed · 5 questions · 2 failed",
            "✘ checks.run-tests · occurred · matched 0",
            "✘ checks.ci-green-before-merge · occurred · matched 0",
            "✔ no-open-contradiction · matched 0",
            "✔ decisions-anchored · anchored · matched 0",
            "✔ anchors-confirmed · anchored · matched 0",
        ]
    );
}

#[test]
fn it_should_print_the_failed_gate_with_the_error_symbol_and_exit_zero() {
    let project = cycle::gate_failed();

    let output = run(&project, None, &["gate-pr", WORK_ITEM]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_text_starting_with(&output.stdout, "✘");
}

#[test]
fn it_should_not_cite_the_work_item_in_the_gate_header() {
    let project = cycle::gate_failed();

    let text = gate_pr(&project, None);

    assert!(!first_line(&text).contains("payroll"), "{text}");
}

#[test]
fn it_should_print_the_code_and_the_pointer_when_the_gate_refuses_a_dirty_tree() {
    let project = cycle::gate_dirty();

    let text = refused(&project, None, &["gate-pr", WORK_ITEM]);

    assert_eq!(
        lines_of(&text),
        [
            "✘ snapshot-dirty · snapshot-dirty at /snapshot",
            "  - /snapshot · snapshot-dirty"
        ]
    );
}

#[test]
fn it_should_keep_the_gate_pr_json_bytes() {
    let project = cycle::gate_failed();

    let text = gate_pr(&project, Some("json"));

    let head = &parsed(&text)["chain_head"];
    assert_eq!(
        text,
        format!(
            "{{\"chain_head\":{{\"hash\":\"{}\",\"seq\":{}}},\"gate\":\"pull-request\",\"passed\":false,\"questions\":[{{\"failing\":[],\"kind\":\"occurred\",\"label\":\"checks.run-tests\",\"matched\":1,\"omitted\":0,\"passed\":true}},{{\"failing\":[],\"kind\":\"occurred\",\"label\":\"checks.ci-green-before-merge\",\"matched\":1,\"omitted\":0,\"passed\":true}},{{\"failing\":[],\"kind\":\"no-open-contradiction\",\"label\":\"no-open-contradiction\",\"matched\":2,\"omitted\":0,\"passed\":true}},{{\"failing\":[\"decisions.ledger-split\"],\"kind\":\"anchored\",\"label\":\"decisions-anchored\",\"matched\":2,\"next_step\":\"{RESTS_ON_STEP}\",\"omitted\":0,\"passed\":false}},{{\"failing\":[\"decisions.bonus-cap\"],\"kind\":\"anchored\",\"label\":\"anchors-confirmed\",\"matched\":2,\"next_step\":\"{CONFIRM_STEP}\",\"omitted\":0,\"passed\":false}}]}}\n",
            head["hash"].as_str().expect("the hash is text"),
            head["seq"],
        )
    );
}

#[test]
fn it_should_keep_the_gate_pr_error_json_bytes() {
    let project = cycle::gate_dirty();

    let text = refused(&project, Some("json"), &["gate-pr", WORK_ITEM]);

    assert_eq!(text, DIRTY_ERROR);
}

// ---------------------------------------------------------------- `close`

#[test]
fn it_should_print_the_gate_the_closing_record_and_no_retro_when_the_gate_passes_and_there_is_nothing_to_report()
 {
    let project = cycle::gate_approved();

    let text = close(&project, None);

    let lines = lines_of(&text);
    assert_eq!(
        lines[..9],
        [
            "✔ close · pull-request · passed · 7 questions",
            "✔ checks.run-tests · occurred · matched 1",
            "✔ checks.ci-green-before-merge · occurred · matched 1",
            "✔ checks.payroll-format · occurred · matched 1",
            "✔ spec-digest-match:checks.payroll-format · occurred · matched 1",
            "✔ no-open-contradiction · matched 0",
            "✔ decisions-anchored · anchored · matched 0",
            "✔ anchors-confirmed · anchored · matched 0",
            "✔ closed · 1 record",
        ]
    );
    assert_eq!(lines.len(), 10, "{text}");
    assert!(record_line().is_match(lines[9]), "{text}");
}

#[test]
fn it_should_print_the_warning_symbol_and_every_next_step_as_a_field_when_the_gate_fails() {
    let project = cycle::gate_failed();

    let text = close(&project, None);

    let lines = lines_of(&text);
    assert_eq!(
        lines[..9],
        [
            "⚠ close · pull-request · not passed · 5 questions · 2 failed".to_owned(),
            "✔ checks.run-tests · occurred · matched 1".to_owned(),
            "✔ checks.ci-green-before-merge · occurred · matched 1".to_owned(),
            "✔ no-open-contradiction · matched 2".to_owned(),
            "✘ decisions-anchored · anchored · matched 2 · failing decisions.ledger-split"
                .to_owned(),
            format!("  next_step: {RESTS_ON_STEP}"),
            "✘ anchors-confirmed · anchored · matched 2 · failing decisions.bonus-cap".to_owned(),
            format!("  next_step: {CONFIRM_STEP}"),
            "✔ closed · 1 record".to_owned(),
        ]
    );
    assert_eq!(lines.len(), 10, "{text}");
}

#[test]
fn it_should_close_with_exit_zero_even_when_the_gate_fails() {
    let project = cycle::gate_failed();

    let output = run(&project, None, &["close", WORK_ITEM]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_text_starting_with(&output.stdout, "⚠");
}

#[test]
fn it_should_not_print_a_next_line_when_the_gate_fails_and_the_retro_is_null() {
    let project = cycle::gate_failed();

    let text = close(&project, None);

    assert!(!text.contains("Next:"), "{text}");
}

#[test]
fn it_should_print_the_retro_block_and_the_pending_proposal_when_the_retro_has_items() {
    let project = cycle::close_with_retro();

    let text = close(&project, None);

    let lines = lines_of(&text);
    assert_eq!(
        lines[10..],
        [
            "✔ retro · 2 divergences · 1 no-criterion gap".to_owned(),
            "- divergence · missing · open · divergences.overtime-cap".to_owned(),
            "- divergence · partial · open · divergences.rounding-drift".to_owned(),
            "- no criterion · gaps.holiday-calendar · answered by premises.calendar-source"
                .to_owned(),
            format!("Next: {RETRO_STEP}."),
        ]
    );
}

#[test]
fn it_should_open_a_human_block_for_the_gate_the_closing_record_and_the_retro() {
    let project = cycle::close_with_retro();

    let text = close(&project, Some("human"));

    let ids = Regex::new(&format!("{RECORD_ID} · seq \\d+")).expect("the id pattern compiles");
    assert_eq!(
        ids.replace(&text, "<id>"),
        format!(
            "✔ close · pull-request · passed · 7 questions\n\n✔ checks.run-tests · occurred · matched 1\n✔ checks.ci-green-before-merge · occurred · matched 1\n✔ checks.payroll-format · occurred · matched 1\n✔ spec-digest-match:checks.payroll-format · occurred · matched 1\n✔ no-open-contradiction · matched 0\n✔ decisions-anchored · anchored · matched 0\n✔ anchors-confirmed · anchored · matched 0\n\n✔ closed · 1 record\n\n- <id>\n\n✔ retro · 2 divergences · 1 no-criterion gap\n\n- divergence · missing · open · divergences.overtime-cap\n- divergence · partial · open · divergences.rounding-drift\n- no criterion · gaps.holiday-calendar · answered by premises.calendar-source\n\nNext: {RETRO_STEP}.\n"
        )
    );
}

#[test]
fn it_should_not_cite_the_work_item_in_the_close_header() {
    let project = cycle::gate_failed();

    let text = close(&project, None);

    assert!(!first_line(&text).contains("payroll"), "{text}");
}

#[test]
fn it_should_print_the_code_and_the_pointer_when_close_refuses_a_dirty_tree() {
    let project = cycle::gate_dirty();

    let text = refused(&project, None, &["close", WORK_ITEM]);

    assert_eq!(
        lines_of(&text),
        [
            "✘ snapshot-dirty · snapshot-dirty at /snapshot",
            "  - /snapshot · snapshot-dirty"
        ]
    );
}

#[test]
fn it_should_keep_the_close_json_bytes() {
    let project = cycle::gate_failed();

    let text = close(&project, Some("json"));

    let value = parsed(&text);
    let (written, read) = (&value["outcome"], &value["report"]["chain_head"]);
    assert_eq!(
        text,
        format!(
            "{{\"outcome\":{{\"chain_head\":{{\"hash\":\"{}\",\"seq\":{}}},\"records\":[{{\"id\":\"{}\",\"seq\":{}}}],\"replayed\":false}},\"report\":{{\"chain_head\":{{\"hash\":\"{}\",\"seq\":{}}},\"gate\":\"pull-request\",\"passed\":false,\"questions\":[{{\"failing\":[],\"kind\":\"occurred\",\"label\":\"checks.run-tests\",\"matched\":1,\"omitted\":0,\"passed\":true}},{{\"failing\":[],\"kind\":\"occurred\",\"label\":\"checks.ci-green-before-merge\",\"matched\":1,\"omitted\":0,\"passed\":true}},{{\"failing\":[],\"kind\":\"no-open-contradiction\",\"label\":\"no-open-contradiction\",\"matched\":2,\"omitted\":0,\"passed\":true}},{{\"failing\":[\"decisions.ledger-split\"],\"kind\":\"anchored\",\"label\":\"decisions-anchored\",\"matched\":2,\"next_step\":\"{RESTS_ON_STEP}\",\"omitted\":0,\"passed\":false}},{{\"failing\":[\"decisions.bonus-cap\"],\"kind\":\"anchored\",\"label\":\"anchors-confirmed\",\"matched\":2,\"next_step\":\"{CONFIRM_STEP}\",\"omitted\":0,\"passed\":false}}]}},\"retro\":null}}\n",
            written["chain_head"]["hash"]
                .as_str()
                .expect("the hash is text"),
            written["chain_head"]["seq"],
            written["records"][0]["id"]
                .as_str()
                .expect("the id is text"),
            written["records"][0]["seq"],
            read["hash"].as_str().expect("the hash is text"),
            read["seq"],
        )
    );
}

#[test]
fn it_should_keep_the_close_error_json_bytes() {
    let project = cycle::gate_dirty();

    let text = refused(&project, Some("json"), &["close", WORK_ITEM]);

    assert_eq!(text, DIRTY_ERROR);
}

// ---------------------------------------------------------------- `close` com retro sem itens (`Value` montado: o app não o produz)

/// O app só devolve retro com ao menos uma divergência ou um gap sem critério, então o retro sem itens vem à mão.
fn close_value_with_retro(retro: &Value) -> Value {
    json!({
        "outcome": {
            "chain_head": { "hash": "h-0", "seq": 4 },
            "records": [{ "id": "acme:0192a000-0000-7000-8000-000000000004", "seq": 4 }],
            "replayed": false,
        },
        "report": {
            "chain_head": { "hash": "h-1", "seq": 3 },
            "gate": "pull-request",
            "passed": true,
            "questions": [{
                "failing": [], "kind": "occurred", "label": "checks.run-tests",
                "matched": 1, "omitted": 0, "passed": true,
            }],
        },
        "retro": retro,
    })
}

fn empty_retro() -> Value {
    json!({
        "divergences": [],
        "amendments": { "approved": [], "pending": [] },
        "no_criterion": [],
        "open_gaps": [],
    })
}

#[test]
fn it_should_summarize_an_empty_retro_as_none_in_the_agent_layout() {
    let value = close_value_with_retro(&empty_retro());

    let text = render_success(CommandKey::Close, &value, Layout::Agent);

    assert!(text.contains("✔ retro · none"), "{text}");
}

#[test]
fn it_should_summarize_an_empty_retro_as_none_in_the_human_layout() {
    let value = close_value_with_retro(&empty_retro());

    let text = render_success(CommandKey::Close, &value, Layout::Human);

    assert!(text.contains("✔ retro · none"), "{text}");
}

#[test]
fn it_should_not_propose_an_improvement_when_the_retro_has_no_items() {
    let value = close_value_with_retro(&empty_retro());

    let text = render_success(CommandKey::Close, &value, Layout::Agent);

    assert!(!text.contains("Next:"), "{text}");
}

#[test]
fn it_should_omit_the_retro_block_when_the_retro_is_null() {
    let value = close_value_with_retro(&Value::Null);

    let text = render_success(CommandKey::Close, &value, Layout::Agent);

    assert!(!text.contains("retro"), "{text}");
}
