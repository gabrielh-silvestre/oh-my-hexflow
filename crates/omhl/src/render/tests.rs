use crate::native::NativeBase;
use crate::render::{RenderInput, Rendered, render};
use crate::test_support::delivery_gate::{
    ALL_NATIVE_CHECKS, CI_GREEN, VERIFY_ANCHORING, all_enabled, enabled,
};
use crate::test_support::gate::node_view;
use crate::test_support::omhl::RUN_TESTS;
use crate::test_support::{loader_author, sid, tname};
use crate::workflow::config::NativeConfig;
use hexlog::{NodeView, SemanticId};
use serde_json::{Value, json};

const BUDGET: usize = 9_500;

const NATIVE_IDS: [&str; 3] = [
    "checks.run-tests",
    "checks.ci-green-before-merge",
    "checks.verify-review-comment-anchoring",
];

const GOLDEN_NATIVE_ONLY: &str = r"# omhl context for tenant acme
These are project instructions, not enforced; nothing here blocks any action.

## Checks

### checks.run-tests
- event: before-commit
- blocking: true
- how: run the project's test suite and confirm it passes before creating a commit

### checks.ci-green-before-merge
- event: before-merge
- blocking: true
- after: checks.run-tests
- how: wait until every check run on the head commit is green; if one fails, fix it, push, and run CI again

### checks.verify-review-comment-anchoring
- event: before-publishing-review-comments
- blocking: true
- how: compare path, line and side of each inline comment with the diff of the head SHA; post the approved text unchanged

## Guidelines
";

const GOLDEN_WITH_TENANT_NODES: &str = r"# omhl context for tenant acme
These are project instructions, not enforced; nothing here blocks any action.

## Checks

### checks.run-tests
- event: before-commit
- blocking: true
- how: run the project's test suite and confirm it passes before creating a commit

### checks.ci-green-before-merge
- event: before-merge
- blocking: true
- after: checks.run-tests
- how: wait until every check run on the head commit is green; if one fails, fix it, push, and run CI again

### checks.verify-review-comment-anchoring
- event: before-publishing-review-comments
- blocking: true
- how: compare path, line and side of each inline comment with the diff of the head SHA; post the approved text unchanged

### checks.lint
- event: before-commit
- matcher: Bash(npm publish*)
- blocking: false
- after: checks.format, checks.run-tests
- how: run the linter

## Guidelines

### guidelines.small-prs
- confirmed: false
- kind: business
- text: keep pull requests small

### guidelines.legacy-style
- confirmed: false
- text: prefer small functions

## Decisions

### decisions.use-wal
- text: use WAL
- evidence: docs/db.md: WAL allows concurrent readers
- evidence: issue 12: locking incidents
- work_item: work-items.pr-1

## Premises

### premises.single-user
- mark: autonomous
- status: assumed
- text: one user per machine
";

#[derive(Default)]
struct Fixture {
    /// Sem valor vale `all_enabled()`.
    enabled: Option<NativeConfig>,
    notice: Option<&'static str>,
    checks: Vec<NodeView>,
    guidelines: Vec<NodeView>,
    decisions: Vec<NodeView>,
    premises: Vec<NodeView>,
    after: Vec<(SemanticId, SemanticId)>,
}

impl Fixture {
    fn rendered(&self, budget: usize) -> Rendered {
        let enabled = self.enabled.clone().unwrap_or_else(all_enabled);
        let input = RenderInput {
            tenant: "acme",
            native: &NativeBase,
            enabled: &enabled,
            notice: self.notice,
            checks: &self.checks,
            guidelines: &self.guidelines,
            decisions: &self.decisions,
            premises: &self.premises,
            after: &self.after,
        };

        render(&input, budget)
    }
}

fn native_after() -> Vec<(SemanticId, SemanticId)> {
    vec![pair("checks.ci-green-before-merge", "checks.run-tests")]
}

fn pair(from: &str, to: &str) -> (SemanticId, SemanticId) {
    (sid(from), sid(to))
}

fn node(id: &str, type_name: &str, data: Value) -> NodeView {
    NodeView {
        type_name: tname(type_name),
        data,
        ..node_view(id)
    }
}

fn check(id: &str) -> NodeView {
    node(
        id,
        "check",
        json!({
            "when": { "vocabulary": "closed", "event": "before-commit" },
            "how": "run it",
            "blocking": true,
        }),
    )
}

fn decision(id: &str, text: &str) -> NodeView {
    node(
        id,
        "decision",
        json!({ "text": text, "evidence": [{ "source": "docs/x.md", "summary": "why" }] }),
    )
}

fn premise(id: &str, text: &str) -> NodeView {
    node(id, "premise", json!({ "text": text, "status": "assumed" }))
}

fn headers(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("### "))
        .collect()
}

