//! `skill_coverage`: toda folha da CLI de ação é citada por uma skill, cada folha tem o seu
//! `references/<folha>.md` com os cinco títulos fixos, o `SKILL.md` cabe em 80 linhas, e todo código de
//! erro citado na seção "Erros" existe no código. A lógica fica em funções puras sobre um diretório de
//! skills, para a prova de falha rodar sobre um diretório sintético.
//!
//! Convenções que as skills seguem:
//! - citação: `omhl <caminho completo>` dentro de crases ou numa linha `$ omhl ...` (a prosa solta
//!   "omhl é..." não conta);
//! - código de erro: o primeiro trecho entre crases de um item de lista da seção "Erros"
//!   (`- \`no-marker\` (saída 1): ...`); o erro de uso do clap (saída 2) não tem código e não é conferido.

use std::fs;
use std::path::{Path, PathBuf};

use clap::CommandFactory;
use omhl::adapters::cli::Cli;
use regex::Regex;
use tempfile::TempDir;

use crate::support::scan::prove_both_sides;

pub const MAX_SKILL_LINES: usize = 80;
const TITLES: [&str; 5] = [
    "## Quando usar",
    "## Entradas",
    "## Saída",
    "## Erros",
    "## Exemplos",
];
const PREFIXES: [&str; 5] = [
    "subcommands not cited by any skill under plugin/skills",
    "cited subcommands that do not exist",
    "SKILL.md files above 80 lines",
    "references missing or without the five fixed titles",
    "error codes cited in Erros that are not a literal in crates/*/src",
];

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn skills_dir() -> PathBuf {
    repo_root().join("plugin/skills")
}

/// Caminhos das folhas de ação (`"check run"`), sem o grupo `hook` e sem o `help` do clap.
pub fn leaf_commands() -> Vec<String> {
    let mut leaves = Vec::new();
    for command in Cli::command().get_subcommands() {
        collect_leaves(command, "", &mut leaves);
    }
    leaves.sort();

    leaves
}

fn collect_leaves(command: &clap::Command, prefix: &str, leaves: &mut Vec<String>) {
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
    if children.is_empty() {
        leaves.push(path.clone());
    }
    for child in children {
        collect_leaves(child, &path, leaves);
    }
}

