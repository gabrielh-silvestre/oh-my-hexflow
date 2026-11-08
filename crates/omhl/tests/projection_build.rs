mod support;

use hexlog::{NodeView, RelationView, SnapshotKind};
use omhl::projection::build::build;
use omhl::projection::{ProjectedFile, ProjectionState};
use serde_json::json;
use support::views::{NodeViewExt, RelationViewExt, node_view, relation_view};

const HEADER: &str = "<!-- omhl: generated file, do not edit -->";
const TRUNCATION_MARKER: &str = "<!-- omhl: truncated -->\n";
const FILE_MAX_BYTES: usize = 262_144;
const WORK_ITEMS_MAX: usize = 1_000;

const MINIMAL_GOLDEN: &str = "<!-- omhl: generated file, do not edit -->\n\
# Ship step 6\n\
\n\
- id: step6\n\
- type: work-item\n\
- status: open\n";

const RICH_GOLDEN: &str = r#"<!-- omhl: generated file, do not edit -->
# Ship step 6

- id: step6
- type: pull-request
- status: closed
- branch: "feat/step6"
- number: 7
- provider: "github"
- repo: "o/r"
- gate: pr-gate passed=true chain_head_seq=12 snapshot=git-commit:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

## Premises

### p1

- status: "assumed"
- text: "Base is main"

## Decisions

### d1

- evidence:
```json
[
  {
    "source": "plan",
    "summary": "D36"
  }
]
```
- text: "Use TOML"

## Attestations

### a1

- check: "checks.run-tests"
- ok: true
- outcome: "passed"
- snapshot: git-commit:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

## Relations

- answers: d1 -> p1
"#;

fn state(nodes: Vec<NodeView>, relations: Vec<RelationView>) -> ProjectionState {
    ProjectionState { nodes, relations }
}

fn built(nodes: Vec<NodeView>, relations: Vec<RelationView>) -> Vec<ProjectedFile> {
    build(&state(nodes, relations)).expect("the projection is built")
}

fn content<'f>(files: &'f [ProjectedFile], name: &str) -> &'f str {
    files
        .iter()
        .find(|file| file.name == name)
        .map(|file| file.content.as_str())
        .expect("the projected file exists")
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn markers(text: &str) -> Vec<&str> {
    text.lines().filter(|line| line.starts_with("- ")).collect()
}

fn node_headers(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("### "))
        .collect()
}

fn work_item(id: &str, title: &str) -> NodeView {
    node_view(id, "work-item", json!({ "title": title, "status": "open" }))
}

fn premise(id: &str, text: &str, owner: &str) -> NodeView {
    node_view(
        id,
        "premise",
        json!({ "text": text, "status": "assumed", "work_item": owner }),
    )
}

fn decision(id: &str, text: &str, owner: &str) -> NodeView {
    node_view(
        id,
        "decision",
        json!({
            "text": text,
            "evidence": [{ "source": "docs", "summary": "the docs say so" }],
            "work_item": owner,
        }),
    )
}

fn attestation(id: &str, owner: &str) -> NodeView {
    node_view(
        id,
        "attestation",
        json!({ "check": "checks.lint", "work_item": owner, "outcome": "passed", "ok": true }),
    )
    .snapshot(SnapshotKind::GitCommit, &"c".repeat(40))
}

fn long_line(index: usize) -> String {
    format!("línea {index:06} {}", "x".repeat(20))
}

fn mixed_nodes() -> Vec<NodeView> {
    vec![
        work_item("a", "First"),
        node_view(
            "b",
            "pull-request",
            json!({ "title": "Second", "status": "open", "repo": "o/r", "number": 2 }),
        ),
        premise("p1", "One", "a"),
        premise("p2", "Two", "a"),
        premise("p10", "Ten", "a"),
        decision("d1", "Choose", "a"),
        attestation("a1", "b"),
    ]
}

fn mixed_relations() -> Vec<RelationView> {
    vec![
        relation_view("answers", "d1", "p1"),
        relation_view("answers", "d1", "p2"),
        relation_view("supports", "a1", "p10"),
    ]
}

