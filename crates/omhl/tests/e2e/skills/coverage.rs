//! `skill_coverage`: toda folha da CLI de ação é citada por uma skill, cada folha tem o seu
//! `references/<folha>.md` com os cinco títulos fixos, o `SKILL.md` cabe em 80 linhas, e todo código de
//! erro citado na seção "Errors" existe no código. A lógica fica em funções puras sobre um diretório de
//! skills, para a prova de falha rodar sobre um diretório sintético. Cada opção da CLI (as de cada folha
//! e as globais) também precisa aparecer na seção "Inputs" da página da folha, e todo caminho `.md`
//! citado (link `](alvo.md)` ou caminho entre crases que começa por `./`, `../`, `references/` ou `docs/`)
//! precisa resolver para um arquivo dentro de `plugin/`: o plugin viaja sozinho, sem o `docs/` do repositório.
//!
//! Convenções que as skills seguem:
//! - citação: `omhl <caminho completo>` dentro de crases ou numa linha `$ omhl ...` (a prosa solta
//!   "omhl é..." não conta);
//! - código de erro: todo trecho entre crases do começo de um item de lista da seção "Errors" (antes do
//!   primeiro `:`) que tenha forma de código, como em `- \`no-marker\` (exit 1): ...` ou
//!   `- \`a\` e \`b\`: ...`; um ponteiro (`/limit`) não é código, e o erro de uso do clap (exit 2) não
//!   tem código e não é conferido;
//! - frontmatter: todo `SKILL.md` abre com `name:` igual à pasta e `description:` não vazia;
//! - página órfã: toda página de `references/` que não é de uma folha é linkada pelo `SKILL.md` da sua skill;
//! - dono único: cada folha tem uma única página de referência, em uma única skill.

use std::fs;
use std::path::{Path, PathBuf};

use clap::CommandFactory;
use omhl::adapters::cli::Cli;
use regex::Regex;
use tempfile::TempDir;

use crate::support::repo_checks::repo_root;
use crate::support::scan::prove_both_sides;

pub const MAX_SKILL_LINES: usize = 80;
const TITLES: [&str; 5] = [
    "## When to use",
    "## Inputs",
    "## Output",
    "## Errors",
    "## Examples",
];
const PREFIXES: [&str; 10] = [
    "subcommands not cited by any skill under plugin/skills",
    "cited subcommands that do not exist",
    "SKILL.md files above 80 lines",
    "references missing or without the five fixed titles",
    "error codes cited in Errors that are not a literal in crates/*/src",
    "options missing from the Inputs section of their reference",
    "markdown paths that do not resolve to a file inside plugin/",
    "SKILL.md files without the house frontmatter (name equal to the folder, non-empty description)",
    "reference pages not linked from the SKILL.md of their skill",
    "commands with a reference page in more than one skill",
];

pub fn skills_dir() -> PathBuf {
    repo_root().join("plugin/skills")
}

/// Caminhos das folhas de ação (`"check run"`), sem o grupo `hook` e sem o `help` do clap.
pub fn leaf_commands() -> Vec<String> {
    leaf_options().into_iter().map(|(leaf, _)| leaf).collect()
}

/// Cada folha com as suas opções longas (`"check run"`, `["--check", ...]`), sem `help`, `version` e as globais.
pub fn leaf_options() -> Vec<(String, Vec<String>)> {
    let mut leaves = Vec::new();
    for command in Cli::command().get_subcommands() {
        collect_leaves(command, "", &mut leaves);
    }
    leaves.sort();

    leaves
}

/// Opções longas globais (`--db`, `--project-dir`), lidas da raiz, onde nascem.
pub fn global_options() -> Vec<String> {
    let mut names = long_options(
        Cli::command()
            .get_arguments()
            .filter(|arg| arg.is_global_set()),
    );
    names.sort();

    names
}

fn long_options<'a>(arguments: impl Iterator<Item = &'a clap::Arg>) -> Vec<String> {
    arguments
        .filter(|arg| !matches!(arg.get_id().as_str(), "help" | "version"))
        .filter_map(|arg| arg.get_long().map(|long| format!("--{long}")))
        .collect()
}

