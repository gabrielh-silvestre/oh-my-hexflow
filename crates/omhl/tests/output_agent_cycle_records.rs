mod support;

use regex::Regex;
use serde_json::Value;
use support::child::Output;
use support::cycle::{self, APPEND_KEYED, APPEND_ONE, APPEND_TWO, MANUAL_CHECK, State, WORK_ITEM};
use support::git::git_in;
use support::project::Project;

const RECORD_ID: &str = r"acme:[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}";
const NOTE: &str = "Totals match the bank export";

// ---------------------------------------------------------------- ajudantes

fn plain(project: Project) -> State {
    State {
        project,
        program: None,
    }
}

fn empty_db() -> State {
    plain(Project::new("acme"))
}

fn attest_state() -> State {
    plain(cycle::attest_repo())
}

fn with_format<'a>(format: Option<&'a str>, args: &[&'a str]) -> Vec<&'a str> {
    let flag = format.into_iter().flat_map(|format| ["--format", format]);

    flag.chain(args.iter().copied()).collect()
}

fn run(state: &State, format: Option<&str>, args: &[&str], stdin: &str) -> Output {
    state.command(&with_format(format, args)).stdin(stdin).run()
}

/// O stdout de um comando que tem de sair 0, sem nada no stderr.
fn printed(state: &State, format: Option<&str>, args: &[&str], stdin: &str) -> String {
    let output = run(state, format, args, stdin);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");

    output.stdout
}

/// O stderr de um comando que tem de sair 1, sem nada no stdout.
fn refused(state: &State, format: Option<&str>, args: &[&str], stdin: &str) -> String {
    let output = run(state, format, args, stdin);

    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(output.stdout, "");

    output.stderr
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

fn record_line(seq: u32) -> Regex {
    Regex::new(&format!("^- {RECORD_ID} · seq {seq}$")).expect("the record line pattern compiles")
}

/// O `Value` do `append` em JSON; a próxima chamada com a mesma `key` é um replay.
fn first_write(state: &State, request: &str) -> Value {
    parsed(&printed(state, Some("json"), &["append"], request))
}

fn ids_of(value: &Value) -> Vec<String> {
    value["records"]
        .as_array()
        .expect("records is a list")
        .iter()
        .map(|record| record["id"].as_str().expect("the id is text").to_owned())
        .collect()
}

fn head_of(state: &State) -> String {
    let output = git_in(state.project.dir(), &["rev-parse", "HEAD"]);

    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn attest_args<'a>(outcome: &'a str, rest: &[&'a str]) -> Vec<&'a str> {
    ["attest", MANUAL_CHECK, WORK_ITEM, "--outcome", outcome]
        .into_iter()
        .chain(rest.iter().copied())
        .collect()
}

// ---------------------------------------------------------------- CA-1: sem `--format`, com `human` e com `json`

/// Os seis testes de CA-1 de um comando: `agent` (sem flag), `human` e `json`, no sucesso e no erro.
macro_rules! default_formats {
    ($command:ident, $success:expr, $success_args:expr, $success_stdin:expr, $failure:expr, $failure_args:expr, $failure_stdin:expr) => {
        mod $command {
            use super::*;

            #[test]
            fn it_should_print_agent_text_for_the_success_when_no_format_is_given() {
                let state = $success;

                let output = run(&state, None, $success_args, $success_stdin);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_text_starting_with(&output.stdout, "✔");
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_human_text_for_the_success() {
                let state = $success;

                let output = run(&state, Some("human"), $success_args, $success_stdin);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_text_starting_with(&output.stdout, "✔");
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_a_single_json_line_for_the_success_with_format_json() {
                let state = $success;

                let output = run(&state, Some("json"), $success_args, $success_stdin);

                assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
                assert_single_json_line(&output.stdout);
                assert_eq!(output.stderr, "");
            }

            #[test]
            fn it_should_print_agent_error_text_when_no_format_is_given() {
                let state = $failure;

                let output = run(&state, None, $failure_args, $failure_stdin);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_text_starting_with(&output.stderr, "✘");
                assert_eq!(output.stdout, "");
            }

            #[test]
            fn it_should_print_human_error_text() {
                let state = $failure;

                let output = run(&state, Some("human"), $failure_args, $failure_stdin);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_text_starting_with(&output.stderr, "✘");
                assert_eq!(output.stdout, "");
            }

            #[test]
            fn it_should_print_a_single_json_error_line_with_format_json() {
                let state = $failure;

                let output = run(&state, Some("json"), $failure_args, $failure_stdin);

                assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
                assert_single_json_line(&output.stderr);
                assert_eq!(output.stdout, "");
            }
        }
    };
}

default_formats!(
    append,
    empty_db(),
    &["append"],
    APPEND_ONE,
    empty_db(),
    &["append"],
    cycle::APPEND_NOT_JSON
);
default_formats!(
    attest,
    attest_state(),
    &attest_args("passed", &[]),
    "",
    attest_state(),
    &[
        "attest",
        "checks.ghost-one",
        WORK_ITEM,
        "--outcome",
        "passed"
    ],
    ""
);

// ---------------------------------------------------------------- `append`

#[test]
fn it_should_count_the_records_in_the_append_header() {
    let state = empty_db();

    let text = printed(&state, None, &["append"], APPEND_TWO);

    assert_eq!(first_line(&text), "✔ append · 2 records");
}

#[test]
fn it_should_use_the_singular_in_the_append_header_for_one_record() {
    let state = empty_db();

    let text = printed(&state, None, &["append"], APPEND_ONE);

    assert_eq!(first_line(&text), "✔ append · 1 record");
}

#[test]
fn it_should_print_one_line_per_record_with_its_seq_and_whole_id() {
    let state = empty_db();

    let text = printed(&state, None, &["append"], APPEND_TWO);

    let lines = lines_of(&text);
    assert_eq!(lines.len(), 3, "{text}");
    assert!(record_line(1).is_match(lines[1]), "{text}");
    assert!(record_line(2).is_match(lines[2]), "{text}");
}

#[test]
fn it_should_print_the_same_ids_as_the_json_value_in_full() {
    let state = empty_db();
    let ids = ids_of(&first_write(&state, APPEND_KEYED));

    let text = printed(&state, None, &["append"], APPEND_KEYED);

    assert_eq!(lines_of(&text)[1], format!("- {} · seq 1", ids[0]));
}

#[test]
fn it_should_not_print_the_chain_head_in_the_append_text() {
    let state = empty_db();
    let value = first_write(&state, APPEND_KEYED);
    let hash = value["chain_head"]["hash"]
        .as_str()
        .expect("the hash is text");

    let text = printed(&state, None, &["append"], APPEND_KEYED);

    assert!(!text.contains(hash), "{text}");
}

#[test]
fn it_should_mark_the_append_header_replayed_when_the_key_already_wrote() {
    let state = empty_db();
    first_write(&state, APPEND_KEYED);

    let text = printed(&state, None, &["append"], APPEND_KEYED);

    assert_eq!(first_line(&text), "✔ append · 1 record · replayed");
}

#[test]
fn it_should_not_mark_the_append_header_replayed_on_the_first_write_with_a_key() {
    let state = empty_db();

    let text = printed(&state, None, &["append"], APPEND_KEYED);

    assert_eq!(first_line(&text), "✔ append · 1 record");
}

#[test]
fn it_should_separate_the_append_header_from_the_records_with_a_blank_line_in_human() {
    let state = empty_db();

    let text = printed(&state, Some("human"), &["append"], APPEND_TWO);

    let lines = lines_of(&text);
    assert_eq!(&lines[..2], ["✔ append · 2 records", ""], "{text}");
    assert!(record_line(1).is_match(lines[2]), "{text}");
    assert!(record_line(2).is_match(lines[3]), "{text}");
}

#[test]
fn it_should_mark_the_human_append_header_replayed_when_the_key_already_wrote() {
    let state = empty_db();
    first_write(&state, APPEND_KEYED);

    let text = printed(&state, Some("human"), &["append"], APPEND_KEYED);

    assert_eq!(first_line(&text), "✔ append · 1 record · replayed");
}

#[test]
fn it_should_print_the_code_and_message_when_the_append_input_is_not_json() {
    let state = empty_db();

    let text = refused(&state, None, &["append"], cycle::APPEND_NOT_JSON);

    assert_eq!(
        text,
        "✘ input-invalid-json · input is not valid JSON at line 1 column 2\n"
    );
}

#[test]
fn it_should_point_at_the_missing_field_when_the_append_record_is_invalid() {
    let state = empty_db();

    let text = refused(&state, None, &["append"], cycle::APPEND_MISSING_TITLE);

    assert_eq!(
        lines_of(&text),
        [
            "✘ INVALID_RECORD · record content is invalid",
            "  - /items/0/body/data/title · required · required",
        ]
    );
}

#[test]
fn it_should_separate_the_append_error_header_from_the_detail_with_a_blank_line_in_human() {
    let state = empty_db();

    let text = refused(
        &state,
        Some("human"),
        &["append"],
        cycle::APPEND_MISSING_TITLE,
    );

    assert_eq!(
        lines_of(&text),
        [
            "✘ INVALID_RECORD · record content is invalid",
            "",
            "  - /items/0/body/data/title · required · required",
        ]
    );
}

#[test]
fn it_should_keep_the_append_json_bytes() {
    let state = empty_db();

    let text = printed(&state, Some("json"), &["append"], APPEND_ONE);

    let value = parsed(&text);
    let (head, record) = (&value["chain_head"], &value["records"][0]);
    assert_eq!(
        text,
        format!(
            r#"{{"chain_head":{{"hash":"{}","seq":{}}},"records":[{{"id":"{}","seq":{}}}],"replayed":false}}{}"#,
            head["hash"].as_str().expect("the hash is text"),
            head["seq"],
            record["id"].as_str().expect("the id is text"),
            record["seq"],
            "\n"
        )
    );
}

#[test]
fn it_should_keep_the_append_json_bytes_when_the_request_is_replayed() {
    let state = empty_db();
    let written = first_write(&state, APPEND_KEYED);
    let (head, record) = (&written["chain_head"], &written["records"][0]);

    let text = printed(&state, Some("json"), &["append"], APPEND_KEYED);

    assert_eq!(
        text,
        format!(
            r#"{{"chain_head":{{"hash":"{}","seq":{}}},"records":[{{"id":"{}","seq":{}}}],"replayed":true}}{}"#,
            head["hash"].as_str().expect("the hash is text"),
            head["seq"],
            record["id"].as_str().expect("the id is text"),
            record["seq"],
            "\n"
        )
    );
}

#[test]
fn it_should_keep_the_append_error_json_bytes() {
    let state = empty_db();

    let text = refused(&state, Some("json"), &["append"], cycle::APPEND_NOT_JSON);

    assert_eq!(
        text,
        "{\"error\":{\"code\":\"input-invalid-json\",\"details\":[],\"message\":\"input is not valid JSON at line 1 column 2\"}}\n"
    );
}

// ---------------------------------------------------------------- `attest`

#[test]
fn it_should_print_the_attest_header_and_the_note_line() {
    let state = attest_state();
    let snapshot = head_of(&state);

    let text = printed(&state, None, &attest_args("passed", &["--note", NOTE]), "");

    assert_eq!(
        lines_of(&text),
        [
            format!(
                "✔ attest · {MANUAL_CHECK} · {WORK_ITEM} · passed · snapshot {}",
                &snapshot[..7]
            ),
            format!("  note: {NOTE}"),
        ]
    );
}

#[test]
fn it_should_omit_the_attest_note_line_when_no_note_is_given() {
    let state = attest_state();

    let text = printed(&state, None, &attest_args("passed", &[]), "");

    assert_eq!(lines_of(&text).len(), 1, "{text}");
}

#[test]
fn it_should_warn_in_the_attest_header_when_the_outcome_is_passed_with_warning() {
    let state = attest_state();
    let snapshot = head_of(&state);

    let text = printed(&state, None, &attest_args("passed_with_warning", &[]), "");

    assert_eq!(
        text,
        format!(
            "⚠ attest · {MANUAL_CHECK} · {WORK_ITEM} · passed_with_warning · snapshot {}\n",
            &snapshot[..7]
        )
    );
}

#[test]
fn it_should_warn_in_the_attest_header_when_the_outcome_is_failed() {
    let state = attest_state();
    let snapshot = head_of(&state);

    let text = printed(&state, None, &attest_args("failed", &[]), "");

    assert_eq!(
        text,
        format!(
            "⚠ attest · {MANUAL_CHECK} · {WORK_ITEM} · failed · snapshot {}\n",
            &snapshot[..7]
        )
    );
}

#[test]
fn it_should_not_print_the_attestation_id_or_the_chain_head_in_the_attest_text() {
    let state = attest_state();
    let value = parsed(&printed(
        &state,
        Some("json"),
        &attest_args("passed", &[]),
        "",
    ));
    let id = value["attestation"]["id"].as_str().expect("the id is text");
    let hash = value["chain_head"]["hash"]
        .as_str()
        .expect("the hash is text");

    let text = printed(&state, None, &attest_args("passed", &[]), "");

    assert!(!text.contains(id) && !text.contains(hash), "{text}");
}

#[test]
fn it_should_print_the_same_attest_text_when_the_same_outcome_is_attested_again() {
    let state = attest_state();
    let first = printed(&state, None, &attest_args("passed", &[]), "");

    let second = printed(&state, None, &attest_args("passed", &[]), "");

    assert_eq!(second, first);
}

#[test]
fn it_should_not_mark_the_attest_header_replayed() {
    let state = attest_state();
    printed(&state, None, &attest_args("passed", &[]), "");

    let text = printed(&state, None, &attest_args("passed", &[]), "");

    assert!(!text.contains("replayed"), "{text}");
}

#[test]
fn it_should_collapse_repeated_spaces_of_the_note_in_agent() {
    let state = attest_state();

    let text = printed(
        &state,
        None,
        &attest_args("passed", &["--note", "Totals   match   the export"]),
        "",
    );

    assert_eq!(lines_of(&text)[1], "  note: Totals match the export");
}

#[test]
fn it_should_keep_repeated_spaces_of_the_note_in_human() {
    let state = attest_state();

    let text = printed(
        &state,
        Some("human"),
        &attest_args("passed", &["--note", "Totals   match   the export"]),
        "",
    );

    assert_eq!(lines_of(&text)[1], "  note: Totals   match   the export");
}

#[test]
fn it_should_print_the_attest_error_with_its_pointer_in_agent() {
    let state = attest_state();

    let text = refused(
        &state,
        None,
        &[
            "attest",
            "checks.ghost-one",
            WORK_ITEM,
            "--outcome",
            "passed",
        ],
        "",
    );

    assert_eq!(
        lines_of(&text),
        [
            "✘ check-not-found · check-not-found at /check",
            "  - /check · check-not-found",
        ]
    );
}

#[test]
fn it_should_separate_the_attest_error_header_from_the_detail_with_a_blank_line_in_human() {
    let state = attest_state();

    let text = refused(
        &state,
        Some("human"),
        &[
            "attest",
            "checks.ghost-one",
            WORK_ITEM,
            "--outcome",
            "passed",
        ],
        "",
    );

    assert_eq!(
        lines_of(&text),
        [
            "✘ check-not-found · check-not-found at /check",
            "",
            "  - /check · check-not-found",
        ]
    );
}

#[test]
fn it_should_keep_the_attest_json_bytes() {
    let state = attest_state();
    let sha = head_of(&state);

    let text = printed(
        &state,
        Some("json"),
        &attest_args("passed", &["--note", NOTE]),
        "",
    );

    let value = parsed(&text);
    let (head, record) = (&value["chain_head"], &value["records"][0]);
    assert_eq!(
        text,
        format!(
            r#"{{"attestation":{{"check":"{MANUAL_CHECK}","id":"attestations.{}.{}.{MANUAL_CHECK}.{WORK_ITEM}","note":"{NOTE}","ok":true,"outcome":"passed","snapshot":{{"id":"{sha}","kind":"git-commit"}},"work_item":"{WORK_ITEM}"}},"chain_head":{{"hash":"{}","seq":{}}},"records":[{{"id":"{}","seq":{}}}],"replayed":false}}{}"#,
            &sha[..32],
            &sha[32..],
            head["hash"].as_str().expect("the hash is text"),
            head["seq"],
            record["id"].as_str().expect("the id is text"),
            record["seq"],
            "\n"
        )
    );
}

#[test]
fn it_should_keep_the_attest_json_bytes_when_the_outcome_is_failed() {
    let state = attest_state();
    let sha = head_of(&state);

    let text = printed(&state, Some("json"), &attest_args("failed", &[]), "");

    let value = parsed(&text);
    let (head, record) = (&value["chain_head"], &value["records"][0]);
    assert_eq!(
        text,
        format!(
            r#"{{"attestation":{{"check":"{MANUAL_CHECK}","id":"attestations.{}.{}.{MANUAL_CHECK}.{WORK_ITEM}","ok":false,"outcome":"failed","snapshot":{{"id":"{sha}","kind":"git-commit"}},"work_item":"{WORK_ITEM}"}},"chain_head":{{"hash":"{}","seq":{}}},"records":[{{"id":"{}","seq":{}}}],"replayed":false}}{}"#,
            &sha[..32],
            &sha[32..],
            head["hash"].as_str().expect("the hash is text"),
            head["seq"],
            record["id"].as_str().expect("the id is text"),
            record["seq"],
            "\n"
        )
    );
}
