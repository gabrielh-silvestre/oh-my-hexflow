use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Backing, Finding, Rule, Scanner, defined_identifiers, finding, scan_repo};

fn set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn scanner() -> Scanner {
    let backing = Backing::from_code(
        "const WAIT_TIMEOUT: u64 = 250;\nconst MAX_BYTES: usize = 2048;\nlet retries = 30;\n",
    );

    let test_literals = [
        "## Seção guardada",
        "oh-my-hexflow:role-scribe",
        "## Divergência",
        "### A matriz por papel",
    ]
    .map(str::to_owned)
    .to_vec();

    Scanner::new(
        set(&["alpha", "beta", "gamma", "Delta"]),
        backing,
        test_literals,
    )
}

fn rust_rules(source: &str) -> Vec<(usize, Rule)> {
    scanner()
        .rust_findings("a.rs", source)
        .into_iter()
        .map(|found| (found.line, found.rule))
        .collect()
}

fn markdown_rules(source: &str) -> Vec<(usize, Rule)> {
    scanner()
        .markdown_findings("a.md", source)
        .into_iter()
        .map(|found| (found.line, found.rule))
        .collect()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a file path has a parent")).expect("create dirs");
    fs::write(path, content).expect("write file");
}

#[test]
fn it_should_reject_a_measure_without_backing() {
    let source = "/// Espera 30 segundos antes de tentar.\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::Measure)]);
}

#[test]
fn it_should_accept_a_measure_whose_number_appears_in_code() {
    let source = "/// Espera 250 ms antes de tentar.\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_not_back_a_time_measure_with_a_size_number() {
    let source = "/// Espera 2048 segundos antes de tentar.\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::Measure)]);
}

#[test]
fn it_should_accept_a_measure_backed_after_unit_conversion() {
    let source = "/// Limite de 2 KiB por linha.\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_check_hours_days_and_milliseconds() {
    let source = "/// A cada 24 horas.\n/// Por 3 dias.\n/// Em 250 milissegundos.\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::Measure), (2, Rule::Measure)]);
}

#[test]
fn it_should_not_back_a_measure_with_a_number_that_only_a_test_fixture_holds() {
    let root = tempfile::tempdir().expect("temp dir");
    write(
        root.path(),
        "crates/c/src/lib.rs",
        "/// Espera 77 segundos.\nfn f() {}\n",
    );
    write(
        root.path(),
        "crates/c/src/tests.rs",
        "const WAIT_SECS: u64 = 77;\n",
    );
    write(
        root.path(),
        "crates/c/tests/it.rs",
        "const WAIT_SECS: u64 = 77;\n",
    );

    let found = scan_repo(root.path());

    let expected = [finding("crates/c/src/lib.rs", 1, Rule::Measure)];
    assert_eq!(found, expected);
}

#[test]
fn it_should_reject_a_count_of_things_the_code_defines() {
    let source = "/// Define três comandos e 12 campos.\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::Count)]);
}

#[test]
fn it_should_accept_a_number_that_counts_nothing_the_code_defines() {
    let source = "/// Lê 3 arquivos e devolve os campos.\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_reject_three_defined_identifiers_on_one_line() {
    let source = "/// Aceita `alpha`, `beta` e `gamma`.\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::CopiedList)]);
}

#[test]
fn it_should_reject_an_enumeration_joined_by_slash_or_pipe() {
    let source = "/// Vale `alpha` / `beta` | `gamma`.\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::CopiedList)]);
}

#[test]
fn it_should_accept_defined_identifiers_mentioned_apart() {
    let source = "/// O `alpha` com `beta` aceita o campo `gamma` (`Delta`).\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_accept_two_defined_identifiers_and_undefined_ones() {
    let source = "/// Aceita `alpha`, `beta`, `omega` e `sigma`.\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_reject_a_contiguous_bullet_list_of_defined_identifiers() {
    let source = "# Título\n\n- `alpha`\n- `beta`;\n- `Delta`.\n\nFim.\n";

    let found = markdown_rules(source);

    assert_eq!(found, [(3, Rule::CopiedList)]);
}