fn collect_leaves(command: &clap::Command, prefix: &str, leaves: &mut Vec<(String, Vec<String>)>) {
    let name = command.get_name();
    if name == "help" || (prefix.is_empty() && name == "hook") {
        return;
    }
    let path = if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix} {name}")
    };
    let children: Vec<&clap::Command> = command
        .get_subcommands()
        .filter(|child| child.get_name() != "help")
        .collect();
    // Um grupo híbrido (`byproduct`) também é folha: tem argumentos próprios além do subcomando.
    if children.is_empty() || command.get_positionals().next().is_some() {
        let mut options = long_options(command.get_arguments().filter(|arg| !arg.is_global_set()));
        options.sort();
        leaves.push((path.clone(), options));
    }
    for child in children {
        collect_leaves(child, &path, leaves);
    }
}

pub struct Doc {
    /// Relativo ao diretório de skills: `review/references/attest.md`.
    pub path: String,
    pub text: String,
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.map(|entry| entry.expect("entry").path()).collect();
    paths.sort();

    paths
}

/// `skills/*/SKILL.md` e `skills/*/references/*.md`, em ordem.
pub fn read_docs(skills: &Path) -> Vec<Doc> {
    let mut found = Vec::new();
    for skill in sorted_entries(skills).iter().filter(|path| path.is_dir()) {
        let mut files = vec![skill.join("SKILL.md")];
        files.extend(
            sorted_entries(&skill.join("references"))
                .into_iter()
                .filter(|path| path.extension().is_some_and(|ext| ext == "md")),
        );
        for file in files.iter().filter(|file| file.is_file()) {
            found.push(Doc {
                path: file
                    .strip_prefix(skills)
                    .expect("inside the skills folder")
                    .to_string_lossy()
                    .into_owned(),
                text: fs::read_to_string(file).expect("the skill file is read"),
            });
        }
    }

    found
}

/// Todo `.rs` de `crates/*/src`, concatenado: a base da conferência dos códigos de erro.
pub fn read_sources(root: &Path) -> String {
    let mut text = String::new();
    for krate in sorted_entries(&root.join("crates")) {
        let mut pending = vec![krate.join("src")];
        while let Some(dir) = pending.pop() {
            for path in sorted_entries(&dir) {
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    text.push_str(&fs::read_to_string(path).expect("the source file is read"));
                }
            }
        }
    }

    text
}

/// `(linha, palavras)` de cada `omhl <palavras>` citado em `text`.
fn citations(text: &str) -> Vec<(usize, Vec<String>)> {
    let pattern = Regex::new(r"(?:^\s*\$ |`)omhl((?: [a-z][a-z0-9-]*)+)").expect("regex");
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for captures in pattern.captures_iter(line) {
            let words = captures[1].split_whitespace().map(str::to_owned).collect();
            found.push((index + 1, words));
        }
    }

    found
}

pub fn starts_with_path(words: &[String], leaf: &str) -> bool {
    let leaf: Vec<&str> = leaf.split(' ').collect();

    words.len() >= leaf.len() && words.iter().zip(&leaf).all(|(word, part)| word == part)
}

/// Palavras que são o começo de um caminho de folha (`omhl check`, o grupo) não citam nada, mas existem.
fn is_group_mention(words: &[String], leaves: &[String]) -> bool {
    leaves.iter().any(|leaf| {
        let leaf: Vec<&str> = leaf.split(' ').collect();
        words.len() < leaf.len() && words.iter().zip(&leaf).all(|(word, part)| word == part)
    })
}