fn block<'t>(text: &'t str, id: &str) -> Vec<&'t str> {
    let header = format!("### {id}");

    text.lines()
        .skip_while(|line| *line != header)
        .take_while(|line| !line.is_empty())
        .collect()
}

fn footer(omitted: usize) -> String {
    format!("[{omitted} more nodes omitted for size; run `omhl list` to read them]")
}

fn golden_two_fixture() -> Fixture {
    let mut after = native_after();
    after.push(pair("checks.lint", "checks.run-tests"));
    after.push(pair("checks.lint", "checks.format"));

    Fixture {
        checks: vec![node(
            "checks.lint",
            "check",
            json!({
                "when": {
                    "vocabulary": "closed",
                    "event": "before-commit",
                    "matcher": "Bash(npm publish*)",
                },
                "how": "run the linter",
                "blocking": false,
            }),
        )],
        guidelines: vec![
            node(
                "guidelines.legacy-style",
                "guideline",
                json!({ "text": "prefer small functions" }),
            ),
            node(
                "guidelines.small-prs",
                "guideline",
                json!({ "text": "keep pull requests small", "kind": "business" }),
            ),
        ],
        decisions: vec![node(
            "decisions.use-wal",
            "decision",
            json!({
                "text": "use WAL",
                "evidence": [
                    { "source": "docs/db.md", "summary": "WAL allows concurrent readers" },
                    { "source": "issue 12", "summary": "locking incidents" },
                ],
                "work_item": "work-items.pr-1",
            }),
        )],
        premises: vec![node(
            "premises.single-user",
            "premise",
            json!({ "text": "one user per machine", "status": "assumed" }),
        )],
        after,
        ..Fixture::default()
    }
}

fn overflowing_fixture() -> Fixture {
    let long_text = "d".repeat(1_500);

    Fixture {
        decisions: (1..=300)
            .map(|n| decision(&format!("decisions.d{n:03}"), &long_text))
            .collect(),
        premises: (1..=40)
            .map(|n| premise(&format!("premises.p{n:02}"), &long_text))
            .collect(),
        ..Fixture::default()
    }
}

