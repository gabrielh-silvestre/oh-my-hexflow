use crate::support;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use ::omhl::guard::{ROLE_REFERENCE_COMMANDS, ROLE_REFERENCE_MATRIX};
use support::repo_checks::repo_root;

const FAILURE_TABLE_CODES: [&str; 17] = [
    "check-not-run",
    "attestation-refused",
    "attestation-not-recorded",
    "checks-failed",
    "unsupported-platform",
    "spawn-failed",
    "outcome-immutable",
    "command-changed-tree",
    "command-changed-worktree",
    "not-run",
    "spec-invalid",
    "spec-stale",
    "snapshot-tree-mismatch",
    "snapshot-dirty",
    "snapshot-not-head",
    "work-item-closed",
    "probe-failed",
];
const FAILURE_TABLE_KINDS: [&str; 11] = [
    "spawn-not-found",
    "spawn-permission-denied",
    "spawn-failed",
    "busy-exhausted",
    "guard-refused",
    "blob-failed",
    "probe-failed",
    "tree-changed",
    "push-to-main",
    "parent-gone",
    "spec-stale",
];
const MARK_LINE: &str =
    r#"printf 'omhl-develop-mark v1\n' > "$(git rev-parse --absolute-git-dir)/omhl-develop-mark""#;
const MECHANICAL_GATES_PAGE: &str = "docs/mechanical-gates.md";
const DEVELOP_MARK_COMMAND: &str = "plugin/commands/omhl-develop-mark.md";
const DIRECTIVES_FLOW_PAGE: &str = "docs/directives-flow.md";
const DIRECTIVES_FLOW_SKILL: &str = "plugin/skills/directives-flow/SKILL.md";
/// Códigos novos da entrega `work-items.dir0`, levantados de `git diff develop..HEAD -- crates/omhl/src`.
const DIRECTIVES_FLOW_CODES: [&str; 49] = [
    // S3: `RuleCode` em `crates/omhl/src/error.rs`
    "confirmation-required",
    "confirmation-invalid",
    // S4: `crates/omhl/src/workflow/config.rs` e `ports/workflow.rs`
    "invalid-workflow-config",
    // S5: construtores em `crates/omhl/src/app/error.rs`
    "worktree-gate-refused",
    "prefix-not-allowed",
    "main-dir-unknown",
    // S6a: construtores em `crates/omhl/src/app/error.rs`
    "spec-exists",
    "spec-slug-invalid",
    "spec-not-found",
    "spec-invalid",
    // S6b: `Code` em `crates/omhl/src/workflow/spec.rs`
    "sections-invalid",
    "section-empty",
    "section-without-items",
    "item-id-invalid",
    "item-id-duplicate",
    "file-too-long",
    "placeholder-found",
    "pending-marker-invalid",
    "divergence-ref-invalid",
    "correction-invalid",
    "correction-target-unknown",
    "amendment-defines-item",
    "amendment-name-invalid",
    "amendment-number-duplicate",
    // S6c: `Code` em `crates/omhl/src/workflow/spec.rs`
    "cited-id-unknown",
    "cited-id-unconfirmed",
    "business-premise-missing",
    "pending-gap-unresolved",
    // S10: `crates/omhl/src/workflow/lock.rs`
    "spec-frozen",
    "spec-in-linked-worktree",
    // S10b: `Code` e `amendment_divergence_mismatch` em `workflow/spec.rs`
    "amendment-divergence-unknown",
    "amendment-divergence-foreign",
    "amendment-divergence-closed",
    "amendment-correction-missing",
    "amendment-divergence-mismatch",
    // S10b: `crates/omhl/src/app/error.rs` e `app/spec.rs`
    "amendment-invalid",
    "amendment-not-found",
    "amendment-divergence-not-open",
    "amendment-work-item-mismatch",
    // E1: `kind` da diretiva e `rests-on`, em `crates/omhl/src/error.rs` e `extensions/parse.rs`
    "kind-missing",
    "kind-invalid",
    "rests-on-architecture",
    "rests-on-target-type",
    "guideline-retire-by-command",
    // E1: retirada de diretiva, em `crates/omhl/src/app/error.rs`
    "guideline-not-found",
    "guideline-not-orphaned",
    "successor-not-current",
    "successor-is-self",
    "supersedes-cycle",
];
/// Âncoras da skill, sempre entre crases no texto; origem: bloco S13 do plano e decisões (e), (i) e (j).
const SKILL_ANCHORS: [&str; 20] = [
    // as quatro categorias de proposta da retro
    "guideline",
    "premise",
    "spec-section",
    "check",
    // retro e novo check
    "omhl report retro",
    ".omhl/checks/",
    "omhl load",
    // gravar pela CLI (`omhl append`, `omhl attest`)
    "append",
    "omhl append",
    "omhl attest",
    // aprovação do hook do `wt`
    "wt config approvals add",
    "Cannot prompt for approval",
    "--no-hooks",
    // emenda (decisão j)
    "divergence",
    "open",
    "emenda-NN.md",
    "omhl spec validate",
    "spec-stale",
    "amendment",
    "confirmed_by",
];

// Apoio

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

fn missing_from<'a>(page: &str, wanted: &[&'a str]) -> Vec<&'a str> {
    wanted
        .iter()
        .copied()
        .filter(|item| !page.contains(&format!("`{item}`")))
        .collect()
}

/// Colunas da matriz por papel: `commands`, `append.nodes`, `append.relations`, `append.retire`.
type RoleColumns = Vec<BTreeSet<String>>;