/// Todos os códigos (`kebab-case` ou `MAIUSCULO_COM_SUBLINHADO`) entre crases no começo de cada item de lista
/// da seção "Errors", antes do primeiro `:`; `/limit` (ponteiro) e texto depois do `:` não entram.
fn error_codes(text: &str) -> Vec<String> {
    let bullet = Regex::new(r"^\s*[-*] (`[^:]*)").expect("regex");
    let code = Regex::new(r"`([A-Za-z][A-Za-z0-9_-]*)`").expect("regex");
    let mut in_section = false;
    let mut codes = Vec::new();
    for line in text.lines() {
        if line.starts_with("## ") {
            in_section = line.trim_end() == "## Errors";
        } else if in_section && let Some(head) = bullet.captures(line) {
            codes.extend(
                code.captures_iter(&head[1])
                    .map(|found| found[1].to_owned()),
            );
        }
    }

    codes
}

/// Texto da seção "Inputs" (até o próximo `## `).
fn entradas(text: &str) -> String {
    let mut in_section = false;
    let mut section = String::new();
    for line in text.lines() {
        if line.starts_with("## ") {
            in_section = line.trim_end() == "## Inputs";
        } else if in_section {
            section.push_str(line);
            section.push('\n');
        }
    }

    section
}

/// A opção como palavra inteira: `--by` não casa `--by-x` nem `--check` casa `--check-run`.
fn mentions(section: &str, option: &str) -> bool {
    let pattern = format!(r"(?m)(?:^|[^\w-]){}(?:[^\w-]|$)", regex::escape(option));

    Regex::new(&pattern).expect("regex").is_match(section)
}

/// `(linha, alvo)` de cada caminho `.md` citado em `text`; links `http` ficam de fora.
fn md_targets(text: &str) -> Vec<(usize, String)> {
    let pattern = Regex::new(
        r"\]\(([^)\s#]+\.md)(?:#[^)\s]*)?\)|`((?:\./|\.\./|references/|docs/)[^`\s]*\.md)`",
    )
    .expect("regex");
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for captures in pattern.captures_iter(line) {
            let target = captures
                .get(1)
                .or(captures.get(2))
                .expect("one group")
                .as_str();
            if !target.starts_with("http") {
                found.push((index + 1, target.to_owned()));
            }
        }
    }

    found
}

/// Os caminhos `.md` do `doc` que não resolvem, a partir da pasta do `doc`, para um arquivo dentro de `plugin`.
fn unresolved_paths(skills: &Path, plugin: &Path, doc: &Doc) -> Vec<String> {
    let folder = skills.join(&doc.path);
    let folder = folder.parent().expect("a parent folder");

    md_targets(&doc.text)
        .into_iter()
        .filter(|(_, target)| {
            !fs::canonicalize(folder.join(target))
                .is_ok_and(|resolved| resolved.is_file() && resolved.starts_with(plugin))
        })
        .map(|(line, target)| format!("{}:{line}: {target}", doc.path))
        .collect()
}

/// `---`, `name: <pasta>`, `description: <não vazia>`, `---`: o frontmatter da casa.
fn has_house_frontmatter(skill: &str, text: &str) -> bool {
    let mut lines = text.lines();

    lines.next() == Some("---")
        && lines.next() == Some(format!("name: {skill}").as_str())
        && lines
            .next()
            .and_then(|line| line.strip_prefix("description: "))
            .is_some_and(|description| !description.trim().is_empty())
        && lines.next() == Some("---")
}

/// Página de `references/` que não é de uma folha e que o `SKILL.md` da sua skill não linka.
fn is_orphan_page(docs: &[Doc], leaves: &[String], page: &Doc) -> bool {
    let Some((skill, relative)) = page.path.split_once('/') else {
        return false;
    };
    let is_leaf_page = leaves
        .iter()
        .any(|leaf| relative == format!("references/{}.md", leaf.replace(' ', "-")));
    if !relative.starts_with("references/") || is_leaf_page {
        return false;
    }

    !docs
        .iter()
        .filter(|doc| doc.path == format!("{skill}/SKILL.md"))
        .any(|doc| {
            md_targets(&doc.text)
                .iter()
                .any(|(_, target)| target.trim_start_matches("./") == relative)
        })
}