#[test]
fn it_should_match_the_golden_native_only_render() {
    let fixture = Fixture {
        after: native_after(),
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(rendered.text, GOLDEN_NATIVE_ONLY);
    assert_eq!(rendered.omitted, 0);
}

#[test]
fn it_should_match_the_golden_render_with_tenant_nodes() {
    let fixture = golden_two_fixture();

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(rendered.text, GOLDEN_WITH_TENANT_NODES);
    assert_eq!(rendered.omitted, 0);
}

#[test]
fn it_should_sort_tenant_nodes_by_semantic_id_regardless_of_seq() {
    let mut first_inserted = check("checks.b");
    first_inserted.seq = 1;
    let mut last_inserted = check("checks.a");
    last_inserted.seq = 2;
    let fixture = Fixture {
        checks: vec![first_inserted, last_inserted],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    let tenant_headers: Vec<&str> = headers(&rendered.text)
        .into_iter()
        .filter(|id| *id == "checks.a" || *id == "checks.b")
        .collect();
    assert_eq!(tenant_headers, ["checks.a", "checks.b"]);
}

#[test]
fn it_should_render_identical_bytes_for_any_insertion_order() {
    let nodes = [
        check("checks.a"),
        check("checks.b"),
        check("checks.c"),
        check("checks.d"),
    ];
    let permutations = [[0, 1, 2, 3], [3, 2, 1, 0], [2, 0, 3, 1]];

    let texts: Vec<String> = permutations
        .iter()
        .map(|order| Fixture {
            checks: order.iter().map(|i| nodes[*i].clone()).collect(),
            ..Fixture::default()
        })
        .map(|fixture| fixture.rendered(BUDGET).text)
        .collect();

    assert!(texts[0].contains("### checks.d"));
    assert_eq!(texts[0], texts[1]);
    assert_eq!(texts[0], texts[2]);
}

#[test]
fn it_should_sort_the_after_targets_of_a_check() {
    let fixture = Fixture {
        checks: vec![check("checks.x")],
        after: vec![pair("checks.x", "checks.z"), pair("checks.x", "checks.a")],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    assert!(
        block(&rendered.text, "checks.x").contains(&"- after: checks.a, checks.z"),
        "{}",
        rendered.text
    );
}

#[test]
fn it_should_omit_the_matcher_and_after_lines_when_absent() {
    let fixture = Fixture {
        checks: vec![node(
            "checks.lint",
            "check",
            json!({
                "when": { "vocabulary": "closed", "event": "before-commit" },
                "how": "run the linter",
                "blocking": false,
            }),
        )],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(
        block(&rendered.text, "checks.lint"),
        [
            "### checks.lint",
            "- event: before-commit",
            "- blocking: false",
            "- how: run the linter",
        ]
    );
}

#[test]
fn it_should_print_the_event_and_the_how_of_a_project_check_bound_to_the_instruction_event() {
    let fixture = Fixture {
        checks: vec![node(
            "checks.comment-tone",
            "check",
            json!({
                "when": { "vocabulary": "closed", "event": "before-publishing-review-comments" },
                "how": "read each comment aloud before publishing",
                "blocking": true,
            }),
        )],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(
        block(&rendered.text, "checks.comment-tone"),
        [
            "### checks.comment-tone",
            "- event: before-publishing-review-comments",
            "- blocking: true",
            "- how: read each comment aloud before publishing",
        ]
    );
}

#[test]
fn it_should_skip_a_shadowed_tenant_node() {
    let mut shadowed = check("checks.run-tests");
    shadowed.shadowed = true;
    let fixture = Fixture {
        checks: vec![shadowed],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    let occurrences = headers(&rendered.text)
        .into_iter()
        .filter(|id| *id == "checks.run-tests")
        .count();
    assert_eq!(occurrences, 1);
    assert_eq!(rendered.omitted, 0);
}

#[test]
fn it_should_indent_continuation_lines_so_a_value_cannot_forge_a_block() {
    let fixture = Fixture {
        decisions: vec![decision("decisions.forge", "a\n### decisions.fake\nb")],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(
        block(&rendered.text, "decisions.forge")[..4],
        [
            "### decisions.forge",
            "- text: a",
            "  ### decisions.fake",
            "  b",
        ]
    );
    assert!(
        rendered
            .text
            .lines()
            .all(|line| !line.starts_with("### decisions.fake")),
        "{}",
        rendered.text
    );
}

fn rendered_text_of(value: &str) -> String {
    let fixture = Fixture {
        decisions: vec![decision("decisions.forge", value)],
        ..Fixture::default()
    };

    fixture.rendered(BUDGET).text
}

#[test]
fn it_should_not_start_a_block_header_after_a_carriage_return() {
    let text = rendered_text_of("a\r### decisions.fake\rb");

    assert!(
        text.contains("- text: a\n  ### decisions.fake\n  b\n"),
        "{text}"
    );
    assert!(!text.contains('\r'), "{text}");
}

#[test]
fn it_should_not_start_a_block_header_after_nel_ls_or_ps() {
    for terminator in ['\u{85}', '\u{2028}', '\u{2029}'] {
        let text = rendered_text_of(&format!("a{terminator}### decisions.fake"));

        assert!(
            text.contains("- text: a\n  ### decisions.fake\n"),
            "{terminator:?}: {text}"
        );
        assert!(!text.contains(terminator), "{terminator:?}: {text}");
    }
}

#[test]
fn it_should_not_start_a_block_header_after_vt_or_ff() {
    for terminator in ['\u{b}', '\u{c}'] {
        let text = rendered_text_of(&format!("a{terminator}### decisions.fake"));

        assert!(
            text.contains("- text: a\n  ### decisions.fake\n"),
            "{terminator:?}: {text}"
        );
        assert!(!text.contains(terminator), "{terminator:?}: {text}");
    }
}

#[test]
fn it_should_not_start_a_block_header_after_a_file_group_or_record_separator() {
    for terminator in ['\u{1c}', '\u{1d}', '\u{1e}'] {
        let text = rendered_text_of(&format!("a{terminator}### decisions.fake"));

        assert!(
            text.contains("- text: a\n  ### decisions.fake\n"),
            "{terminator:?}: {text}"
        );
        assert!(!text.contains(terminator), "{terminator:?}: {text}");
    }
}

#[test]
fn it_should_treat_crlf_as_one_line_break() {
    let text = rendered_text_of("a\r\nb");

    assert!(text.contains("- text: a\n  b\n"), "{text}");
}

#[test]
fn it_should_keep_the_lf_behavior() {
    let text = rendered_text_of("a\nb\n\nc");

    assert!(text.contains("- text: a\n  b\n  \n  c\n"), "{text}");
}

#[test]
fn it_should_omit_nothing_within_the_budget() {
    let fixture = golden_two_fixture();

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(rendered.omitted, 0);
    assert!(!rendered.text.contains("more nodes omitted"));
}

#[test]
fn it_should_drop_tenant_nodes_from_the_end_of_the_display_order_over_the_budget() {
    let fixture = overflowing_fixture();

    let rendered = fixture.rendered(BUDGET);

    let shown = headers(&rendered.text);
    let shown_decisions: Vec<&str> = shown
        .iter()
        .copied()
        .filter(|id| id.starts_with("decisions."))
        .collect();
    let shown_premises = shown
        .iter()
        .filter(|id| id.starts_with("premises."))
        .count();
    let id_prefix: Vec<String> = (1..=shown_decisions.len())
        .map(|n| format!("decisions.d{n:03}"))
        .collect();
    assert!(rendered.text.chars().count() <= BUDGET);
    assert!(rendered.omitted > 0);
    assert!(shown_premises == 0 || shown_decisions.len() == 300);
    assert_eq!(shown_decisions, id_prefix);
    assert_eq!(
        rendered.omitted,
        340 - shown_decisions.len() - shown_premises
    );
}

#[test]
fn it_should_close_an_overflowing_text_with_the_footer_inside_the_budget() {
    let fixture = overflowing_fixture();

    let rendered = fixture.rendered(BUDGET);

    let expected_ending = format!("\n\n{}\n", footer(rendered.omitted));
    assert!(
        rendered.text.ends_with(&expected_ending),
        "{}",
        rendered.text
    );
    assert!(!rendered.text.ends_with(&format!("\n{expected_ending}")));
    assert!(rendered.text.chars().count() <= BUDGET);
}

#[test]
fn it_should_count_characters_not_bytes_and_never_split_one() {
    let fixture = Fixture {
        decisions: vec![decision("decisions.multibyte", &"ã🙂".repeat(60))],
        ..Fixture::default()
    };
    let exact_budget = fixture.rendered(BUDGET).text.chars().count();

    let fits = fixture.rendered(exact_budget);
    let one_short = fixture.rendered(exact_budget - 1);

    assert_eq!(fits.omitted, 0);
    assert_eq!(one_short.omitted, 1);
}

#[test]
fn it_should_omit_a_single_node_larger_than_the_budget_instead_of_cutting_it() {
    let fixture = Fixture {
        decisions: vec![decision("decisions.big", &"x".repeat(20_000))],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    assert!(!headers(&rendered.text).contains(&"decisions.big"));
    assert_eq!(rendered.omitted, 1);
    assert!(rendered.text.ends_with(&format!("{}\n", footer(1))));
}

#[test]
fn it_should_never_cut_native_nodes() {
    let fixture = Fixture {
        decisions: vec![decision("decisions.small", "keep it short")],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(100);

    let shown = headers(&rendered.text);
    assert!(NATIVE_IDS.iter().all(|id| shown.contains(id)));
    assert_eq!(rendered.omitted, 1);
    assert!(rendered.text.ends_with(&format!("{}\n", footer(1))));
}

#[test]
fn it_should_render_the_native_base_alone_in_under_4000_characters() {
    let fixture = Fixture {
        after: native_after(),
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    assert!(rendered.text.chars().count() < 4_000);
}

// Marcas de premissas e diretivas

fn confirmation() -> Value {
    json!({ "by": "person:ana", "question": "Is it right?", "answer": "Yes." })
}

fn kinded_premise(id: &str, kind: &str, status: &str) -> NodeView {
    let mut data = json!({
        "text": "the rule", "kind": kind, "status": status, "work_item": "work-items.w1",
    });
    if status == "confirmed" {
        data["confirmed_by"] = confirmation();
    }

    node(id, "premise", data)
}

fn directive(id: &str, data: Value) -> NodeView {
    node(id, "guideline", data)
}

fn kinded_guideline(id: &str, kind: &str, text: &str) -> NodeView {
    directive(id, json!({ "text": text, "kind": kind }))
}

fn section<'t>(text: &'t str, title: &str) -> &'t str {
    let Some((_, from_title)) = text.split_once(&format!("\n## {title}\n")) else {
        return "";
    };

    from_title.split("\n## ").next().unwrap_or_default()
}

fn rendered_premises(premises: Vec<NodeView>) -> Rendered {
    Fixture {
        premises,
        ..Fixture::default()
    }
    .rendered(BUDGET)
}

fn rendered_guidelines(guidelines: Vec<NodeView>) -> Rendered {
    Fixture {
        guidelines,
        ..Fixture::default()
    }
    .rendered(BUDGET)
}

#[test]
fn it_should_mark_an_unconfirmed_business_premise_as_pending() {
    let rendered = rendered_premises(vec![kinded_premise("premises.b", "business", "assumed")]);

    assert!(block(&rendered.text, "premises.b").contains(&"- mark: pending"));
}

#[test]
fn it_should_list_a_pending_premise_apart_from_the_binding_ones() {
    let rendered = rendered_premises(vec![kinded_premise("premises.b", "business", "assumed")]);

    assert!(section(&rendered.text, "Pending premises").contains("### premises.b\n"));
    assert!(!section(&rendered.text, "Premises").contains("premises.b"));
}

#[test]
fn it_should_mark_a_confirmed_business_premise_as_confirmed() {
    let rendered = rendered_premises(vec![kinded_premise("premises.b", "business", "confirmed")]);

    assert!(block(&rendered.text, "premises.b").contains(&"- mark: confirmed"));
}

#[test]
fn it_should_list_a_confirmed_premise_among_the_binding_ones() {
    let rendered = rendered_premises(vec![kinded_premise("premises.b", "business", "confirmed")]);

    assert!(section(&rendered.text, "Premises").contains("### premises.b\n"));
    assert!(!section(&rendered.text, "Pending premises").contains("premises.b"));
}

#[test]
fn it_should_mark_a_confirmed_technical_premise_as_confirmed() {
    let rendered = rendered_premises(vec![kinded_premise("premises.t", "technical", "confirmed")]);

    assert!(block(&rendered.text, "premises.t").contains(&"- mark: confirmed"));
}

/// Premissa de antes do PR.
fn legacy_confirmed_premise(id: &str, kind: &str) -> NodeView {
    node(
        id,
        "premise",
        json!({ "text": "the rule", "kind": kind, "status": "confirmed", "work_item": "work-items.w1" }),
    )
}

#[test]
fn it_should_mark_a_legacy_confirmed_technical_premise_as_autonomous() {
    let rendered = rendered_premises(vec![legacy_confirmed_premise("premises.t", "technical")]);

    assert!(block(&rendered.text, "premises.t").contains(&"- mark: autonomous"));
}

#[test]
fn it_should_mark_a_legacy_confirmed_business_premise_as_pending() {
    let rendered = rendered_premises(vec![legacy_confirmed_premise("premises.b", "business")]);

    assert!(block(&rendered.text, "premises.b").contains(&"- mark: pending"));
}

#[test]
fn it_should_list_a_legacy_confirmed_business_premise_among_the_pending_ones() {
    let rendered = rendered_premises(vec![legacy_confirmed_premise("premises.b", "business")]);

    assert!(section(&rendered.text, "Pending premises").contains("### premises.b\n"));
}

#[test]
fn it_should_mark_an_assumed_technical_premise_as_autonomous() {
    let rendered = rendered_premises(vec![kinded_premise("premises.t", "technical", "assumed")]);

    assert!(block(&rendered.text, "premises.t").contains(&"- mark: autonomous"));
}

#[test]
fn it_should_list_an_autonomous_premise_among_the_binding_ones() {
    let rendered = rendered_premises(vec![kinded_premise("premises.t", "technical", "assumed")]);

    assert!(section(&rendered.text, "Premises").contains("### premises.t\n"));
    assert!(!section(&rendered.text, "Pending premises").contains("premises.t"));
}

#[test]
fn it_should_mark_an_assumed_premise_without_kind_as_autonomous() {
    let rendered = rendered_premises(vec![premise("premises.old", "legacy rule")]);

    assert!(block(&rendered.text, "premises.old").contains(&"- mark: autonomous"));
}

#[test]
fn it_should_show_the_kind_of_a_premise() {
    let rendered = rendered_premises(vec![kinded_premise("premises.t", "technical", "assumed")]);

    assert!(block(&rendered.text, "premises.t").contains(&"- kind: technical"));
}

#[test]
fn it_should_omit_the_kind_line_of_a_premise_without_kind() {
    let rendered = rendered_premises(vec![premise("premises.old", "legacy rule")]);

    assert!(!rendered.text.contains("- kind:"));
}

#[test]
fn it_should_render_the_premise_lines_in_the_fixed_order_with_only_the_stored_text() {
    let rendered = rendered_premises(vec![kinded_premise("premises.b", "business", "assumed")]);

    assert_eq!(
        block(&rendered.text, "premises.b"),
        [
            "### premises.b",
            "- mark: pending",
            "- kind: business",
            "- status: assumed",
            "- text: the rule",
            "- work_item: work-items.w1",
        ]
    );
}

#[test]
fn it_should_not_create_the_pending_section_without_a_pending_premise() {
    let rendered = rendered_premises(vec![kinded_premise("premises.t", "technical", "assumed")]);

    assert!(!rendered.text.contains("## Pending premises"));
}

#[test]
fn it_should_sort_the_pending_premises_by_id() {
    let rendered = rendered_premises(vec![
        kinded_premise("premises.b2", "business", "assumed"),
        kinded_premise("premises.b1", "business", "assumed"),
    ]);

    assert_eq!(
        headers(section(&rendered.text, "Pending premises")),
        ["premises.b1", "premises.b2"]
    );
}

#[test]
fn it_should_skip_a_shadowed_pending_premise() {
    let mut shadowed = kinded_premise("premises.b", "business", "assumed");
    shadowed.shadowed = true;

    let rendered = rendered_premises(vec![shadowed]);

    assert!(!rendered.text.contains("premises.b"));
}

#[test]
fn it_should_mark_a_guideline_with_confirmed_by_as_confirmed() {
    let data = json!({ "text": "be brief", "confirmed_by": confirmation() });

    let rendered = rendered_guidelines(vec![directive("guidelines.brief", data)]);

    assert_eq!(
        block(&rendered.text, "guidelines.brief"),
        [
            "### guidelines.brief",
            "- confirmed: true",
            "- text: be brief"
        ]
    );
}

#[test]
fn it_should_mark_a_guideline_written_by_the_loader_as_confirmed() {
    let loaded = NodeView {
        author: loader_author(),
        ..directive("guidelines.tone", json!({ "text": "be kind" }))
    };

    let rendered = rendered_guidelines(vec![loaded]);

    assert!(block(&rendered.text, "guidelines.tone").contains(&"- confirmed: true"));
}

#[test]
fn it_should_not_mark_a_guideline_without_confirmation_as_confirmed() {
    let rendered = rendered_guidelines(vec![directive(
        "guidelines.loose",
        json!({ "text": "be loose" }),
    )]);

    assert_eq!(
        block(&rendered.text, "guidelines.loose"),
        [
            "### guidelines.loose",
            "- confirmed: false",
            "- text: be loose"
        ]
    );
}

#[test]
fn it_should_print_no_native_guideline_without_tenant_guidelines() {
    let rendered = Fixture::default().rendered(BUDGET);

    assert!(headers(section(&rendered.text, "Guidelines")).is_empty());
}

#[test]
fn it_should_show_the_kind_of_a_guideline() {
    let rendered = rendered_guidelines(vec![kinded_guideline(
        "guidelines.rule",
        "architecture",
        "layer it",
    )]);

    assert_eq!(
        block(&rendered.text, "guidelines.rule"),
        [
            "### guidelines.rule",
            "- confirmed: false",
            "- kind: architecture",
            "- text: layer it"
        ]
    );
}

#[test]
fn it_should_omit_the_kind_line_of_a_guideline_without_kind() {
    let rendered = rendered_guidelines(vec![directive(
        "guidelines.loose",
        json!({ "text": "be loose" }),
    )]);

    assert!(!rendered.text.contains("- kind:"));
}

#[test]
fn it_should_sort_guidelines_by_kind_business_first() {
    let rendered = rendered_guidelines(vec![
        directive("guidelines.c-legacy", json!({ "text": "old" })),
        kinded_guideline("guidelines.z-biz", "business", "rule"),
        kinded_guideline("guidelines.b-arch", "architecture", "rule"),
        kinded_guideline("guidelines.y-biz", "business", "rule"),
        kinded_guideline("guidelines.a-arch", "architecture", "rule"),
    ]);

    assert_eq!(
        headers(section(&rendered.text, "Guidelines")),
        [
            "guidelines.y-biz",
            "guidelines.z-biz",
            "guidelines.a-arch",
            "guidelines.b-arch",
            "guidelines.c-legacy",
        ]
    );
}

#[test]
fn it_should_cut_architecture_guidelines_before_business_ones_over_the_budget() {
    let long_text = "g".repeat(1_500);
    let mut guidelines = vec![
        kinded_guideline("guidelines.z1", "business", &long_text),
        kinded_guideline("guidelines.z2", "business", &long_text),
    ];
    guidelines.extend(
        (1..=6).map(|n| kinded_guideline(&format!("guidelines.a{n}"), "architecture", &long_text)),
    );

    let rendered = rendered_guidelines(guidelines);

    let shown = headers(&rendered.text);
    let shown_architecture = shown
        .iter()
        .filter(|id| id.starts_with("guidelines.a"))
        .count();
    assert!(rendered.text.chars().count() <= BUDGET);
    assert!(shown.contains(&"guidelines.z1") && shown.contains(&"guidelines.z2"));
    assert!(rendered.omitted > 0);
    assert_eq!(rendered.omitted, 6 - shown_architecture);
}

#[test]
fn it_should_keep_native_nodes_intact_when_guidelines_with_kind_are_cut() {
    let long_text = "g".repeat(1_500);
    let guidelines = (1..=8)
        .map(|n| kinded_guideline(&format!("guidelines.a{n}"), "architecture", &long_text))
        .collect();

    let rendered = rendered_guidelines(guidelines);

    let shown = headers(&rendered.text);
    assert!(rendered.omitted > 0);
    assert!(NATIVE_IDS.iter().all(|id| shown.contains(id)));
}

#[test]
fn it_should_render_the_native_base_in_under_4000_characters_with_the_marks() {
    let rendered = Fixture::default().rendered(BUDGET);

    assert!(rendered.text.chars().count() < 4_000);
}

fn late_premise(id: &str, status: &str, text: &str) -> NodeView {
    let mut late = kinded_premise(id, "business", status);
    late.data["origin"] = json!("late");
    late.data["text"] = json!(text);

    late
}

fn premise_headers(text: &str) -> Vec<&str> {
    headers(text)
        .into_iter()
        .filter(|id| id.starts_with("premises."))
        .collect()
}

fn section_titles(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix("## "))
        .collect()
}

#[test]
fn it_should_list_a_late_premise_under_late_premises_whatever_its_status() {
    let rendered = rendered_premises(vec![
        late_premise("premises.l-pending", "assumed", "the rule"),
        late_premise("premises.l-confirmed", "confirmed", "the rule"),
    ]);

    let late = section(&rendered.text, "Late premises");
    assert!(late.contains("### premises.l-pending\n"));
    assert!(late.contains("### premises.l-confirmed\n"));
    assert!(!section(&rendered.text, "Premises").contains("premises.l-"));
    assert!(!section(&rendered.text, "Pending premises").contains("premises.l-"));
}

#[test]
fn it_should_keep_the_mark_line_of_a_late_premise_saying_confirmed_or_pending() {
    let rendered = rendered_premises(vec![
        late_premise("premises.l-pending", "assumed", "the rule"),
        late_premise("premises.l-confirmed", "confirmed", "the rule"),
    ]);

    assert!(block(&rendered.text, "premises.l-pending").contains(&"- mark: pending"));
    assert!(block(&rendered.text, "premises.l-confirmed").contains(&"- mark: confirmed"));
}

#[test]
fn it_should_order_the_sections_decisions_premises_pending_premises_late_premises() {
    let fixture = Fixture {
        decisions: vec![decision("decisions.d1", "why")],
        premises: vec![
            late_premise("premises.l", "confirmed", "the rule"),
            kinded_premise("premises.b-pending", "business", "assumed"),
            kinded_premise("premises.b-main", "business", "confirmed"),
        ],
        ..Fixture::default()
    };

    let rendered = fixture.rendered(BUDGET);

    let titles = section_titles(&rendered.text);
    let tail = &titles[titles.len() - 4..];
    assert_eq!(
        tail,
        ["Decisions", "Premises", "Pending premises", "Late premises"]
    );
}

#[test]
fn it_should_list_the_confirmed_late_premises_before_the_pending_ones() {
    let rendered = rendered_premises(vec![
        late_premise("premises.a-pending", "assumed", "the rule"),
        late_premise("premises.b-confirmed", "confirmed", "the rule"),
    ]);

    assert_eq!(
        headers(section(&rendered.text, "Late premises")),
        ["premises.b-confirmed", "premises.a-pending"]
    );
}

#[test]
fn it_should_drop_the_pending_late_premises_before_the_confirmed_ones_over_the_budget() {
    let long_text = "l".repeat(3_000);
    let fixture = Fixture {
        premises: vec![
            late_premise("premises.l-confirmed", "confirmed", &long_text),
            late_premise("premises.l-pending", "assumed", &long_text),
        ],
        ..Fixture::default()
    };
    let full = fixture.rendered(usize::MAX);
    let budget = full.text.chars().count() - 1;

    let rendered = fixture.rendered(budget);

    assert_eq!(premise_headers(&rendered.text), ["premises.l-confirmed"]);
    assert_eq!(rendered.omitted, 1);
}

#[test]
fn it_should_drop_every_late_premise_before_any_main_one_over_the_budget() {
    let long_text = "m".repeat(3_000);
    let mut main = kinded_premise("premises.main", "business", "confirmed");
    main.data["text"] = json!(long_text);
    let mut pending = kinded_premise("premises.z-pending", "business", "assumed");
    pending.data["text"] = json!(long_text);
    let fixture = Fixture {
        premises: vec![
            main.clone(),
            pending.clone(),
            late_premise("premises.a-late", "confirmed", &long_text),
        ],
        ..Fixture::default()
    };
    let without_late = Fixture {
        premises: vec![main, pending],
        ..Fixture::default()
    };
    let budget = without_late.rendered(usize::MAX).text.chars().count() + 100;

    let rendered = fixture.rendered(budget);

    assert_eq!(
        premise_headers(&rendered.text),
        ["premises.main", "premises.z-pending"]
    );
}

#[test]
fn it_should_not_create_the_late_section_without_a_late_premise() {
    let rendered = rendered_premises(vec![kinded_premise("premises.b", "business", "confirmed")]);

    assert!(!rendered.text.contains("## Late premises"));
}

const GOLDEN_NO_NATIVE_CHECKS: &str = r"# omhl context for tenant acme
These are project instructions, not enforced; nothing here blocks any action.

## Checks

## Guidelines
";

fn with_enabled(ids: &[&'static str]) -> Fixture {
    Fixture {
        enabled: Some(enabled(ids)),
        after: native_after(),
        ..Fixture::default()
    }
}

fn native_headers(rendered: &Rendered) -> Vec<&str> {
    headers(&rendered.text)
        .into_iter()
        .filter(|id| ALL_NATIVE_CHECKS.contains(id))
        .collect()
}

fn lint_after(targets: &[&str]) -> Fixture {
    Fixture {
        enabled: Some(enabled(&[CI_GREEN])),
        checks: vec![check("checks.lint")],
        after: targets
            .iter()
            .map(|target| pair("checks.lint", target))
            .collect(),
        ..Fixture::default()
    }
}

#[test]
fn it_should_match_the_golden_render_without_native_checks_enabled() {
    let fixture = with_enabled(&[]);

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(rendered.text, GOLDEN_NO_NATIVE_CHECKS);
    assert_eq!(rendered.omitted, 0);
}

#[test]
fn it_should_print_only_the_run_tests_block_when_only_it_is_enabled() {
    let fixture = with_enabled(&[RUN_TESTS]);

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(native_headers(&rendered), [RUN_TESTS]);
}

#[test]
fn it_should_print_only_the_ci_green_block_when_only_it_is_enabled() {
    let fixture = with_enabled(&[CI_GREEN]);

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(native_headers(&rendered), [CI_GREEN]);
}

#[test]
fn it_should_print_only_the_anchoring_block_when_only_it_is_enabled() {
    let fixture = with_enabled(&[VERIFY_ANCHORING]);

    let rendered = fixture.rendered(BUDGET);

    assert_eq!(native_headers(&rendered), [VERIFY_ANCHORING]);
}

#[test]
fn it_should_print_the_after_line_of_ci_green_when_run_tests_is_enabled_too() {
    let fixture = with_enabled(&[RUN_TESTS, CI_GREEN]);

    let rendered = fixture.rendered(BUDGET);

    assert!(block(&rendered.text, CI_GREEN).contains(&"- after: checks.run-tests"));
}

#[test]
fn it_should_not_print_the_after_line_of_ci_green_when_only_it_is_enabled() {
    let fixture = with_enabled(&[CI_GREEN]);

    let rendered = fixture.rendered(BUDGET);

    assert!(!rendered.text.contains("- after:"), "{}", rendered.text);
}

#[test]
fn it_should_not_print_the_after_line_of_a_project_check_to_a_disabled_native_check() {
    let fixture = lint_after(&[RUN_TESTS]);

    let rendered = fixture.rendered(BUDGET);

    assert!(!block(&rendered.text, "checks.lint").contains(&"- after: checks.run-tests"));
    assert!(!rendered.text.contains("- after:"), "{}", rendered.text);
}

#[test]
fn it_should_print_the_after_line_of_a_project_check_to_an_enabled_native_check() {
    let fixture = lint_after(&[CI_GREEN]);

    let rendered = fixture.rendered(BUDGET);

    assert!(
        block(&rendered.text, "checks.lint").contains(&"- after: checks.ci-green-before-merge")
    );
}

#[test]
fn it_should_keep_only_the_enabled_targets_of_a_project_check() {
    let fixture = lint_after(&[RUN_TESTS, CI_GREEN, "checks.format"]);

    let rendered = fixture.rendered(BUDGET);

    assert!(
        block(&rendered.text, "checks.lint")
            .contains(&"- after: checks.ci-green-before-merge, checks.format"),
        "{}",
        rendered.text
    );
}

#[test]
fn it_should_print_the_notice_in_the_preamble_before_the_checks() {
    let fixture = Fixture {
        notice: Some("WARNING: the notice"),
        ..with_enabled(&[])
    };

    let rendered = fixture.rendered(BUDGET);

    assert!(
        rendered.text.starts_with(
            "# omhl context for tenant acme\nThese are project instructions, not enforced; nothing here blocks any action.\nWARNING: the notice\n\n## Checks\n"
        ),
        "{}",
        rendered.text
    );
}

#[test]
fn it_should_keep_the_notice_when_the_budget_cuts_nodes() {
    let fixture = Fixture {
        notice: Some("WARNING: the notice"),
        ..overflowing_fixture()
    };

    let rendered = fixture.rendered(BUDGET);

    assert!(rendered.omitted > 0);
    assert_eq!(
        rendered.text.matches("WARNING: the notice").count(),
        1,
        "{}",
        rendered.text
    );
}

#[test]
fn it_should_print_no_notice_without_one() {
    let fixture = with_enabled(&ALL_NATIVE_CHECKS);

    let rendered = fixture.rendered(BUDGET);

    assert!(!rendered.text.contains("WARNING"), "{}", rendered.text);
}