#[test]
fn it_should_return_no_files_for_an_empty_state() {
    let empty = state(Vec::new(), Vec::new());

    let result = build(&empty);

    assert_eq!(result, Ok(Vec::new()));
}

#[test]
fn it_should_project_one_file_per_work_item() {
    let nodes = vec![work_item("b", "Second"), work_item("a", "First")];

    let files = built(nodes, Vec::new());

    let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
    assert_eq!(names, ["a.md", "b.md"]);
}

#[test]
fn it_should_project_pull_requests_too() {
    let nodes = vec![node_view(
        "pr-7",
        "pull-request",
        json!({ "title": "Add gate", "status": "open" }),
    )];

    let files = built(nodes, Vec::new());

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "pr-7.md");
    assert!(lines(&files[0].content).contains(&"- type: pull-request"));
}

#[test]
fn it_should_start_with_the_generated_header() {
    let nodes = vec![work_item("a", "First")];

    let files = built(nodes, Vec::new());

    assert_eq!(lines(content(&files, "a.md"))[0], HEADER);
}

#[test]
fn it_should_match_the_minimal_golden() {
    let nodes = vec![work_item("step6", "Ship step 6")];

    let files = built(nodes, Vec::new());

    assert_eq!(content(&files, "step6.md"), MINIMAL_GOLDEN);
}

#[test]
fn it_should_match_the_rich_golden() {
    let commit = "a".repeat(40);
    let pull_request = node_view(
        "step6",
        "pull-request",
        json!({
            "title": "Ship step 6",
            "status": "closed",
            "provider": "github",
            "repo": "o/r",
            "number": 7,
            "branch": "feat/step6",
            "gate": {
                "name": "pr-gate",
                "passed": true,
                "chain_head_seq": 12,
                "snapshot": { "kind": "git-commit", "id": commit },
            },
        }),
    );
    let first_decision = node_view(
        "d1",
        "decision",
        json!({
            "text": "Use TOML",
            "evidence": [{ "source": "plan", "summary": "D36" }],
            "work_item": "step6",
        }),
    );
    let first_attestation = node_view(
        "a1",
        "attestation",
        json!({
            "check": "checks.run-tests",
            "work_item": "step6",
            "outcome": "passed",
            "ok": true,
        }),
    )
    .snapshot(SnapshotKind::GitCommit, &commit);
    let nodes = vec![
        pull_request,
        premise("p1", "Base is main", "step6"),
        first_decision,
        first_attestation,
    ];
    let relations = vec![relation_view("answers", "d1", "p1")];

    let files = built(nodes, relations);

    assert_eq!(content(&files, "step6.md"), RICH_GOLDEN);
}

#[test]
fn it_should_list_extra_work_item_fields_sorted() {
    let nodes = vec![node_view(
        "a",
        "pull-request",
        json!({
            "title": "Add gate",
            "status": "open",
            "repo": "o/r",
            "number": 3,
            "branch": "main",
        }),
    )];

    let files = built(nodes, Vec::new());

    assert_eq!(
        markers(content(&files, "a.md")),
        [
            "- id: a",
            "- type: pull-request",
            "- status: open",
            "- branch: \"main\"",
            "- number: 3",
            "- repo: \"o/r\"",
        ]
    );
}

#[test]
fn it_should_render_the_gate_line_last_in_the_header() {
    let closed = node_view(
        "a",
        "work-item",
        json!({
            "title": "Done",
            "status": "closed",
            "zzz": "last alphabetically",
            "gate": {
                "name": "g",
                "passed": false,
                "chain_head_seq": 5,
                "snapshot": { "kind": "git-commit", "id": "b".repeat(40) },
            },
        }),
    );
    let nodes = vec![closed, premise("p1", "One", "a")];

    let files = built(nodes, Vec::new());

    let text = lines(content(&files, "a.md"));
    let section = text
        .iter()
        .position(|line| *line == "## Premises")
        .expect("the premises section exists");
    let expected = format!(
        "- gate: g passed=false chain_head_seq=5 snapshot=git-commit:{}",
        "b".repeat(40)
    );
    assert_eq!(text[section - 2], expected);
}

