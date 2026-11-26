use crate::support;

use serde_json::{Value, json};
use support::child::{omhl, omhl_json};
use support::git::git_ok;
use support::omhl::work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";
const ANCHORING: &str = "checks.verify-review-comment-anchoring";
const NATIVE_CHECKS: [&str; 3] = [RUN_TESTS, CI_GREEN, ANCHORING];
const INVENTED_NATIVE: &str = "checks.native-from-the-future";
const BASE_LABELS: [&str; 3] = [
    "no-open-contradiction",
    "decisions-anchored",
    "anchors-confirmed",
];

/// O que o projeto mostra ao agente: o `render`, o `additionalContext` do `session-start` e as perguntas do `gate-pr`.
struct Surfaces {
    render: String,
    context: String,
    labels: Vec<String>,
}

// Apoio

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn surfaces(project: &Project) -> Surfaces {
    let run = |args: &[&str], stdin: &str| {
        let output = omhl_json(args)
            .cwd(project.dir())
            .xdg(project.xdg())
            .stdin(stdin)
            .run();
        assert_eq!(output.code, Some(0), "{args:?}: {}", output.stderr);

        json_line(&output.stdout)
    };
    run(&["append"], &open_work_item_request());
    let render = run(&["render"], "");
    let gate = run(&["gate-pr", W1], "");
    let hook_input = json!({
        "hook_event_name": "SessionStart",
        "session_id": "s1",
        "cwd": project.dir(),
        "source": "startup",
    });
    let hook = omhl(&["hook", "session-start"])
        .xdg(project.xdg())
        .stdin(hook_input.to_string())
        .run();
    assert_eq!(hook.code, Some(0), "stderr: {}", hook.stderr);

    Surfaces {
        render: render["text"].as_str().expect("a text").to_owned(),
        context: json_line(&hook.stdout)["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .expect("additionalContext is a string")
            .to_owned(),
        labels: gate["questions"]
            .as_array()
            .expect("the gate lists its questions")
            .iter()
            .map(|question| question["label"].as_str().expect("a label").to_owned())
            .collect(),
    }
}

fn open_work_item_request() -> String {
    json!({ "items": [{ "body": {
        "kind": "node", "id": W1, "type": "work-item", "data": work_item_data("t"),
    } }] })
    .to_string()
}

/// Os ids dos blocos da seção `## Checks`; as guidelines ficam na seção seguinte.
fn checks_section_blocks(text: &str) -> Vec<&str> {
    text.lines()
        .skip_while(|line| *line != "## Checks")
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| line.strip_prefix("### "))
        .collect()
}

/// Os asserts do projeto sem configuração: vazio é a prova; cada item é um motivo de falha.
fn violations(surfaces: &Surfaces) -> Vec<String> {
    let mut found = Vec::new();
    for (surface, text) in [
        ("render", &surfaces.render),
        ("additionalContext", &surfaces.context),
    ] {
        for id in NATIVE_CHECKS.iter().filter(|id| text.contains(**id)) {
            found.push(format!("{surface} cites {id}"));
        }
        let blocks = checks_section_blocks(text);
        if !blocks.is_empty() {
            found.push(format!("{surface} has blocks in ## Checks: {blocks:?}"));
        }
    }
    if surfaces.labels != BASE_LABELS {
        found.push(format!("gate-pr asks {:?}", surfaces.labels));
    }

    found
}

fn has(found: &[String], reason: &str) -> bool {
    found.iter().any(|violation| violation.starts_with(reason))
}

fn enabling(ids: &[&str]) -> Surfaces {
    let project = Project::with_git("acme");
    project.enable_natives(ids);
    git_ok(project.dir(), &["add", "-A"]);
    git_ok(project.dir(), &["commit", "-q", "-m", "enable the natives"]);

    surfaces(&project)
}

// CMP-014: o projeto sem configuração

#[test]
fn it_should_show_no_native_check_block_id_or_question_in_a_project_without_configuration() {
    let project = Project::with_git("acme");

    let found = violations(&surfaces(&project));

    assert_eq!(found, Vec::<String>::new());
}

// CA-014: os mesmos asserts falham com cada variante

#[test]
fn it_should_flag_the_render_and_the_context_when_a_native_check_is_enabled_alone() {
    for id in NATIVE_CHECKS {
        let found = violations(&enabling(&[id]));

        for surface in ["render", "additionalContext"] {
            assert!(
                has(&found, &format!("{surface} cites {id}")),
                "{id}: {found:?}"
            );
            assert!(
                has(&found, &format!("{surface} has blocks in ## Checks")),
                "{id}: {found:?}"
            );
        }
    }
}

#[test]
fn it_should_flag_the_gate_when_run_tests_or_ci_green_is_enabled() {
    for id in [RUN_TESTS, CI_GREEN] {
        let found = violations(&enabling(&[id]));

        assert!(has(&found, "gate-pr asks"), "{id}: {found:?}");
    }
}

#[test]
fn it_should_flag_a_block_and_a_question_of_a_native_id_that_does_not_exist() {
    let project = Project::with_git("acme");
    let clean = surfaces(&project);
    let with_block = |text: &str| {
        text.replacen(
            "## Checks\n",
            &format!("## Checks\n\n### {INVENTED_NATIVE}\n- blocking: true\n"),
            1,
        )
    };
    let mut labels = clean.labels.clone();
    labels.push(INVENTED_NATIVE.to_owned());
    let invented = Surfaces {
        render: with_block(&clean.render),
        context: with_block(&clean.context),
        labels,
    };

    let found = violations(&invented);

    assert!(has(&found, "render has blocks in ## Checks"), "{found:?}");
    assert!(
        has(&found, "additionalContext has blocks in ## Checks"),
        "{found:?}"
    );
    assert!(has(&found, "gate-pr asks"), "{found:?}");
}