fn cell_items(cell: &str) -> BTreeSet<String> {
    cell.split(',')
        .map(|item| item.trim().trim_matches('`').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Primeira tabela da seção `### A matriz por papel`, por papel (sem cabeçalho e separador).
fn documented_role_matrix(page: &str) -> BTreeMap<String, RoleColumns> {
    page.lines()
        .skip_while(|line| *line != "### A matriz por papel")
        .skip(1)
        .skip_while(|line| !line.starts_with('|'))
        .take_while(|line| line.starts_with('|'))
        .skip(2)
        .map(|row| {
            let mut cells = row.trim().trim_matches('|').split('|');
            let role = cells.next().expect("the row has a role cell");

            (
                role.trim().trim_matches('`').to_string(),
                cells.map(cell_items).collect(),
            )
        })
        .collect()
}

fn set_of(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn compiled_role_matrix() -> BTreeMap<String, RoleColumns> {
    ROLE_REFERENCE_COMMANDS
        .iter()
        .map(|(role, commands)| {
            let (_, nodes, relations, retire) = ROLE_REFERENCE_MATRIX
                .iter()
                .find(|(matrix_role, ..)| matrix_role == role)
                .expect("the matrix covers the role");

            (
                role.to_string(),
                vec![
                    set_of(commands),
                    set_of(nodes),
                    set_of(relations),
                    set_of(retire),
                ],
            )
        })
        .collect()
}

// Documentação

#[test]
fn it_should_link_the_mechanical_gates_page_from_the_readme() {
    let readme = read_repo_file("README.md");

    assert!(readme.contains("](docs/mechanical-gates.md)"));
    assert!(repo_root().join(MECHANICAL_GATES_PAGE).is_file());
}

#[test]
fn it_should_document_every_error_code_and_kind_of_the_failure_table_in_the_mechanical_gates_page()
{
    let page = read_repo_file(MECHANICAL_GATES_PAGE);

    let missing_codes = missing_from(&page, &FAILURE_TABLE_CODES);
    let missing_kinds = missing_from(&page, &FAILURE_TABLE_KINDS);

    assert_eq!(missing_codes, Vec::<&str>::new());
    assert_eq!(missing_kinds, Vec::<&str>::new());
}

#[test]
fn it_should_provide_the_develop_mark_command_with_the_marker_line() {
    let command = read_repo_file(DEVELOP_MARK_COMMAND);
    let page = read_repo_file(MECHANICAL_GATES_PAGE);

    assert!(command.lines().any(|line| line == MARK_LINE));
    assert!(command.contains("omhl-develop-mark v1"));
    assert!(page.contains("](../plugin/commands/omhl-develop-mark.md)"));
}

#[test]
fn it_should_name_diretiva_as_the_pt_br_name_of_guideline_in_the_glossary() {
    let glossary = read_repo_file("GLOSSARY.md").to_lowercase();

    let entry = glossary
        .split("\n**")
        .find(|block| block.starts_with("guideline**:"))
        .expect("the glossary has a Guideline entry");
    let avoid_lines: Vec<&str> = entry
        .lines()
        .filter(|line| line.starts_with("_avoid_:"))
        .collect();

    assert!(entry.contains("diretiva"), "Guideline entry lacks diretiva");
    assert!(
        !avoid_lines.is_empty(),
        "Guideline entry has no _Avoid_ line"
    );
    assert!(
        avoid_lines.iter().all(|line| !line.contains("directive")),
        "Guideline _Avoid_ still lists directive: {avoid_lines:?}"
    );
}

#[test]
fn it_should_provide_the_directives_flow_page() {
    assert!(repo_root().join(DIRECTIVES_FLOW_PAGE).is_file());
}

#[test]
fn it_should_document_every_new_error_and_violation_code_in_the_directives_flow_page() {
    let page = read_repo_file(DIRECTIVES_FLOW_PAGE);

    let missing = missing_from(&page, &DIRECTIVES_FLOW_CODES);

    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn it_should_link_the_directives_flow_page_from_the_readme() {
    let readme = read_repo_file("README.md");

    assert!(readme.contains("](docs/directives-flow.md)"));
}

#[test]
fn it_should_link_the_directives_flow_page_from_agents_md() {
    let agents = read_repo_file("AGENTS.md");

    assert!(agents.contains("](docs/directives-flow.md)"));
}

#[test]
fn it_should_tell_agents_md_to_write_through_the_cli() {
    let agents = read_repo_file("AGENTS.md");

    let append_line = agents
        .lines()
        .find(|line| line.contains("omhl append"))
        .expect("AGENTS.md mentions omhl append");

    assert!(!append_line.contains("nunca"));
}

#[test]
fn it_should_provide_the_directives_flow_skill_with_name_and_description_in_the_frontmatter() {
    let skill = read_repo_file(DIRECTIVES_FLOW_SKILL);

    let frontmatter: Vec<&str> = skill
        .strip_prefix("---\n")
        .and_then(|rest| rest.split("\n---").next())
        .expect("the skill starts with a frontmatter")
        .lines()
        .collect();
    let value_of = |key: &str| {
        frontmatter
            .iter()
            .find_map(|line| line.strip_prefix(key))
            .map(str::trim)
    };

    assert_eq!(value_of("name:"), Some("directives-flow"));
    assert!(value_of("description:").is_some_and(|text| !text.is_empty()));
}

#[test]
fn it_should_cover_every_required_anchor_in_the_directives_flow_skill() {
    let skill = read_repo_file(DIRECTIVES_FLOW_SKILL);

    let missing = missing_from(&skill, &SKILL_ANCHORS);

    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn it_should_keep_the_matrix_table_in_docs_directives_flow_equal_to_the_compiled_matrix() {
    let page = read_repo_file(DIRECTIVES_FLOW_PAGE);

    let documented = documented_role_matrix(&page);

    assert_eq!(documented, compiled_role_matrix());
}