#[test]
fn it_should_group_nodes_by_the_work_item_field() {
    let nodes = vec![
        work_item("a", "First"),
        work_item("b", "Second"),
        premise("p1", "Of a", "a"),
        premise("p2", "Of b", "b"),
    ];

    let files = built(nodes, Vec::new());

    assert_eq!(node_headers(content(&files, "a.md")), ["### p1"]);
    assert_eq!(node_headers(content(&files, "b.md")), ["### p2"]);
}

#[test]
fn it_should_ignore_checks_and_guidelines() {
    let nodes = vec![
        work_item("a", "First"),
        node_view("c1", "check", json!({ "how": "run", "blocking": true })),
        node_view(
            "g1",
            "guideline",
            json!({ "text": "Be kind", "work_item": "a" }),
        ),
    ];

    let files = built(nodes, Vec::new());

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "a.md");
    assert!(!files[0].content.contains("## "));
    assert!(!files[0].content.contains("### "));
}

#[test]
fn it_should_exclude_shadowed_nodes() {
    let nodes = vec![
        work_item("a", "First"),
        decision("d1", "Hidden", "a").shadowed(),
        decision("d2", "Shown", "a"),
    ];

    let files = built(nodes, Vec::new());

    assert_eq!(node_headers(content(&files, "a.md")), ["### d2"]);
}

#[test]
fn it_should_sort_nodes_by_id_within_sections() {
    let nodes = vec![
        work_item("a", "First"),
        premise("p2", "Two", "a"),
        premise("p1", "One", "a"),
        premise("p10", "Ten", "a"),
    ];

    let files = built(nodes, Vec::new());

    assert_eq!(
        node_headers(content(&files, "a.md")),
        ["### p1", "### p10", "### p2"]
    );
}

#[test]
fn it_should_put_a_relation_in_the_file_of_its_from_owner() {
    let nodes = vec![
        work_item("a", "First"),
        work_item("b", "Second"),
        premise("p1", "One", "a"),
        decision("d1", "Choose", "a"),
    ];
    let relations = vec![relation_view("answers", "d1", "p1")];

    let files = built(nodes, relations);

    assert!(lines(content(&files, "a.md")).contains(&"- answers: d1 -> p1"));
    assert!(!content(&files, "b.md").contains("answers"));
}

#[test]
fn it_should_drop_relations_with_a_retired_endpoint() {
    let nodes = vec![
        work_item("a", "First"),
        premise("p1", "One", "a"),
        decision("d1", "Choose", "a"),
        decision("d2", "Choose again", "a"),
        decision("d3", "Choose once more", "a"),
    ];
    let relations = vec![
        relation_view("answers", "d1", "p1").retired(),
        relation_view("answers", "d2", "p1").retired_to(),
        relation_view("answers", "d3", "p1").retired_from(),
    ];

    let files = built(nodes, relations);

    let text = content(&files, "a.md");
    assert!(!text.contains("## Relations"));
    assert!(!text.contains("- answers:"));
}

#[test]
fn it_should_fence_multiline_text_with_a_longer_fence() {
    let nodes = vec![
        work_item("a", "First"),
        premise("p1", "before\n```\nafter", "a"),
    ];

    let files = built(nodes, Vec::new());

    assert!(content(&files, "a.md").contains("- text:\n````text\nbefore\n```\nafter\n````\n"));
}

#[test]
fn it_should_normalize_cr_in_text() {
    let nodes = vec![work_item("a", "First"), premise("p1", "a\r\nb\rc", "a")];

    let files = built(nodes, Vec::new());

    assert!(content(&files, "a.md").contains("- text:\n```text\na\nb\nc\n```\n"));
}

