use crate::support;

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{Value, json};
use support::child::omhl_json;
use support::repo_checks::repo_root;
use tempfile::TempDir;

const BOUNDARY_DOC: &str = "docs/plugin-and-project-boundary.md";
const GUIDELINE_FILE: &str = ".omhl/guidelines/boundary-classification.toml";
const CLASSES: [&str; 3] = [
    "Method",
    "Optional default configuration",
    "Exclusive to this repository",
];
const COLUMNS: [&str; 4] = ["Behavior", "Class", "Where it lives", "Today"];
const NAMED_BEHAVIORS: [&str; 13] = [
    "lock of the spec",
    "role hook",
    "session start",
    "native checks",
    "native guidelines",
    "branch name",
    "prefix",
    "`develop`",
    "hooks of `wt`",
    "git hooks",
    "github",
    "language",
    "conventions of this repository",
];
const TEXT_HOMES: [&str; 5] = [
    "plugin/skills/*/references/",
    "plugin/examples/",
    "docs/",
    "README.md",
    "CONTRIBUTING.md",
];
const CONTRIBUTING: &str = "CONTRIBUTING.md";
const MECHANICAL_GATES: &str = "docs/mechanical-gates.md";
const MOVED_HEADINGS: [&str; 3] = [
    "O custo do `pre-push`",
    "Testes unitários no gate, integração no CI",
    "A `main` com proteção do GitHub",
];
const REPOSITORY_ONLY_PHRASES: [&str; 2] = ["neste repositório", "máquina do dono"];
const ADR_FIELDS: [&str; 6] = [
    "Decision",
    "Drivers",
    "Alternatives considered",
    "Why chosen",
    "Consequences",
    "Follow-ups",
];
const ROLES_LINK: &str = "https://github.com/gabrielh-silvestre/oh-my-hexflow/blob/main/docs/directives-flow.md#a-matriz-por-papel";

// Apoio

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