#[test]
fn it_should_accept_bullets_that_only_start_with_an_identifier() {
    let source = "- `alpha`: faz um.\n- `beta`: faz dois.\n- `gamma`: faz três.\n";

    let found = markdown_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_accept_a_list_in_a_section_a_test_cites_by_title() {
    let source = "## Seção guardada\n\nVale `alpha`, `beta` e `gamma`.\n";

    let found = markdown_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_reject_the_same_list_in_a_section_no_test_cites() {
    let source = "## Outra seção\n\nVale `alpha`, `beta` e `gamma`.\n";

    let found = markdown_rules(source);

    assert_eq!(found, [(3, Rule::CopiedList)]);
}

#[test]
fn it_should_guard_the_subsections_of_a_cited_section() {
    let source = "## Seção guardada\n\n### Filha\n\nVale `alpha`, `beta` e `gamma`.\n";

    let found = markdown_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_end_the_guard_at_the_next_heading_of_the_same_level() {
    let source = "## Seção guardada\n\n## Outra seção\n\nVale `alpha`, `beta` e `gamma`.\n";

    let found = markdown_rules(source);

    assert_eq!(found, [(5, Rule::CopiedList)]);
}

#[test]
fn it_should_not_guard_a_single_term_title_that_a_literal_only_contains() {
    let source = "# oh-my-hexflow\n\nVale `alpha`, `beta` e `gamma`.\n";

    let found = markdown_rules(source);

    assert_eq!(found, [(3, Rule::CopiedList)]);
}

#[test]
fn it_should_not_guard_a_heading_of_another_level_than_the_literal() {
    let source = "### Divergência\n\nVale `alpha`, `beta` e `gamma`.\n";

    let found = markdown_rules(source);

    assert_eq!(found, [(3, Rule::CopiedList)]);
}

#[test]
fn it_should_guard_a_section_whose_whole_heading_line_is_a_literal() {
    let source = "### A matriz por papel\n\nVale `alpha`, `beta` e `gamma`.\n";

    let found = markdown_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_guard_by_the_string_literals_of_the_test_files_of_the_repository() {
    let root = tempfile::tempdir().expect("temp dir");
    let list = "Vale `alpha`, `beta` e `gamma`.\n";
    write(
        root.path(),
        "crates/c/src/lib.rs",
        "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n",
    );
    write(
        root.path(),
        "crates/c/tests/page.rs",
        "let _ = \"## Seção citada\";\n// \"Seção comentada\"\n",
    );
    write(
        root.path(),
        "docs/cited.md",
        &format!("## Seção citada\n\n{list}"),
    );
    write(
        root.path(),
        "docs/commented.md",
        &format!("## Seção comentada\n\n{list}"),
    );

    let found = scan_repo(root.path());

    let expected = [finding("docs/commented.md", 3, Rule::CopiedList)];
    assert_eq!(found, expected);
}

#[test]
fn it_should_not_join_bullets_separated_by_a_line() {
    let source = "- `alpha`\n- `beta`\n\n- `gamma`\n";

    let found = markdown_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_reject_a_doc_block_longer_than_three_lines() {
    let source = "/// a\n///\n/// b\n/// c\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::DocBlock)]);
}

#[test]
fn it_should_accept_a_doc_block_of_three_lines() {
    let source = "/// a\n/// b\n/// c\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_count_a_fenced_doctest_inside_the_doc_block_cap() {
    let source = "/// Frase.\n/// ```\n/// let a = 1;\n/// let b = 2;\n/// ```\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::DocBlock)]);
}

#[test]
fn it_should_reject_an_inner_doc_block_of_four_lines() {
    let source = "//! a\n//! b\n//! c\n//! d\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::DocBlock)]);
}

#[test]
fn it_should_give_the_same_findings_with_crlf_line_endings() {
    let source = "/// a\n/// b\n/// c\n/// 30 segundos\nfn f() {}\n";

    let with_crlf = rust_rules(&source.replace('\n', "\r\n"));

    assert_eq!(with_crlf, [(1, Rule::DocBlock), (4, Rule::Measure)]);
}

#[test]
fn it_should_give_the_same_markdown_findings_with_crlf_line_endings() {
    let source = "Espera 30 segundos.\n\n```\nEspera 40 segundos.\n```\n";

    let with_crlf = markdown_rules(&source.replace('\n', "\r\n"));

    assert_eq!(with_crlf, [(1, Rule::Measure)]);
}

#[test]
fn it_should_treat_tilde_and_four_backtick_fences_as_fences() {
    let source = "~~~\nEspera 40 segundos.\n~~~\n\n````\n```\nEspera 50 segundos.\n```\n````\n\nEspera 60 segundos.\n";

    let found = markdown_rules(source);

    assert_eq!(found, [(11, Rule::Measure)]);
}

#[test]
fn it_should_not_let_an_attribute_split_a_doc_block() {
    let source = "/// a\n/// b\n#[expect(\n    dead_code,\n    reason = \"x\"\n)]\n/// c\n/// d\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, [(1, Rule::DocBlock)]);
}

#[test]
fn it_should_not_join_outer_and_inner_doc_lines() {
    let source = "//! a\n//! b\n/// c\n/// d\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_ignore_doc_markers_inside_a_string_literal() {
    let source =
        "const X: &str = \"\n/// 30 segundos\n\";\nconst Y: &str = r#\"\n/// 40 segundos\n\"#;\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_ignore_plain_comments() {
    let source = "// Espera 30 segundos.\nfn f() {}\n";

    let found = rust_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_flag_a_measure_in_markdown_outside_fences_only() {
    let source = "Espera 30 segundos.\n\n```\nEspera 40 segundos.\n```\n";

    let found = markdown_rules(source);

    assert_eq!(found, [(1, Rule::Measure)]);
}

#[test]
fn it_should_not_read_a_section_number_in_a_heading_as_a_measure() {
    let source = "### 3.1 Núcleo: o domínio\n";

    let found = markdown_rules(source);

    assert_eq!(found, []);
}

#[test]
fn it_should_collect_definitions_and_enum_variants() {
    let source = "pub fn run() {}\nconst fn limit() {}\n// fn hidden() {}\nenum Mode {\n    Fast,\n    Slow { n: u8 },\n}\n";

    let names = defined_identifiers(source);

    assert_eq!(names, set(&["run", "limit", "Mode", "Fast", "Slow"]));
}

#[test]
fn it_should_collect_numbers_by_the_unit_family_of_their_line() {
    let source = "const TIMEOUT_SECS: u64 = 1_048;\n// timeout 999\nconst MAX_BYTES: u8 = 7; // 888\nlet n = 5;\n";

    let backing = Backing::from_code(source);

    assert_eq!(backing.time, set(&["1048"]));
    assert_eq!(backing.size, set(&["7"]));
    assert_eq!(backing.parallel, set(&[]));
}

#[test]
fn it_should_skip_the_files_the_sweep_leaves_out() {
    let root = tempfile::tempdir().expect("temp dir");
    let doc = "/// Espera 30 segundos.\nfn f() {}\n";
    write(root.path(), "crates/c/src/lib.rs", doc);
    write(root.path(), "crates/c/src/sub/tests.rs", doc);
    write(root.path(), "crates/c/src/tests/helper.rs", doc);
    write(root.path(), "crates/c/tests/e2e.rs", doc);
    write(root.path(), "crates/omhl/src/adapters/cli.rs", doc);
    write(root.path(), "docs/guide.md", "Espera 30 segundos.\n");
    write(root.path(), "docs/adr/0001.md", "Espera 30 segundos.\n");
    write(
        root.path(),
        "THIRD_PARTY_NOTICES.md",
        "Espera 30 segundos.\n",
    );

    let found = scan_repo(root.path());

    let expected = [
        finding("crates/c/src/lib.rs", 1, Rule::Measure),
        finding("docs/guide.md", 1, Rule::Measure),
    ];
    assert_eq!(found, expected);
}

#[test]
fn it_should_scan_the_contributing_page() {
    let root = tempfile::tempdir().expect("temp dir");
    write(root.path(), "CONTRIBUTING.md", "Espera 30 segundos.\n");

    let found = scan_repo(root.path());

    assert_eq!(found, [finding("CONTRIBUTING.md", 1, Rule::Measure)]);
}

#[test]
fn it_should_not_restate_the_code_in_the_repository_documentation() {
    let found = scan_repo(&repo_root());

    let lines: Vec<String> = found.iter().map(Finding::to_string).collect();
    assert!(
        lines.is_empty(),
        "documentation restates the code ({} findings):\n{}",
        lines.len(),
        lines.join("\n")
    );
}