#[derive(Default)]
pub struct Findings {
    uncited: Vec<String>,
    nonexistent: Vec<String>,
    long_skills: Vec<String>,
    bad_references: Vec<String>,
    unknown_codes: Vec<String>,
    missing_options: Vec<String>,
    unresolved_paths: Vec<String>,
    bad_frontmatter: Vec<String>,
    orphan_pages: Vec<String>,
    duplicate_owners: Vec<String>,
}

impl Findings {
    fn lists(&self) -> [&Vec<String>; 10] {
        [
            &self.uncited,
            &self.nonexistent,
            &self.long_skills,
            &self.bad_references,
            &self.unknown_codes,
            &self.missing_options,
            &self.unresolved_paths,
            &self.bad_frontmatter,
            &self.orphan_pages,
            &self.duplicate_owners,
        ]
    }

    fn counts(&self) -> [usize; 10] {
        self.lists().map(|list| list.len())
    }

    /// Uma mensagem por falha não vazia, com prefixo fixo e a lista ordenada.
    pub fn messages(&self) -> Vec<String> {
        self.lists()
            .iter()
            .zip(PREFIXES)
            .filter(|(list, _)| !list.is_empty())
            .map(|(list, prefix)| format!("{prefix}: [{}]", list.join(", ")))
            .collect()
    }
}

pub fn check(
    skills: &Path,
    options: &[(String, Vec<String>)],
    globals: &[String],
    sources: &str,
) -> Findings {
    let docs = read_docs(skills);
    let plugin = fs::canonicalize(skills.join("..")).expect("the plugin folder exists");
    let leaves: Vec<String> = options.iter().map(|(leaf, _)| leaf.clone()).collect();
    let mut findings = Findings::default();

    for (leaf, local) in options {
        // Só o SKILL.md cita: todo references/<folha>.md traz `$ omhl <folha>` no exemplo e se citaria sozinho.
        let cited = docs
            .iter()
            .filter(|doc| doc.path.ends_with("/SKILL.md"))
            .any(|doc| {
                citations(&doc.text)
                    .iter()
                    .any(|(_, words)| starts_with_path(words, leaf))
            });
        if !cited {
            findings.uncited.push(leaf.clone());
        }

        let file = format!("/{}.md", leaf.replace(' ', "-"));
        let versions: Vec<&Doc> = docs
            .iter()
            .filter(|doc| doc.path.contains("/references/") && doc.path.ends_with(&file))
            .collect();
        if versions.is_empty() {
            findings
                .bad_references
                .push(format!("{leaf}: file missing"));
        }
        if versions.len() > 1 {
            let owners: Vec<&str> = versions.iter().map(|doc| doc.path.as_str()).collect();
            findings
                .duplicate_owners
                .push(format!("{leaf}: {}", owners.join(" + ")));
        }
        for doc in versions {
            let section = entradas(&doc.text);
            for option in local.iter().chain(globals) {
                if !mentions(&section, option) {
                    findings
                        .missing_options
                        .push(format!("{}: {option}", doc.path));
                }
            }
            let missing: Vec<&str> = TITLES
                .into_iter()
                .filter(|title| !doc.text.lines().any(|line| line.trim_end() == *title))
                .collect();
            if !missing.is_empty() {
                findings
                    .bad_references
                    .push(format!("{}: missing {missing:?}", doc.path));
            }
        }
    }

    for doc in &docs {
        for (line, words) in citations(&doc.text) {
            let known = leaves.iter().any(|leaf| starts_with_path(&words, leaf));
            if !known && words[0] != "hook" && !is_group_mention(&words, &leaves) {
                findings
                    .nonexistent
                    .push(format!("{}:{line}: omhl {}", doc.path, words.join(" ")));
            }
        }
        findings
            .unresolved_paths
            .extend(unresolved_paths(skills, &plugin, doc));
        if doc.path.ends_with("/SKILL.md") && doc.text.lines().count() > MAX_SKILL_LINES {
            findings.long_skills.push(doc.path.clone());
        }
        if let Some(skill) = doc.path.strip_suffix("/SKILL.md")
            && !has_house_frontmatter(skill, &doc.text)
        {
            findings.bad_frontmatter.push(doc.path.clone());
        }
        if is_orphan_page(&docs, &leaves, doc) {
            findings.orphan_pages.push(doc.path.clone());
        }
        for code in error_codes(&doc.text) {
            if !sources.contains(&format!("\"{code}\"")) {
                findings.unknown_codes.push(format!("{}: {code}", doc.path));
            }
        }
    }

    findings
}