fn section(page: &str, heading: &str) -> String {
    let start = format!("## {heading}");

    page.lines()
        .skip_while(|line| *line != start)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn cells(row: &str) -> Vec<String> {
    row.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// A tabela da seção `Classification`: o cabeçalho e as linhas de dados.
fn classification_table() -> (Vec<String>, Vec<Vec<String>>) {
    let page = read_repo_file(BOUNDARY_DOC);
    let rows: Vec<Vec<String>> = section(&page, "Classification")
        .lines()
        .filter(|line| line.starts_with('|'))
        .map(cells)
        .collect();
    let (header, body) = rows.split_first().expect("the section has a table");

    (header.clone(), body.iter().skip(1).cloned().collect())
}

fn guideline() -> toml::Table {
    toml::from_str(&read_repo_file(GUIDELINE_FILE)).expect("the guideline is valid TOML")
}

/// Os links relativos de Markdown quebrados (arquivo ou âncora) dos documentos que a regra do CMP-022 cobre.
fn broken_links(root: &Path) -> Vec<String> {
    let mut pages: Vec<PathBuf> = ["README.md", CONTRIBUTING, "GLOSSARY.md", "AGENTS.md"]
        .iter()
        .map(|name| root.join(name))
        .filter(|path| path.is_file())
        .collect();
    if let Ok(entries) = fs::read_dir(root.join("docs")) {
        let mut docs: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
            .collect();
        docs.sort();
        pages.extend(docs);
    }
    let link = Regex::new(r"\[[^\]]*\]\(([^)\s]+)\)").expect("the link pattern is valid");

    pages
        .iter()
        .flat_map(|page| {
            let text = fs::read_to_string(page).expect("the page is readable");
            let name = page
                .strip_prefix(root)
                .unwrap_or(page)
                .display()
                .to_string();
            let dir = page.parent().expect("a page has a folder");

            outside_fences(&text)
                .lines()
                .flat_map(|line| link.captures_iter(line))
                .map(|captured| captured[1].to_owned())
                .filter(|target| !target.contains("://") && !target.starts_with("mailto:"))
                .filter_map(|target| {
                    link_problem(page, dir, &target)
                        .map(|problem| format!("{name}: ({target}) {problem}"))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn link_problem(page: &Path, dir: &Path, target: &str) -> Option<&'static str> {
    let (path, anchor) = target.split_once('#').unwrap_or((target, ""));
    let file = if path.is_empty() {
        page.to_path_buf()
    } else {
        dir.join(path)
    };
    if !file.exists() {
        return Some("points to a file that does not exist");
    }
    if anchor.is_empty() {
        return None;
    }
    let text = fs::read_to_string(&file).unwrap_or_default();
    let anchors: Vec<String> = outside_fences(&text)
        .lines()
        .filter(|line| line.starts_with('#'))
        .map(|line| github_slug(line.trim_start_matches('#')))
        .collect();

    (!anchors.iter().any(|slug| slug == anchor))
        .then_some("points to an anchor that does not exist")
}

fn outside_fences(text: &str) -> String {
    let mut inside = false;

    text.lines()
        .filter(|line| {
            if line.trim_start().starts_with("```") {
                inside = !inside;
                return false;
            }
            !inside
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A regra do slug do GitHub: minúsculas, acentos mantidos, espaço vira hífen, pontuação e crase saem.
fn github_slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric() || matches!(character, ' ' | '-' | '_'))
        .map(|character| if character == ' ' { '-' } else { character })
        .collect()
}

/// Os filhos diretos de `docs/` que trazem texto só deste repositório.
fn docs_with_repository_text(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(root.join("docs"))
        .expect("docs exists")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .filter(|path| {
            let text = fs::read_to_string(path)
                .expect("the page is readable")
                .to_lowercase();
            REPOSITORY_ONLY_PHRASES
                .iter()
                .any(|phrase| text.contains(phrase))
        })
        .map(|path| {
            path.file_name()
                .expect("a page has a name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    found.sort();

    found
}

fn seeded_repository(files: &[(&str, &str)]) -> TempDir {
    let root = TempDir::new().expect("temporary repository is created");
    for (relative, content) in files {
        let path = root.path().join(relative);
        fs::create_dir_all(path.parent().expect("a file has a folder"))
            .expect("folders are created");
        fs::write(path, content).expect("file is written");
    }

    root
}

// Documento da fronteira

#[test]
fn it_should_have_the_three_classes_and_the_text_rule_as_sections() {
    let page = read_repo_file(BOUNDARY_DOC);

    let missing: Vec<&str> = [CLASSES.as_slice(), &["Where text lives"]]
        .concat()
        .into_iter()
        .filter(|heading| !page.lines().any(|line| line == format!("## {heading}")))
        .collect();

    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn it_should_classify_with_the_columns_behavior_class_where_it_lives_and_today() {
    let (header, _) = classification_table();

    assert_eq!(header, COLUMNS);
}

#[test]
fn it_should_give_every_row_of_the_table_one_of_the_three_classes() {
    let (_, rows) = classification_table();

    let outside: Vec<&String> = rows
        .iter()
        .map(|row| &row[1])
        .filter(|class| !CLASSES.contains(&class.as_str()))
        .collect();

    assert!(!rows.is_empty());
    assert_eq!(outside, Vec::<&String>::new());
}

#[test]
fn it_should_fill_the_today_cell_of_every_row() {
    let (_, rows) = classification_table();

    let empty: Vec<&String> = rows
        .iter()
        .map(|row| &row[0])
        .zip(rows.iter().map(|row| &row[3]))
        .filter(|(_, today)| today.is_empty())
        .map(|(behavior, _)| behavior)
        .collect();

    assert_eq!(empty, Vec::<&String>::new());
}

#[test]
fn it_should_cover_every_behavior_the_boundary_names() {
    let (_, rows) = classification_table();
    let behaviors: Vec<String> = rows.iter().map(|row| row[0].to_lowercase()).collect();

    let uncovered: Vec<&str> = NAMED_BEHAVIORS
        .into_iter()
        .filter(|wanted| !behaviors.iter().any(|behavior| behavior.contains(wanted)))
        .collect();

    assert_eq!(uncovered, Vec::<&str>::new());
}

#[test]
fn it_should_link_the_boundary_document_from_the_readme() {
    let readme = read_repo_file("README.md");

    assert!(readme.contains(&format!("]({BOUNDARY_DOC})")));
}

#[test]
fn it_should_name_the_five_homes_of_the_text_and_the_rule_that_the_other_side_links() {
    let page = read_repo_file(BOUNDARY_DOC);

    let rule = section(&page, "Where text lives");

    let missing: Vec<&str> = TEXT_HOMES
        .into_iter()
        .filter(|home| !rule.contains(&format!("`{home}`")))
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
    assert!(rule.contains("The other side links to it and does not copy it."));
}

// Leitores

#[test]
fn it_should_link_the_boundary_examples_architecture_and_agents_from_contributing() {
    let contributing = read_repo_file(CONTRIBUTING);

    let missing: Vec<&str> = [
        BOUNDARY_DOC,
        "plugin/examples/README.md",
        "docs/architecture.md",
        "AGENTS.md",
    ]
    .into_iter()
    .filter(|target| !contributing.contains(&format!("]({target})")))
    .collect();

    assert_eq!(missing, Vec::<&str>::new());
    assert!(contributing.contains(".omhl/guidelines/"));
}

#[test]
fn it_should_link_contributing_and_the_boundary_from_the_readme_and_keep_the_credits() {
    let readme = read_repo_file("README.md");

    assert!(readme.contains("](CONTRIBUTING.md)"));
    assert!(readme.contains(&format!("]({BOUNDARY_DOC})")));
    assert!(readme.lines().any(|line| line == "## Créditos"));
}

#[test]
fn it_should_keep_the_development_text_out_of_the_readme_and_in_contributing() {
    let readme = read_repo_file("README.md");
    let contributing = read_repo_file(CONTRIBUTING);

    assert!(!readme.contains("claude --plugin-dir"));
    assert!(!readme.lines().any(|line| line == "## Desenvolvimento"));
    assert!(
        contributing
            .lines()
            .any(|line| line == "## Desenvolvimento")
    );
    assert!(contributing.contains("claude --plugin-dir"));
}

#[test]
fn it_should_keep_the_repository_only_sections_out_of_the_mechanical_gates_page() {
    let page = read_repo_file(MECHANICAL_GATES);
    let contributing = read_repo_file(CONTRIBUTING);

    let misplaced: Vec<&str> = MOVED_HEADINGS
        .into_iter()
        .filter(|heading| {
            let line = format!("### {heading}");
            page.lines().any(|found| found == line)
                || !contributing.lines().any(|found| found == line)
        })
        .collect();

    assert_eq!(misplaced, Vec::<&str>::new());
}

#[test]
fn it_should_keep_the_repository_only_phrases_out_of_the_docs() {
    let found = docs_with_repository_text(&repo_root());

    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn it_should_report_a_seeded_doc_with_a_repository_only_phrase() {
    let root = seeded_repository(&[
        ("docs/x.md", "Vale neste repositório.\n"),
        ("docs/y.md", "Vale para todos.\n"),
    ]);

    let found = docs_with_repository_text(root.path());

    assert_eq!(found, vec!["x.md".to_owned()]);
}

#[test]
fn it_should_say_in_contributing_that_the_project_writes_no_more_adrs() {
    let contributing = read_repo_file(CONTRIBUTING);

    assert!(contributing.contains("não escreve mais ADRs"));
    assert!(contributing.contains("histórico congelado"));
}

#[test]
fn it_should_not_send_the_numeric_cut_to_an_adr_in_the_architecture_test() {
    let source = read_repo_file("crates/omhl/tests/integration/architecture.rs");

    assert!(!source.contains("0007-opcoes-da-cli"));
    assert!(!source.contains("Avaliados e mantidos"));
    assert!(source.contains("beside its entry of NUMERIC_CUTS_IN_APP"));
}

#[test]
fn it_should_keep_the_six_adr_fields_in_the_planner_response() {
    let planner = read_repo_file("plugin/agents/planner.md");

    let missing: Vec<&str> = ADR_FIELDS
        .into_iter()
        .filter(|field| !planner.contains(field))
        .collect();

    assert_eq!(missing, Vec::<&str>::new());
}

// Avisos de atualização

#[test]
fn it_should_open_the_update_notices_of_the_readme_with_the_notes_gitignore() {
    let readme = read_repo_file("README.md");

    let notices = section(&readme, "Avisos de atualização");

    for needle in [
        ".omhl/notes/",
        ".gitignore",
        "O hook cria",
        "`*`",
        "o `omhl` não o recria",
        "à mão",
        "anterior a esta mudança",
        "binário velho",
        "falhou no meio",
    ] {
        assert!(notices.contains(needle), "the notice lacks `{needle}`");
    }
}

#[test]
fn it_should_give_contributing_no_update_notice() {
    let contributing = read_repo_file(CONTRIBUTING);

    assert!(!contributing.contains("Avisos de atualização"));
}

#[test]
fn it_should_link_the_readme_update_notices_from_the_boundary_document() {
    let page = read_repo_file(BOUNDARY_DOC);

    assert!(page.contains("](../README.md#avisos-de-atualização)"));
}

// Links relativos

#[test]
fn it_should_resolve_every_relative_link_of_the_repository_documents() {
    let broken = broken_links(&repo_root());

    assert_eq!(broken, Vec::<String>::new());
}

#[test]
fn it_should_name_the_file_and_the_link_of_a_seeded_broken_link() {
    let root = seeded_repository(&[("README.md", "Veja [o guia](docs/missing.md).\n")]);

    let broken = broken_links(root.path());

    assert_eq!(
        broken,
        vec!["README.md: (docs/missing.md) points to a file that does not exist".to_owned()]
    );
}

#[test]
fn it_should_name_the_file_and_the_link_of_a_seeded_missing_anchor() {
    let root = seeded_repository(&[
        ("README.md", "Veja [a parte](docs/guide.md#nao-existe).\n"),
        ("docs/guide.md", "# Guia\n\n## Parte\n"),
    ]);

    let broken = broken_links(root.path());

    assert_eq!(
        broken,
        vec![
            "README.md: (docs/guide.md#nao-existe) points to an anchor that does not exist"
                .to_owned()
        ]
    );
}

#[test]
fn it_should_resolve_an_anchor_by_the_github_slug_rule() {
    let root = seeded_repository(&[
        (
            "README.md",
            "Veja [a parte](docs/guide.md#ação-omhl-rápida) e [a raiz](#título).\n\n# Título\n",
        ),
        ("docs/guide.md", "# Guia\n\n## Ação: `omhl` rápida\n"),
    ]);

    let broken = broken_links(root.path());

    assert_eq!(broken, Vec::<String>::new());
}

// Diretiva

#[test]
fn it_should_declare_the_boundary_guideline_as_architecture_with_an_english_text() {
    let table = guideline();

    let text = table["text"].as_str().expect("the guideline has a text");

    assert_eq!(table["kind"].as_str(), Some("architecture"));
    assert!(!text.trim().is_empty());
    assert!(text.is_ascii());
}

#[test]
fn it_should_cite_the_boundary_document_in_the_guideline() {
    let table = guideline();

    let text = table["text"].as_str().expect("the guideline has a text");

    assert!(text.contains(BOUNDARY_DOC));
}

#[test]
fn it_should_load_the_repository_guideline_in_the_dry_run_without_problems() {
    let xdg = TempDir::new().expect("data directory is created");

    let output = omhl_json(&["load", "--dry-run"])
        .cwd(&repo_root())
        .xdg(xdg.path())
        .run();

    let report: Value = serde_json::from_str(output.stdout.trim_end()).expect("JSON report");
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(report["skipped"], json!([]));
    assert_eq!(report["warnings"], json!([]));
    assert!(
        report["created"]
            .to_string()
            .contains("guidelines.boundary-classification")
    );
}

// Ponteiros

#[test]
fn it_should_point_the_directives_flow_skill_to_a_readme_of_examples_inside_the_plugin() {
    let skill = "plugin/skills/directives-flow/SKILL.md";
    let link = "../../examples/README.md";

    let target = repo_root().join("plugin/skills/directives-flow").join(link);

    assert!(read_repo_file(skill).contains(&format!("]({link})")));
    assert!(target.is_file());
    assert!(
        target
            .canonicalize()
            .expect("the target resolves")
            .starts_with(
                repo_root()
                    .join("plugin")
                    .canonicalize()
                    .expect("plugin resolves")
            )
    );
}

#[test]
fn it_should_link_the_examples_from_the_docs_that_describe_optional_files() {
    let pages = [
        "docs/directives-flow.md",
        "docs/extensions-and-projection.md",
    ];

    let without_link: Vec<&str> = pages
        .into_iter()
        .filter(|page| !read_repo_file(page).contains("plugin/examples/"))
        .collect();

    assert_eq!(without_link, Vec::<&str>::new());
}

#[test]
fn it_should_link_the_examples_from_the_readme() {
    let readme = read_repo_file("README.md");

    assert!(readme.contains("](plugin/examples/README.md)"));
}

#[test]
fn it_should_keep_the_absolute_link_of_the_roles_page_to_the_role_matrix() {
    let roles = read_repo_file("plugin/skills/directives-flow/references/roles.md");

    assert!(roles.contains(ROLES_LINK));
}

// Glossário

#[test]
fn it_should_define_boundary_in_the_glossary_with_an_avoid_line() {
    let glossary = read_repo_file("GLOSSARY.md");

    let entry = glossary
        .split("\n\n")
        .find(|block| block.starts_with("**Boundary**:"))
        .expect("the glossary has a Boundary entry");

    assert!(entry.lines().any(|line| line.starts_with("_Avoid_:")));
}
