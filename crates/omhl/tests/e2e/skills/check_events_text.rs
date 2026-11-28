//! O texto do evento de cada check e do nativo com comando: cada fato mora numa página só e as outras ligam.
//! É texto de skill e de doc: o teste prova palavras-chave e links, não a frase inteira.

use std::fs;
use std::path::{Path, PathBuf};

use crate::skills::coverage::skills_dir;
use crate::skills::orchestration::read;
use crate::support::child::omhl;
use crate::support::repo_checks::repo_root;

const EVENTS: &str = "review/references/events.md";
const NATIVE_CHECKS: &str = "directives-flow/references/native-checks.md";
const CHECK_RUN: &str = "review/references/check-run.md";
const ATTEST: &str = "review/references/attest.md";
const BYPRODUCT: &str = "review/references/byproduct.md";
const EVENT_NAMES: [&str; 4] = [
    "before-commit",
    "before-push",
    "before-merge",
    "before-publishing-review-comments",
];
const DOC_PAGES: [&str; 2] = ["docs/mechanical-gates.md", "docs/directives-flow.md"];

fn lowercase(relative: &str) -> String {
    read(relative).to_lowercase()
}

fn read_repo(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} must exist and be readable: {error}"))
}

/// Os alvos relativos dos links `](alvo)` do texto, sem a âncora.
fn relative_links(text: &str) -> Vec<String> {
    text.split("](")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .map(|target| target.split('#').next().unwrap_or("").to_string())
        .filter(|target| !target.is_empty() && !target.contains("://"))
        .collect()
}

fn links_to(text: &str, page: &str) -> bool {
    relative_links(text)
        .iter()
        .any(|target| target.ends_with(page))
}

fn markdown_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("plugin dir is readable").flatten() {
        let path = entry.path();
        if path.is_dir() {
            markdown_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            found.push(path);
        }
    }
}

fn event_line_has(text: &str, event: &str, words: &[&str]) -> bool {
    let name = format!("`{event}`");
    text.lines()
        .any(|line| line.contains(&name) && words.iter().all(|word| line.contains(word)))
}

#[test]
fn it_should_have_the_events_reference_linked_from_the_review_skill() {
    let skill = read("review/SKILL.md");

    assert!(
        links_to(&skill, "references/events.md"),
        "review SKILL.md links references/events.md"
    );
}

#[test]
fn it_should_cite_the_four_event_names_in_the_events_reference() {
    let events = read(EVENTS);

    for name in EVENT_NAMES {
        assert!(events.contains(&format!("`{name}`")), "events cites {name}");
    }
}

#[test]
fn it_should_say_before_commit_is_the_pre_commit_and_pre_merge_commit_hooks() {
    let events = lowercase(EVENTS);

    assert!(event_line_has(
        &events,
        "before-commit",
        &["`pre-commit`", "`pre-merge-commit`"]
    ));
}

#[test]
fn it_should_say_before_push_is_the_pre_push_hook_required_by_gate_delivery_and_close() {
    let events = lowercase(EVENTS);

    assert!(event_line_has(
        &events,
        "before-push",
        &["`pre-push`", "`gate-delivery`", "`close`"]
    ));
}

#[test]
fn it_should_say_before_merge_is_run_by_hand_with_no_hook() {
    let events = lowercase(EVENTS);

    assert!(event_line_has(
        &events,
        "before-merge",
        &[
            "`omhl check run --event before-merge`",
            "by hand",
            "no hook"
        ]
    ));
}

#[test]
fn it_should_say_before_publishing_review_comments_is_an_agent_instruction_without_attestation() {
    let events = lowercase(EVENTS);

    assert!(event_line_has(
        &events,
        "before-publishing-review-comments",
        &["agent", "no attestation"]
    ));
}

#[test]
fn it_should_say_a_project_binds_a_check_to_any_event_and_an_invented_one_is_refused() {
    let events = lowercase(EVENTS);

    assert!(events.contains("binds"), "events says a project binds");
    assert!(
        events.contains("invented"),
        "events names the invented event"
    );
    assert!(events.contains("refused"), "events says it is refused");
}

#[test]
fn it_should_name_omhl_merge_only_in_the_review_skill_and_its_merge_page() {
    // int1 (CMP-033, EMD-026) dá ao `omhl merge` a página própria e a citação na skill review
    let allowed = [
        repo_root().join("plugin/skills/review/SKILL.md"),
        repo_root().join("plugin/skills/review/references/merge.md"),
    ];
    let mut pages = Vec::new();
    markdown_files(&repo_root().join("plugin"), &mut pages);

    let offenders: Vec<_> = pages
        .iter()
        .filter(|page| !allowed.contains(page))
        .filter(|page| fs::read_to_string(page).is_ok_and(|text| text.contains("omhl merge")))
        .collect();

    assert!(
        offenders.is_empty(),
        "pages naming omhl merge: {offenders:?}"
    );
}

#[test]
fn it_should_list_the_four_events_in_the_check_run_help() {
    let output = omhl(&["check", "run", "--help"]).run();

    for name in EVENT_NAMES {
        assert!(output.stdout.contains(name), "help lists {name}");
    }
}