pub struct Doc {
    /// Relativo ao diretório de skills: `directives-flow/references/attest.md`.
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

fn starts_with_path(words: &[String], leaf: &str) -> bool {
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

/// Códigos do primeiro trecho entre crases de cada item de lista da seção "Erros".
fn error_codes(text: &str) -> Vec<String> {
    let bullet = Regex::new(r"^\s*[-*] `([^`]+)`").expect("regex");
    let mut in_section = false;
    let mut codes = Vec::new();
    for line in text.lines() {
        if line.starts_with("## ") {
            in_section = line.trim_end() == "## Erros";
        } else if in_section && let Some(captures) = bullet.captures(line) {
            codes.push(captures[1].to_owned());
        }
    }

    codes
}

#[derive(Default)]
pub struct Findings {
    uncited: Vec<String>,
    nonexistent: Vec<String>,
    long_skills: Vec<String>,
    bad_references: Vec<String>,
    unknown_codes: Vec<String>,
}

impl Findings {
    fn lists(&self) -> [&Vec<String>; 5] {
        [
            &self.uncited,
            &self.nonexistent,
            &self.long_skills,
            &self.bad_references,
            &self.unknown_codes,
        ]
    }

    fn counts(&self) -> [usize; 5] {
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

pub fn check(skills: &Path, leaves: &[String], sources: &str) -> Findings {
    let docs = read_docs(skills);
    let mut findings = Findings::default();

    for leaf in leaves {
        let cited = docs.iter().any(|doc| {
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
        for doc in versions {
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
            if !known && words[0] != "hook" && !is_group_mention(&words, leaves) {
                findings
                    .nonexistent
                    .push(format!("{}:{line}: omhl {}", doc.path, words.join(" ")));
            }
        }
        if doc.path.ends_with("/SKILL.md") && doc.text.lines().count() > MAX_SKILL_LINES {
            findings.long_skills.push(doc.path.clone());
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

const SYNTHETIC_LEAVES: [&str; 2] = ["list", "check run"];
const SYNTHETIC_SOURCES: &str = r#"fn code() -> &'static str { "known-code" }"#;

fn reference(errors: &str) -> String {
    format!(
        "# t\n\n## Quando usar\n\n## Entradas\n\n## Saída\n\n## Erros\n\n{errors}\n\n## Exemplos\n"
    )
}

/// Árvore sintética completa, depois das trocas (`None` apaga o arquivo), serializada como
/// `=== <caminho>` seguido do conteúdo: é o "trecho" que o `prove_both_sides` recebe.
fn tree(changes: &[(&str, Option<String>)]) -> String {
    let known = "- `known-code` (saída 1): the thing failed.";
    let mut files = std::collections::BTreeMap::from([
        (
            "s/SKILL.md".to_owned(),
            "# s\n\nRun `omhl list` and `omhl check run`.\n".to_owned(),
        ),
        ("s/references/list.md".to_owned(), reference(known)),
        ("s/references/check-run.md".to_owned(), reference(known)),
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
        materialize(encoded, dir.path());
        let leaves: Vec<String> = SYNTHETIC_LEAVES.map(str::to_owned).to_vec();
        let findings = check(dir.path(), &leaves, SYNTHETIC_SOURCES);
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

/// As cinco falhas, cada uma isolada, contra a árvore completa que não reprova nenhuma.
pub fn prove_checks() -> Vec<String> {
    let complete = tree(&[]);
    let both = "# s\n\nRun `omhl list` and `omhl check run`.\n";
    let uncited = tree(&[("s/SKILL.md", Some("Run `omhl list`.\n".to_owned()))]);
    let nonexistent = tree(&[(
        "s/SKILL.md",
        Some(format!("{both}Also `omhl listt` and `omhl check nope`.\n")),
    )]);
    let long = tree(&[(
        "s/SKILL.md",
        Some(format!("{both}{}", "filler\n".repeat(MAX_SKILL_LINES))),
    )]);
    let no_file = tree(&[("s/references/check-run.md", None)]);
    let no_title = tree(&[(
        "s/references/list.md",
        Some(reference("- `known-code` (saída 1): x.").replace("## Saída\n\n", "")),
    )]);
    let ghost = tree(&[(
        "s/references/list.md",
        Some(reference("- `ghost-code` (saída 1): x.")),
    )]);
    let group_only = tree(&[("s/SKILL.md", Some(format!("{both}See `omhl check`.\n")))]);
    let clap_usage = tree(&[(
        "s/references/list.md",
        Some(reference("- Saída 2: erro de uso do clap, sem código.")),
    )]);
    let hook_group = tree(&[(
        "s/SKILL.md",
        Some(format!("{both}Not ours: `omhl hook x`.\n")),
    )]);

    let mut failures = Vec::new();
    for (kind, flagged) in [
        (0, vec![uncited]),
        (1, vec![nonexistent]),
        (2, vec![long]),
        (3, vec![no_file, no_title]),
        (4, vec![ghost]),
    ] {
        let flagged: Vec<&str> = flagged.iter().map(String::as_str).collect();
        let accepted = [
            complete.as_str(),
            group_only.as_str(),
            clap_usage.as_str(),
            hook_group.as_str(),
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
        "check retire",
        "check run",
        "check sweep",
        "close",
        "gate-pr",
        "gate-worktree",
        "get",
        "guideline retire",
        "hooks install",
        "hooks post-commit",
        "list",
        "load",
        "project",
        "render",
        "report answers",
        "report recurrences",
        "report retro",
        "spec init",
        "spec validate",
        "verify",
    ];

    let leaves = leaf_commands();

    assert_eq!(leaves, expected);
}

#[test]
fn it_should_prove_each_coverage_failure_on_a_synthetic_skills_folder() {
    let failures = prove_checks();

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Cobertura das skills reais em plugin/skills: reprova subcomando sem citação ou sem references.
#[test]
fn skill_coverage() {
    let mut failures = prove_checks();
    let findings = check(&skills_dir(), &leaf_commands(), &read_sources(&repo_root()));
    failures.extend(findings.messages());

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