// Prova de falha sobre um diretório sintético

const SYNTHETIC_LEAVES: [(&str, &str); 2] = [("list", "--limit"), ("check run", "--event")];
const SYNTHETIC_GLOBAL: &str = "--db";
const SYNTHETIC_SOURCES: &str = r#"fn code() -> &'static str { "known-code" }"#;
const BOTH_CITED: &str = "Run `omhl list` and `omhl check run`.\n";

/// `SKILL.md` sintético com o frontmatter da casa para a skill `name`.
fn skill_md(name: &str, body: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Protocolo de teste. Use sempre.\n---\n# {name}\n\n{body}"
    )
}

/// Página completa: a seção "Entradas" cita a opção da folha e a global.
fn reference(leaf: &str, errors: &str) -> String {
    let option = SYNTHETIC_LEAVES
        .iter()
        .find(|(name, _)| *name == leaf)
        .map_or("", |(_, option)| *option);

    page(
        leaf,
        &format!("- `{option}`, `{SYNTHETIC_GLOBAL}`."),
        "",
        errors,
    )
}

fn page(leaf: &str, entradas: &str, usage: &str, errors: &str) -> String {
    format!(
        "# t\n\n## When to use\n\n{usage}\n\n## Inputs\n\n{entradas}\n\n## Output\n\n## Errors\n\n{errors}\n\n## Examples\n\n$ omhl {leaf}\n"
    )
}

/// Árvore sintética completa, depois das trocas (`None` apaga o arquivo), serializada como
/// `=== <caminho>` seguido do conteúdo: é o "trecho" que o `prove_both_sides` recebe.
fn tree(changes: &[(&str, Option<String>)]) -> String {
    let known = "- `known-code` (exit 1): the thing failed.";
    let mut files = std::collections::BTreeMap::from([
        ("s/SKILL.md".to_owned(), skill_md("s", BOTH_CITED)),
        ("s/references/list.md".to_owned(), reference("list", known)),
        (
            "s/references/check-run.md".to_owned(),
            reference("check run", known),
        ),
    ]);
    for (path, content) in changes {
        match content {
            Some(content) => files.insert((*path).to_owned(), content.clone()),
            None => files.remove(*path),
        };
    }

    files
        .iter()
        .map(|(path, content)| format!("=== {path}\n{content}"))
        .collect()
}

fn materialize(encoded: &str, dir: &Path) {
    let mut current: Option<PathBuf> = None;
    for line in encoded.split_inclusive('\n') {
        if let Some(path) = line.strip_prefix("=== ") {
            let target = dir.join(path.trim_end());
            fs::create_dir_all(target.parent().expect("a parent")).expect("parent");
            fs::write(&target, "").expect("file");
            current = Some(target);
        } else {
            let target = current.as_ref().expect("content after a path line");
            let mut text = fs::read_to_string(target).expect("file");
            text.push_str(line);
            fs::write(target, text).expect("file");
        }
    }
}

/// Conta só as falhas da lista `kind`; falha de outra lista no mesmo diretório é erro da prova.
fn count_of(kind: usize) -> impl Fn(&str) -> Result<usize, String> {
    move |encoded| {
        let dir = TempDir::new().expect("temp dir");
        let skills = dir.path().join("plugin/skills");
        materialize(encoded, &skills);
        let options: Vec<(String, Vec<String>)> = SYNTHETIC_LEAVES
            .map(|(leaf, option)| (leaf.to_owned(), vec![option.to_owned()]))
            .to_vec();
        let globals = [SYNTHETIC_GLOBAL.to_owned()];
        let findings = check(&skills, &options, &globals, SYNTHETIC_SOURCES);
        let counts = findings.counts();
        if counts
            .iter()
            .enumerate()
            .any(|(index, count)| index != kind && *count > 0)
        {
            return Err(format!(
                "the snippet trips other failures: {:?}",
                findings.messages()
            ));
        }

        Ok(counts[kind])
    }
}