#[test]
fn it_should_name_the_four_events_in_each_doc_page() {
    for page in DOC_PAGES {
        let doc = read_repo(page);

        for name in EVENT_NAMES {
            assert!(doc.contains(name), "{page} names {name}");
        }
    }
}

#[test]
fn it_should_have_a_table_row_per_event_in_each_doc_page() {
    for page in DOC_PAGES {
        let doc = read_repo(page);
        let rows: Vec<&str> = doc
            .lines()
            .filter(|line| line.trim_start().starts_with('|'))
            .collect();

        for name in EVENT_NAMES {
            let named = rows
                .iter()
                .filter(|row| row.contains(&format!("`{name}`")))
                .count();

            assert_eq!(named, 1, "{page} has one table row for {name}");
        }
    }
}

#[test]
fn it_should_link_the_events_reference_from_each_doc_page() {
    for page in DOC_PAGES {
        let doc = read_repo(page);

        assert!(
            doc.contains("](../plugin/skills/review/references/events.md"),
            "{page} links the events reference"
        );
    }
}

#[test]
fn it_should_not_copy_the_events_reference_sentences_into_the_doc_pages() {
    let events = read(EVENTS);
    let sentences: Vec<&str> = events
        .lines()
        .map(str::trim)
        .filter(|line| line.len() >= 40 && !line.starts_with('#'))
        .collect();

    for page in DOC_PAGES {
        let doc = read_repo(page);
        let copied: Vec<_> = sentences
            .iter()
            .filter(|line| doc.contains(**line))
            .collect();

        assert!(copied.is_empty(), "{page} copies: {copied:?}");
    }
}

#[test]
fn it_should_have_the_native_run_table_in_the_native_checks_reference() {
    let text = lowercase(NATIVE_CHECKS);

    for word in [
        "[native.run",
        "`checks.run-tests`",
        "`native-run-ignored`",
        "union",
    ] {
        assert!(text.contains(word), "native-checks carries {word}");
    }
}

#[test]
fn it_should_not_keep_the_old_native_claims_in_the_native_checks_reference() {
    let text = lowercase(NATIVE_CHECKS);

    for stale in ["stay on", "commit under review", "is not configurable"] {
        assert!(
            !text.contains(stale),
            "native-checks no longer says {stale}"
        );
    }
}

#[test]
fn it_should_have_what_check_run_does_with_the_two_declarations_in_check_run() {
    let text = read(CHECK_RUN);

    for word in [
        "attestations.evaluated.<head>.<tail>.<check>.<work_item>",
        "@head",
    ] {
        assert!(text.contains(word), "check-run carries {word}");
    }
}

#[test]
fn it_should_have_the_check_run_author_refusal_in_attest() {
    let text = read(ATTEST);

    for word in ["check-run-author", "HEAD", "by hand"] {
        assert!(text.contains(word), "attest carries {word}");
    }
}

#[test]
fn it_should_have_the_native_byproduct_in_the_byproduct_reference() {
    let text = read(BYPRODUCT);

    for word in ["omhl byproduct", "checks.run-tests"] {
        assert!(text.contains(word), "byproduct carries {word}");
    }
}

#[test]
fn it_should_keep_each_native_command_fact_in_one_page() {
    let facts = [
        ("[native.run", NATIVE_CHECKS),
        ("native-run-ignored", NATIVE_CHECKS),
        ("attestations.evaluated", CHECK_RUN),
        ("@head", CHECK_RUN),
        ("check-run-author", ATTEST),
    ];

    for (fact, owner) in facts {
        let holders: Vec<_> = [NATIVE_CHECKS, CHECK_RUN, ATTEST, BYPRODUCT]
            .into_iter()
            .filter(|page| read(page).contains(fact))
            .collect();

        assert_eq!(holders, [owner], "{fact} lives only in its owner page");
    }
}

#[test]
fn it_should_link_the_native_command_pages_to_each_other() {
    let links = [
        (NATIVE_CHECKS, ["check-run.md", "attest.md", "byproduct.md"]),
        (CHECK_RUN, ["native-checks.md", "attest.md", "byproduct.md"]),
        (ATTEST, ["native-checks.md", "check-run.md", "byproduct.md"]),
    ];

    for (page, targets) in links {
        let text = read(page);

        for target in targets {
            assert!(links_to(&text, target), "{page} links {target}");
        }
    }
}

#[test]
fn it_should_link_native_checks_from_the_directives_flow_skill() {
    let skill = read("directives-flow/SKILL.md");

    assert!(links_to(&skill, "references/native-checks.md"));
}

#[test]
fn it_should_resolve_every_relative_link_of_the_new_pages() {
    for page in [EVENTS, NATIVE_CHECKS, CHECK_RUN, ATTEST, BYPRODUCT] {
        let base = skills_dir().join(page);
        let text = read(page);

        for target in relative_links(&text) {
            let path = base.parent().expect("page has a parent").join(&target);

            assert!(path.exists(), "{page} links a missing file: {target}");
        }
    }
}
