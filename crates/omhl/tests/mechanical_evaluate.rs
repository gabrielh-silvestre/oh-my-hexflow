mod support;

use omhl::mechanical::evaluate::{Observation, evaluate, evaluate_with_reason};
use omhl::mechanical::spec::{CheckSpec, Outcome};
use serde_json::{Value, json};
use support::omhl::check_data;

const CLIPPY_WARNING: &str = "    Checking omhl v0.1.0\nwarning: unused variable: `x`\n";

fn passed_on_zero() -> Value {
    json!([{"exit": 0, "outcome": "passed"}])
}

fn run_data(event: &str, expect: Value) -> Value {
    let mut data = check_data(event, "Run the linter.", true);
    data["run"] = json!(["cargo", "clippy"]);
    data["timeout_secs"] = json!(120);
    data["expect"] = expect;

    data
}

// As duas regras do check `lint` do relatório (`lint_data()` do contrato, só o `expect`).
fn lint_expect() -> Value {
    json!([
        {"exit": 0, "stderr": "(?m)^warning", "outcome": "passed_with_warning"},
        {"exit": 0, "outcome": "passed"},
    ])
}

fn spec(expect: Value) -> CheckSpec {
    CheckSpec::from_data(&run_data("before-pr", expect))
        .expect("decodes")
        .expect("has run")
}

fn observed(exit_code: Option<i32>) -> Observation<'static> {
    Observation {
        exit_code,
        stdout: "",
        stderr: "",
        timed_out: false,
        tree_changed: false,
        truncated: false,
    }
}

#[test]
fn it_should_pass_when_the_first_matching_rule_says_passed() {
    let spec = spec(passed_on_zero());
    let observation = observed(Some(0));

    let outcome = evaluate(&spec, &observation);

    assert_eq!(outcome, Outcome::Passed);
}

#[test]
fn it_should_return_the_outcome_of_the_first_matching_rule_even_if_a_later_rule_also_matches() {
    let warning_first = spec(lint_expect());
    let passed_first = spec(json!([
        {"exit": 0, "outcome": "passed"},
        {"exit": 0, "stderr": "(?m)^warning", "outcome": "passed_with_warning"},
    ]));
    let observation = Observation {
        stderr: CLIPPY_WARNING,
        ..observed(Some(0))
    };

    let first = evaluate(&warning_first, &observation);
    let second = evaluate(&passed_first, &observation);

    assert_eq!(first, Outcome::PassedWithWarning);
    assert_eq!(second, Outcome::Passed);
}

#[test]
fn it_should_pass_with_warning_when_stderr_matches_the_warning_regex() {
    let spec = spec(lint_expect());
    let with_warning = Observation {
        stderr: CLIPPY_WARNING,
        ..observed(Some(0))
    };
    let clean = Observation {
        stderr: "    Checking omhl v0.1.0\n    Finished\n",
        ..observed(Some(0))
    };

    let warned = evaluate(&spec, &with_warning);
    let finished = evaluate(&spec, &clean);

    assert_eq!(warned, Outcome::PassedWithWarning);
    assert_eq!(finished, Outcome::Passed);
}

#[test]
fn it_should_fail_when_no_rule_matches() {
    let spec = spec(lint_expect());
    let observation = Observation {
        stderr: "error: could not compile `omhl`\n",
        ..observed(Some(101))
    };

    let outcome = evaluate(&spec, &observation);

    assert_eq!(outcome, Outcome::Failed);
}

#[test]
fn it_should_fail_when_the_command_timed_out_even_if_an_exit_rule_would_match() {
    let spec = spec(passed_on_zero());
    let observation = Observation {
        timed_out: true,
        ..observed(Some(0))
    };

    let outcome = evaluate(&spec, &observation);

    assert_eq!(outcome, Outcome::Failed);
}

#[test]
fn it_should_fail_when_the_process_was_killed_by_a_signal_and_has_no_exit_code() {
    let spec = spec(json!([
        {"exit": 0, "outcome": "passed"},
        {"exit": 137, "outcome": "passed"},
    ]));
    let observation = observed(None);

    let outcome = evaluate(&spec, &observation);

    assert_eq!(outcome, Outcome::Failed);
}