/// As dez falhas, cada uma isolada, contra a árvore completa que não reprova nenhuma.
pub fn prove_checks() -> Vec<String> {
    let complete = tree(&[]);
    let skill_with = |body: &str| tree(&[("s/SKILL.md", Some(skill_md("s", body)))]);
    let uncited = skill_with("Run `omhl list`.\n");
    let nonexistent = skill_with(&format!(
        "{BOTH_CITED}Also `omhl listt` and `omhl check nope`.\n"
    ));
    let long = skill_with(&format!(
        "{BOTH_CITED}{}",
        "filler\n".repeat(MAX_SKILL_LINES)
    ));
    let no_file = tree(&[("s/references/check-run.md", None)]);
    let no_title = tree(&[(
        "s/references/list.md",
        Some(reference("list", "- `known-code` (exit 1): x.").replace("## Output\n\n", "")),
    )]);
    let ghost = tree(&[(
        "s/references/list.md",
        Some(reference("list", "- `ghost-code` (exit 1): x.")),
    )]);
    let group_only = skill_with(&format!("{BOTH_CITED}See `omhl check`.\n"));
    let clap_usage = tree(&[(
        "s/references/list.md",
        Some(reference("list", "- Exit 2: clap usage error, no code.")),
    )]);
    let hook_group = skill_with(&format!("{BOTH_CITED}Not ours: `omhl hook x`.\n"));
    let two_codes = tree(&[(
        "s/references/list.md",
        Some(reference(
            "list",
            "- `known-code` and `ghost-code` (exit 1): x.",
        )),
    )]);
    let code_with_pointer = tree(&[(
        "s/references/list.md",
        Some(reference(
            "list",
            "- `known-code` at `/limit` (exit 1): the `ghost-code` appears only after the colon.",
        )),
    )]);

    let known = "- `known-code` (exit 1): x.";
    let list_page = |entradas: &str, usage: &str| {
        tree(&[(
            "s/references/list.md",
            Some(page("list", entradas, usage, known)),
        )])
    };
    let option_in_prose = list_page("- `--db`.", "Use `--limit` here.");
    let option_absent = list_page("- `--db`.", "");
    let global_absent = list_page("- `--limit`.", "");
    let option_prefix_only = list_page("- `--limit-x`, `--db`.", "");

    let link_page = |links: &str| {
        tree(&[(
            "s/references/list.md",
            Some(page("list", "- `--limit`, `--db`.", links, known)),
        )])
    };
    let link_to_nothing = link_page("See [x](missing.md).");
    let link_outside_plugin = format!(
        "{}{}",
        link_page("See [x](../../../../docs/outside.md)."),
        "=== ../../docs/outside.md\n# outside\n"
    );
    let docs_path = link_page("See `docs/outside.md`.");
    let links_inside = link_page(
        "See [c](check-run.md#erros), [s](../SKILL.md), [w](https://x.dev/y.md), `emenda-NN.md`.",
    );

    let frontmatter_of = |text: String| tree(&[("s/SKILL.md", Some(text))]);
    let no_frontmatter = frontmatter_of(format!("# s\n\n{BOTH_CITED}"));
    let wrong_name = frontmatter_of(skill_md("s", BOTH_CITED).replace("name: s", "name: other"));
    let empty_description = frontmatter_of(skill_md("s", BOTH_CITED).replace(
        "description: Protocolo de teste. Use sempre.",
        "description: ",
    ));

    let extra_page = ("s/references/extra.md", Some("# extra\n".to_owned()));
    let orphan_page = tree(std::slice::from_ref(&extra_page));
    let linked_page = tree(&[
        extra_page,
        (
            "s/SKILL.md",
            Some(skill_md(
                "s",
                &format!("{BOTH_CITED}See [e](references/extra.md).\n"),
            )),
        ),
    ]);

    let second_owner = tree(&[
        ("t/SKILL.md", Some(skill_md("t", "Nothing.\n"))),
        ("t/references/list.md", Some(reference("list", known))),
    ]);

    let mut failures = Vec::new();
    for (kind, flagged) in [
        (0, vec![uncited]),
        (1, vec![nonexistent]),
        (2, vec![long]),
        (3, vec![no_file, no_title]),
        (4, vec![ghost, two_codes]),
        (
            5,
            vec![
                option_in_prose,
                option_absent,
                global_absent,
                option_prefix_only,
            ],
        ),
        (6, vec![link_to_nothing, link_outside_plugin, docs_path]),
        (7, vec![no_frontmatter, wrong_name, empty_description]),
        (8, vec![orphan_page]),
        (9, vec![second_owner]),
    ] {
        let flagged: Vec<&str> = flagged.iter().map(String::as_str).collect();
        let accepted = [
            complete.as_str(),
            group_only.as_str(),
            clap_usage.as_str(),
            hook_group.as_str(),
            links_inside.as_str(),
            code_with_pointer.as_str(),
            linked_page.as_str(),
        ];
        failures.extend(
            prove_both_sides(count_of(kind), &flagged, &accepted)
                .into_iter()
                .map(|failure| format!("{}: {failure}", PREFIXES[kind])),
        );
    }

    failures
}