#[test]
fn it_should_escape_control_characters() {
    let nodes = vec![
        work_item("a", "x\u{1b}\ty"),
        premise("p1", "a\u{7f}b\u{1b}c\u{85}d", "a"),
        premise("p2", "a\u{7f}b\n\u{1b}c", "a"),
    ];

    let files = built(nodes, Vec::new());

    let text = content(&files, "a.md");
    assert!(text.contains("# x\\u001b\\u0009y\n"));
    assert!(text.contains("- text: \"a\\u007fb\\u001bc\\u0085d\"\n"));
    assert!(text.contains("- text:\n```text\na\\u007fb\n\\u001bc\n```\n"));
    assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
}

#[test]
fn it_should_render_arrays_as_pretty_json_blocks() {
    let evidence = json!([
        { "source": "plan", "summary": "D36" },
        { "source": "docs", "summary": "the docs say so" },
    ]);
    let pretty = serde_json::to_string_pretty(&evidence).expect("the evidence is serializable");
    let first = node_view(
        "d1",
        "decision",
        json!({ "text": "Choose", "evidence": evidence, "work_item": "a" }),
    );
    let nodes = vec![work_item("a", "First"), first];

    let files = built(nodes, Vec::new());

    assert!(content(&files, "a.md").contains(&format!("- evidence:\n```json\n{pretty}\n```\n")));
}

#[test]
fn it_should_not_include_record_ids_seq_or_timestamps() {
    let plain = built(mixed_nodes(), mixed_relations());
    let varied_nodes = mixed_nodes()
        .into_iter()
        .map(|node| {
            node.record("test:0192a000-0000-7000-8000-00000000ff01")
                .seq(99)
                .at("2030-01-01T00:00:00.000Z")
                .author("someone-else")
        })
        .collect();
    let varied_relations = mixed_relations()
        .into_iter()
        .map(|relation| {
            relation
                .seq(99)
                .at("2030-01-01T00:00:00.000Z")
                .author("someone-else")
        })
        .collect();

    let varied = built(varied_nodes, varied_relations);

    assert!(content(&plain, "a.md").contains("### p1"));
    assert_eq!(plain, varied);
}

#[test]
fn it_should_be_deterministic_for_shuffled_input() {
    let original = built(mixed_nodes(), mixed_relations());
    let mut reversed_nodes = mixed_nodes();
    reversed_nodes.reverse();
    let mut reversed_relations = mixed_relations();
    reversed_relations.reverse();
    let mut rotated_nodes = mixed_nodes();
    rotated_nodes.rotate_left(3);
    let mut rotated_relations = mixed_relations();
    rotated_relations.rotate_left(1);

    let reversed = built(reversed_nodes, reversed_relations);
    let rotated = built(rotated_nodes, rotated_relations);

    assert_eq!(original, reversed);
    assert_eq!(original, rotated);
}

#[test]
fn it_should_truncate_a_file_over_256_kib_at_a_line_boundary() {
    let text = (0..9_000).map(long_line).collect::<Vec<_>>().join("\n");
    let nodes = vec![
        work_item("a", "First"),
        node_view("d1", "decision", json!({ "text": text, "work_item": "a" })),
    ];

    let files = built(nodes, Vec::new());

    let projected = content(&files, "a.md");
    let kept = projected
        .strip_suffix(TRUNCATION_MARKER)
        .expect("the file ends with the truncation marker");
    let kept_text_lines: Vec<&str> = kept
        .lines()
        .filter(|line| line.starts_with("línea "))
        .collect();
    assert!(projected.len() <= FILE_MAX_BYTES);
    assert!(kept.ends_with('\n'));
    assert!(!kept_text_lines.is_empty());
    assert!(
        kept_text_lines
            .iter()
            .enumerate()
            .all(|(index, line)| *line == long_line(index))
    );
    assert!(projected.len() + long_line(0).len() + 1 > FILE_MAX_BYTES);
}

#[test]
fn it_should_refuse_more_than_1000_work_items() {
    let nodes: Vec<NodeView> = (0..=WORK_ITEMS_MAX)
        .map(|index| work_item(&format!("wi-{index}"), "Title"))
        .collect();

    let error = build(&state(nodes, Vec::new())).expect_err("too many work items are refused");

    assert_eq!(error.code, "too-many-work-items");
}