#[test]
fn it_should_fail_when_the_tree_changed() {
    let spec = spec(passed_on_zero());
    let observation = Observation {
        tree_changed: true,
        ..observed(Some(0))
    };

    let outcome = evaluate(&spec, &observation);

    assert_eq!(outcome, Outcome::Failed);
}

#[test]
fn it_should_match_a_stdout_regex_only_when_the_exit_also_matches() {
    let spec = spec(json!([{"exit": 0, "stdout": "ok", "outcome": "passed"}]));
    let wrong_exit = Observation {
        stdout: "all ok\n",
        ..observed(Some(1))
    };
    let right_exit = Observation {
        stdout: "all ok\n",
        ..observed(Some(0))
    };

    let failed = evaluate(&spec, &wrong_exit);
    let passed = evaluate(&spec, &right_exit);

    assert_eq!(failed, Outcome::Failed);
    assert_eq!(passed, Outcome::Passed);
}

#[test]
fn it_should_treat_a_rule_without_regex_as_exit_only() {
    let spec = spec(passed_on_zero());
    let zero = Observation {
        stdout: "anything\n",
        stderr: "error: boom\n",
        ..observed(Some(0))
    };
    let two = Observation {
        stdout: "anything\n",
        stderr: "error: boom\n",
        ..observed(Some(2))
    };

    let passed = evaluate(&spec, &zero);
    let failed = evaluate(&spec, &two);

    assert_eq!(passed, Outcome::Passed);
    assert_eq!(failed, Outcome::Failed);
}

#[test]
fn it_should_match_a_multiline_regex_against_a_whole_stream() {
    let multiline = spec(json!([
        {"exit": 0, "stderr": "(?m)^warning", "outcome": "passed_with_warning"}
    ]));
    let anchored = spec(json!([
        {"exit": 0, "stderr": "^warning", "outcome": "passed_with_warning"}
    ]));
    let observation = Observation {
        stderr: CLIPPY_WARNING,
        ..observed(Some(0))
    };

    let with_flag = evaluate(&multiline, &observation);
    let without_flag = evaluate(&anchored, &observation);

    assert_eq!(with_flag, Outcome::PassedWithWarning);
    assert_eq!(without_flag, Outcome::Failed);
}

#[test]
fn it_should_fail_when_the_output_was_cut_and_a_stream_rule_would_decide() {
    let spec = spec(lint_expect());
    let observation = Observation {
        stderr: "    Finished\n",
        truncated: true,
        ..observed(Some(0))
    };

    let outcome = evaluate(&spec, &observation);

    assert_eq!(outcome, Outcome::Failed);
}

#[test]
fn it_should_keep_an_exit_only_rule_when_the_output_was_cut() {
    let spec = spec(passed_on_zero());
    let observation = Observation {
        truncated: true,
        ..observed(Some(0))
    };

    let outcome = evaluate(&spec, &observation);

    assert_eq!(outcome, Outcome::Passed);
}

#[test]
fn it_should_ignore_a_stream_rule_with_another_exit_when_the_output_was_cut() {
    let spec = spec(json!([
        {"exit": 1, "stderr": "(?m)^warning", "outcome": "passed_with_warning"},
        {"exit": 0, "outcome": "passed"},
    ]));
    let observation = Observation {
        truncated: true,
        ..observed(Some(0))
    };

    let outcome = evaluate(&spec, &observation);

    assert_eq!(outcome, Outcome::Passed);
}

#[test]
fn it_should_flag_the_unchecked_pattern_only_when_a_stream_rule_for_the_exit_was_skipped() {
    let spec = spec(json!([{"exit": 0, "stdout": "x$", "outcome": "passed"}]));
    let cut = |exit_code| Observation {
        truncated: true,
        ..observed(exit_code)
    };

    let skipped = evaluate_with_reason(&spec, &cut(Some(0)));
    let other_exit = evaluate_with_reason(&spec, &cut(Some(2)));

    assert!(skipped.pattern_unchecked);
    assert!(!other_exit.pattern_unchecked);
    assert_eq!(other_exit.outcome, Outcome::Failed);
}