#[test]
fn it_should_enumerate_the_action_leaves_of_the_cli() {
    let expected = [
        "append",
        "attest",
        "byproduct",
        "byproduct prune",
        "check retire",
        "check run",
        "check sweep",
        "close",
        "gate-delivery",
        "gate-worktree",
        "get",
        "guideline retire",
        "hooks install",
        "hooks post-commit",
        "hooks post-merge",
        "list",
        "load",
        "project",
        "render",
        "report answers",
        "report recurrences",
        "report retro",
        "report sessions",
        "spec init",
        "spec validate",
        "verify",
    ];

    let leaves = leaf_commands();

    assert_eq!(leaves, expected);
}

#[test]
fn it_should_enumerate_the_options_of_the_cli() {
    let expected = [
        ("append", vec!["--owner-override"]),
        ("attest", vec!["--note", "--outcome"]),
        ("byproduct", vec!["--tail"]),
        ("check retire", vec!["--owner-override"]),
        ("check run", vec!["--check", "--event", "--pre-push"]),
        (
            "close",
            vec![
                "--abandon",
                "--base",
                "--failed-delivery",
                "--force",
                "--note",
                "--reason",
            ],
        ),
        ("gate-delivery", vec!["--base"]),
        ("gate-worktree", vec!["--branch"]),
        ("guideline retire", vec!["--by"]),
        ("hooks install", vec!["--force"]),
        (
            "list",
            vec!["--after-seq", "--id-prefix", "--limit", "--type", "--where"],
        ),
        ("load", vec!["--dry-run", "--owner-override"]),
        ("report answers", vec!["--work-item"]),
        ("report recurrences", vec!["--min"]),
        ("report sessions", vec!["--work-item"]),
    ];

    let with_options: Vec<(String, Vec<String>)> = leaf_options()
        .into_iter()
        .filter(|(_, options)| !options.is_empty())
        .collect();

    assert_eq!(
        (with_options, global_options()),
        (
            expected
                .map(|(leaf, options)| (
                    leaf.to_owned(),
                    options.into_iter().map(str::to_owned).collect()
                ))
                .to_vec(),
            vec![
                "--db".to_owned(),
                "--format".to_owned(),
                "--project-dir".to_owned()
            ]
        )
    );
}

#[test]
fn it_should_prove_each_coverage_failure_on_a_synthetic_skills_folder() {
    let failures = prove_checks();

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn skill_coverage() {
    let mut failures = prove_checks();
    let findings = check(
        &skills_dir(),
        &leaf_options(),
        &global_options(),
        &read_sources(&repo_root()),
    );
    failures.extend(findings.messages());

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
