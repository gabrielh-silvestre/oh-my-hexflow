//! Travas de camadas do crate `omhl`: o `main` sem lógica de domínio, o `native` como folha e o
//! `gate` sem conhecer o domínio. Os fontes são lidos como dado; cada regra prova o varredor
//! contra trechos que devem e que não devem ser acusados antes de varrer o código real.

use std::fs;
use std::path::{Path, PathBuf};

const MAIN_LINES_MAX: usize = 30;
const NATIVE_FORBIDDEN_MODULES: &[&str] = &["gate", "facade", "guard", "pr_gate"];
const NATIVE_FORBIDDEN_PATHS: &[&str] = &["omhl::"];
const GATE_FORBIDDEN_MODULES: &[&str] = &["native", "graphs", "pr_gate"];
const GATE_FORBIDDEN_LITERALS: &[&str] = &["check", "work-item", "work_item", "attestation"];
const HEXLOG_PATHS: &[&str] = &["hexlog::"];
const HEXLOG_IMPORTS: &[&str] = &["use hexlog", "extern crate hexlog"];
const STDOUT_MACROS: &[&str] = &["println!", "print!", "dbg!"];
const FORBIDDEN_PACKAGES: &[&str] = &[
    "tokio",
    "hyper",
    "reqwest",
    "ureq",
    "git2",
    "async-std",
    "serde_yaml",
];
const FORBIDDEN_PACKAGE_PREFIX: &str = "gix";
const PURE_LAYER_DIRS: &[&str] = &["extensions", "projection"];
const PURE_LAYER_FORBIDDEN_MODULES: &[&str] = &["adapters", "app", "facade"];
const PURE_LAYER_FORBIDDEN_PATHS: &[&str] = &["adapters::", "app::", "facade::"];
const TEMPFILE_ALLOWED_FILE: &str = "projection/write.rs";
const EXPECTED_DEPENDENCIES: &[&str] = &[
    "clap",
    "hexlog",
    "serde",
    "serde_json",
    "tempfile",
    "toml",
    "uuid",
];
const ATOMIC_PATHS: &[&str] = &["sync::atomic", "atomic::"];

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_files(dir, &mut files);
    files.sort();
    files
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|found| found == "rs") {
            files.push(path);
        }
    }
}

// ------------------------------------------------------------ léxico mínimo

/// Código sem comentários e com o miolo das strings e dos chars esvaziado, mais o texto de cada
/// literal de string.
struct Lexed {
    code: String,
    literals: Vec<String>,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn lex(source: &str) -> Result<Lexed, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut code = String::new();
    let mut literals = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i = skip_block_comment(&chars, i)?;
            code.push(' ');
        } else if let Some((prefix, hashes)) = raw_string_start(&chars, i) {
            let (text, end) = read_raw_string(&chars, i + prefix, hashes)?;
            literals.push(text);
            code.push_str("\"\"");
            i = end;
        } else if c == '"' || (c == 'b' && next == Some('"') && !prev_is_ident(&chars, i)) {
            let start = if c == 'b' { i + 2 } else { i + 1 };
            let (text, end) = read_string(&chars, start)?;
            literals.push(text);
            code.push_str("\"\"");
            i = end;
        } else if c == '\'' {
            i = skip_char_or_lifetime(&chars, i, &mut code);
        } else {
            code.push(c);
            i += 1;
        }
    }

    Ok(Lexed { code, literals })
}

fn prev_is_ident(chars: &[char], i: usize) -> bool {
    i > 0 && is_ident(chars[i - 1])
}

fn skip_block_comment(chars: &[char], start: usize) -> Result<usize, String> {
    let mut depth = 0;
    let mut i = start;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            depth += 1;
            i += 2;
        } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Ok(i);
            }
        } else {
            i += 1;
        }
    }

    Err("unterminated block comment".to_owned())
}

/// `r"`, `r#"`, `br"`...: devolve o tamanho do prefixo até a primeira aspa e o número de `#`.
fn raw_string_start(chars: &[char], i: usize) -> Option<(usize, usize)> {
    if prev_is_ident(chars, i) {
        return None;
    }
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }

    (chars.get(j) == Some(&'"')).then_some((j + 1 - i, hashes))
}

fn read_raw_string(chars: &[char], start: usize, hashes: usize) -> Result<(String, usize), String> {
    let mut i = start;
    while i < chars.len() {
        let closes =
            chars[i] == '"' && (1..=hashes).all(|offset| chars.get(i + offset) == Some(&'#'));
        if closes {
            return Ok((chars[start..i].iter().collect(), i + 1 + hashes));
        }
        i += 1;
    }

    Err("unterminated raw string".to_owned())
}

fn read_string(chars: &[char], start: usize) -> Result<(String, usize), String> {
    let mut text = String::new();
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            '"' => return Ok((text, i + 1)),
            '\\' => {
                if let Some(&escaped) = chars.get(i + 1) {
                    text.push(escaped);
                }
                i += 2;
            }
            other => {
                text.push(other);
                i += 1;
            }
        }
    }

    Err("unterminated string".to_owned())
}

/// `'x'` e `'\n'` são chars (esvaziados); `'a` sem fechamento é lifetime (mantido).
fn skip_char_or_lifetime(chars: &[char], start: usize, code: &mut String) -> usize {
    let escaped = chars.get(start + 1) == Some(&'\\');
    let close = if escaped { start + 3 } else { start + 2 };
    if chars.get(close) == Some(&'\'') {
        code.push_str("''");
        return close + 1;
    }
    code.push('\'');

    start + 1
}

// -------------------------------------------------------------- varredores

/// Número de referências `crate::<módulo>`, inclusive dentro de `crate::{ ... }`.
fn module_refs(source: &str, modules: &[&str]) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);
    let mut count = 0;
    for module in modules {
        count += occurrences(&code, &format!("crate::{module}"), true);
    }
    count += grouped_module_refs(&code, modules);

    Ok(count)
}

/// Tira o espaço em volta de `::`, `{`, `}` e `,` para `crate :: gate` valer como `crate::gate`.
fn tighten(code: &str) -> String {
    let mut text = code.split_whitespace().collect::<Vec<_>>().join(" ");
    for token in ["::", "{", "}", ","] {
        text = text.replace(&format!(" {token}"), token);
        text = text.replace(&format!("{token} "), token);
    }

    text
}

/// Itens de topo de cada `crate::{...}` que começam por um dos módulos.
fn grouped_module_refs(code: &str, modules: &[&str]) -> usize {
    let mut count = 0;
    let mut rest = code;
    while let Some(at) = rest.find("crate::{") {
        let group = &rest[at + "crate::{".len()..];
        let mut depth = 0;
        let mut item_start = 0;
        for (index, c) in group.char_indices() {
            let ends_item = match c {
                '{' => {
                    depth += 1;
                    false
                }
                '}' if depth == 0 => true,
                '}' => {
                    depth -= 1;
                    false
                }
                ',' => depth == 0,
                _ => false,
            };
            if ends_item {
                count += usize::from(starts_with_module(&group[item_start..index], modules));
                item_start = index + 1;
                if c == '}' {
                    break;
                }
            }
        }
        rest = group;
    }

    count
}

fn starts_with_module(item: &str, modules: &[&str]) -> bool {
    modules.iter().any(|module| {
        item.strip_prefix(module)
            .is_some_and(|after| after.chars().next().is_none_or(|c| !is_ident(c)))
    })
}

/// Número de ocorrências de um caminho (`omhl::`, `hexlog::`...) no código, fora de comentário e
/// de string, sem identificador colado antes.
fn path_refs(source: &str, paths: &[&str]) -> Result<usize, String> {
    let code = lex(source)?.code;

    Ok(paths
        .iter()
        .map(|path| occurrences(&code, path, false))
        .sum())
}

/// `ident_after`: exige que o que vem depois da ocorrência não continue um identificador.
fn occurrences(code: &str, needle: &str, ident_after: bool) -> usize {
    code.match_indices(needle)
        .filter(|(at, _)| {
            let before = code[..*at].chars().next_back();
            let after = code[at + needle.len()..].chars().next();
            before.is_none_or(|c| !is_ident(c)) && !(ident_after && after.is_some_and(is_ident))
        })
        .count()
}

/// Número de literais de string cujo conteúdo é exatamente um dos nomes.
fn literal_refs(source: &str, banned: &[&str]) -> Result<usize, String> {
    Ok(lex(source)?
        .literals
        .iter()
        .filter(|text| banned.contains(&text.as_str()))
        .count())
}

// ------------------------------------------------------ prova dos dois lados

fn prove_both_sides(
    count: impl Fn(&str) -> Result<usize, String>,
    flagged: &[&str],
    accepted: &[&str],
) -> Vec<String> {
    let mut failures = Vec::new();
    for snippet in flagged {
        match count(snippet) {
            Ok(0) => failures.push(format!("embedded snippet was not flagged: {snippet}")),
            Ok(_) => {}
            Err(error) => {
                failures.push(format!("embedded snippet did not lex ({error}): {snippet}"))
            }
        }
    }
    for snippet in accepted {
        match count(snippet) {
            Ok(0) => {}
            Ok(_) => failures.push(format!("embedded snippet was flagged: {snippet}")),
            Err(error) => {
                failures.push(format!("embedded snippet did not lex ({error}): {snippet}"))
            }
        }
    }

    failures
}

fn scan_dir(dir: &Path, count: impl Fn(&str) -> Result<usize, String>) -> Vec<String> {
    let files = rust_files(dir);
    let mut failures = Vec::new();
    if files.is_empty() {
        failures.push(format!("no Rust sources under {}", dir.display()));
    }
    for file in files {
        match count(&read(&file)) {
            Ok(0) => {}
            Ok(found) => failures.push(format!(
                "{}: {found} forbidden reference(s)",
                file.display()
            )),
            Err(error) => failures.push(format!("{}: {error}", file.display())),
        }
    }

    failures
}

fn scan_paths(paths: &[PathBuf], count: impl Fn(&str) -> Result<usize, String>) -> Vec<String> {
    let mut failures = Vec::new();
    for path in paths {
        if path.is_dir() {
            failures.extend(scan_dir(path, &count));
        } else if path.is_file() {
            match count(&read(path)) {
                Ok(0) => {}
                Ok(found) => failures.push(format!(
                    "{}: {found} forbidden reference(s)",
                    path.display()
                )),
                Err(error) => failures.push(format!("{}: {error}", path.display())),
            }
        } else {
            failures.push(format!("missing source path {}", path.display()));
        }
    }

    failures
}

/// Referências a `hexlog` fora de comentário e de string: o caminho `hexlog::` e as formas de
/// importar a crate inteira (`use hexlog`, `extern crate hexlog`).
fn hexlog_refs(source: &str) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);
    let imports: usize = HEXLOG_IMPORTS
        .iter()
        .map(|import| occurrences(&code, import, true))
        .sum();

    Ok(path_refs(source, HEXLOG_PATHS)? + imports)
}

/// Referências a um módulo `adapters` por qualquer caminho de camada interna.
fn adapters_refs(source: &str) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);

    Ok(module_refs(source, &["adapters"])?
        + occurrences(&code, "omhl::adapters", true)
        + path_refs(source, &["adapters::"])?)
}

/// Macros que escrevem direto no stdout do processo, com ou sem espaço antes do `!`.
fn stdout_macro_refs(source: &str) -> Result<usize, String> {
    let mut code = lex(source)?.code;
    for open in ["(", "[", "{"] {
        code = code.replace(&format!(" !{open}"), &format!("!{open}"));
    }

    Ok(STDOUT_MACROS
        .iter()
        .map(|name| occurrences(&code, name, false))
        .sum())
}

/// Pacotes proibidos em um `Cargo.lock`, lidos como dado: só as linhas `name = "..."`.
fn forbidden_lock_packages(lock: &str) -> Result<usize, String> {
    Ok(lock
        .lines()
        .filter_map(|line| line.trim_end().strip_prefix("name = \"")?.strip_suffix('"'))
        .filter(|name| {
            FORBIDDEN_PACKAGES.contains(name) || name.starts_with(FORBIDDEN_PACKAGE_PREFIX)
        })
        .count())
}

fn assert_no_failures(failures: &[String]) {
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

// ------------------------------------------------------------------- testes

#[test]
fn it_should_keep_main_free_of_domain_logic_and_hexlog_paths() {
    let count = |source: &str| path_refs(source, &["hexlog::"]);
    let main = read(&src_dir().join("main.rs"));
    let mut failures = prove_both_sides(
        count,
        &[
            "use hexlog::Hexlog;",
            "fn main() { let c = hexlog::Config::default(); }",
        ],
        &[
            "// hexlog::Hexlog is only mentioned here",
            "fn main() { println!(\"hexlog::\"); }",
            "use my_hexlog::Thing;",
        ],
    );

    match count(&main) {
        Ok(0) => {}
        Ok(found) => failures.push(format!("src/main.rs: {found} `hexlog::` reference(s)")),
        Err(error) => failures.push(format!("src/main.rs: {error}")),
    }
    let lines = main.lines().count();
    if lines > MAIN_LINES_MAX {
        failures.push(format!(
            "src/main.rs has {lines} lines, above {MAIN_LINES_MAX}"
        ));
    }

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_the_native_sources_free_of_gate_facade_guard_and_pr_gate_imports() {
    let count = |source: &str| {
        Ok(module_refs(source, NATIVE_FORBIDDEN_MODULES)?
            + path_refs(source, NATIVE_FORBIDDEN_PATHS)?)
    };
    let mut failures = prove_both_sides(
        count,
        &[
            "use crate::gate::Question;",
            "use crate::facade::Omhl;",
            "use crate::guard::attestation_id;",
            "let name = crate::pr_gate::GATE_NAME;",
            "use crate::{error::OmhlError, facade::Omhl};",
            "use crate::{error::OmhlError, gate::{GateReport, Question}};",
            "use omhl::facade::Omhl;",
        ],
        &[
            "use crate::error::OmhlError;",
            "use crate::{error::OmhlError, probe::SnapshotProbe};",
            "// use crate::gate::Question;",
            "/* crate::facade::Omhl */ fn f() {}",
            "const PATH: &str = \"crate::guard::attestation_id\";",
            "use crate::gate_helpers::Thing;",
        ],
    );

    failures.extend(scan_dir(&src_dir().join("native"), count));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_the_gate_sources_free_of_domain_names_and_native_imports() {
    let count = |source: &str| {
        Ok(module_refs(source, GATE_FORBIDDEN_MODULES)?
            + literal_refs(source, GATE_FORBIDDEN_LITERALS)?)
    };
    let mut failures = prove_both_sides(
        count,
        &[
            "use crate::native::NativeBase;",
            "use crate::graphs::catalog;",
            "use crate::pr_gate::GATE_NAME;",
            "use crate::{error::OmhlError, native::NativeCheck};",
            "let kind = \"check\";",
            "let kind = \"work-item\";",
            "let kind = \"work_item\";",
            "let kind = r#\"attestation\"#;",
        ],
        &[
            "use crate::error::OmhlError;",
            "// use crate::native::NativeBase;",
            "// the \"check\" type is not named here",
            "let text = \"a check passed\";",
            "let label = \"occurred\";",
            "use crate::native_like::Thing;",
        ],
    );

    failures.extend(scan_dir(&src_dir().join("gate"), count));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_adapters_free_of_hexlog_imports() {
    let mut failures = prove_both_sides(
        hexlog_refs,
        &[
            "use hexlog::Hexlog;",
            "use ::hexlog::Hexlog;",
            "use hexlog;",
            "use hexlog as store;",
            "extern crate hexlog;",
            "fn f() { let c = hexlog::Config::default(); }",
        ],
        &[
            "// use hexlog::Hexlog;",
            "const PATH: &str = \"hexlog::Hexlog\";",
            "use my_hexlog::Thing;",
            "use hexlogger::Thing;",
            "use crate::app::App;",
        ],
    );

    failures.extend(scan_dir(&src_dir().join("adapters"), hexlog_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_not_import_adapters_from_the_inner_layers() {
    let mut failures = prove_both_sides(
        adapters_refs,
        &[
            "use crate::adapters::cli;",
            "use crate::adapters;",
            "use crate::{app::App, adapters::cli};",
            "use omhl::adapters::cli::run;",
            "use super::adapters::mcp;",
        ],
        &[
            "// use crate::adapters::cli;",
            "const PATH: &str = \"crate::adapters::cli\";",
            "use crate::app::App;",
            "use crate::my_adapters::Thing;",
            "use crate::adapters_like::Thing;",
        ],
    );
    let inner = [
        src_dir().join("app"),
        src_dir().join("render.rs"),
        src_dir().join("project"),
        src_dir().join("ports"),
    ];

    failures.extend(scan_paths(&inner, adapters_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_stdout_writes_out_of_every_source_file() {
    let mut failures = prove_both_sides(
        stdout_macro_refs,
        &[
            "fn f() { println!(\"x\"); }",
            "fn f() { print!(\"x\"); }",
            "fn f() { dbg!(value); }",
            "fn f() { std::println!(\"x\"); }",
            "fn f() { println !(\"x\"); }",
        ],
        &[
            "fn f() { eprintln!(\"x\"); }",
            "fn f() { eprint!(\"x\"); }",
            "fn f() { writeln!(out, \"x\").ok(); }",
            "// println!(\"x\")",
            "const TEXT: &str = \"println!(x)\";",
            "fn f() { my_println!(\"x\"); }",
        ],
    );

    failures.extend(scan_dir(&src_dir(), stdout_macro_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_forbidden_crates_out_of_the_lockfile() {
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    let mut failures = prove_both_sides(
        forbidden_lock_packages,
        &[
            "[[package]]\nname = \"tokio\"\nversion = \"1.0.0\"\n",
            "name = \"hyper\"",
            "name = \"reqwest\"",
            "name = \"ureq\"",
            "name = \"git2\"",
            "name = \"async-std\"",
            "name = \"serde_yaml\"",
            "name = \"gix\"",
            "name = \"gix-hash\"",
        ],
        &[
            "name = \"hexlog\"",
            "name = \"serde\"",
            "version = \"tokio\"",
            "dependencies = [\n \"serde\",\n]",
        ],
    );

    match fs::read_to_string(&lock) {
        Ok(text) => match forbidden_lock_packages(&text) {
            Ok(0) => {}
            Ok(found) => failures.push(format!("Cargo.lock: {found} forbidden package(s)")),
            Err(error) => failures.push(format!("Cargo.lock: {error}")),
        },
        Err(error) => failures.push(format!("cannot read {}: {error}", lock.display())),
    }

    assert_no_failures(&failures);
}

// ------------------------------------------------- travas do passo 6

/// Referências a `adapters`, `app` ou `facade` por `crate::`, `super::` ou caminho solto.
fn pure_layer_refs(source: &str) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);
    let super_refs: usize = PURE_LAYER_FORBIDDEN_MODULES
        .iter()
        .map(|module| occurrences(&code, &format!("super::{module}"), true))
        .sum();

    Ok(module_refs(source, PURE_LAYER_FORBIDDEN_MODULES)?
        + path_refs(source, PURE_LAYER_FORBIDDEN_PATHS)?
        + super_refs)
}

/// Menções ao crate `tempfile` fora de comentário e de string.
fn tempfile_refs(source: &str) -> Result<usize, String> {
    Ok(occurrences(&lex(source)?.code, "tempfile", true))
}

/// Tipos atômicos (`AtomicU64`, `AtomicBool`...) e caminhos do módulo `atomic`.
fn atomic_refs(source: &str) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);
    let types = code
        .match_indices("Atomic")
        .filter(|(at, _)| {
            let before = code[..*at].chars().next_back();
            let after = code[at + "Atomic".len()..].chars().next();
            before.is_none_or(|c| !is_ident(c)) && after.is_some_and(|c| c.is_ascii_uppercase())
        })
        .count();
    let paths: usize = ATOMIC_PATHS
        .iter()
        .map(|path| occurrences(&code, path, false))
        .sum();

    Ok(types + paths)
}

/// Falhas de um `Cargo.toml` lido como dado: a tabela `[dependencies]` precisa ter exatamente os
/// pacotes esperados (`tempfile` só em dev-dependencies não conta).
fn dependency_failures(manifest: &str) -> Vec<String> {
    let table: toml::Table = match toml::from_str(manifest) {
        Ok(table) => table,
        Err(error) => return vec![format!("manifest is not valid TOML: {error}")],
    };
    let Some(dependencies) = table.get("dependencies").and_then(toml::Value::as_table) else {
        return vec!["manifest has no [dependencies] table".to_owned()];
    };
    let found: Vec<&str> = dependencies.keys().map(String::as_str).collect();
    let mut failures = Vec::new();
    for name in &found {
        if !EXPECTED_DEPENDENCIES.contains(name) {
            failures.push(format!("unexpected dependency: {name}"));
        }
    }
    for name in EXPECTED_DEPENDENCIES {
        if !found.contains(name) {
            failures.push(format!("missing dependency: {name}"));
        }
    }

    failures
}

fn relative_to_src(file: &Path) -> String {
    file.strip_prefix(src_dir())
        .unwrap_or(file)
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

const EXACT_MANIFEST: &str = r#"[package]
name = "omhl"

[dependencies]
clap = "1"
hexlog = { path = "../hexlog" }
serde = "1"
serde_json = "1"
tempfile = "3"
toml = "1"
uuid = "1"
"#;

#[test]
fn it_should_not_import_adapters_app_or_facade_in_extensions_and_projection() {
    let mut failures = prove_both_sides(
        pure_layer_refs,
        &[
            "use crate::adapters::cli::Cli;",
            "use crate::app::engine::Engine;",
            "use crate::facade::Omhl;",
            "use crate::{error::OmhlError, app::Action};",
            "use super::app::Runtime;",
            "let run = omhl::adapters::hook::run;",
        ],
        &[
            "use crate::error::OmhlError;",
            "use crate::project::layout::OMHL_DIR;",
            "// use crate::app::engine::Engine;",
            "const PATH: &str = \"crate::facade::Omhl\";",
            "use crate::application::Thing;",
            "use my_app::Thing;",
        ],
    );

    for dir in PURE_LAYER_DIRS {
        failures.extend(scan_dir(&src_dir().join(dir), pure_layer_refs));
    }

    assert_no_failures(&failures);
}

#[test]
fn it_should_use_tempfile_only_in_projection_write() {
    let mut failures = prove_both_sides(
        tempfile_refs,
        &[
            "use tempfile::NamedTempFile;",
            "let file = tempfile::tempfile();",
            "use tempfile::{Builder, TempDir};",
        ],
        &[
            "// tempfile is used elsewhere",
            "const NAME: &str = \"tempfile\";",
            "use my_tempfile::Thing;",
            "fn tempfile_name() {}",
        ],
    );
    let files = rust_files(&src_dir());
    if files.is_empty() {
        failures.push(format!("no Rust sources under {}", src_dir().display()));
    }

    for file in &files {
        let name = relative_to_src(file);
        if name == TEMPFILE_ALLOWED_FILE {
            continue;
        }
        match tempfile_refs(&read(file)) {
            Ok(0) => {}
            Ok(found) => failures.push(format!(
                "{name}: {found} `tempfile` reference(s) outside {TEMPFILE_ALLOWED_FILE}"
            )),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }

    assert_no_failures(&failures);
}

#[test]
fn it_should_declare_exactly_the_expected_direct_dependencies() {
    let with_extra = format!("{EXACT_MANIFEST}tokio = \"1\"\n");
    let tempfile_only_in_dev = EXACT_MANIFEST.replace("tempfile = \"3\"\n", "")
        + "\n[dev-dependencies]\ntempfile = \"3\"\n";
    let mut failures = Vec::new();
    if !dependency_failures(EXACT_MANIFEST).is_empty() {
        failures.push("embedded exact manifest was flagged".to_owned());
    }
    if dependency_failures(&with_extra).is_empty() {
        failures.push("embedded manifest with an extra dependency was not flagged".to_owned());
    }
    if dependency_failures(&tempfile_only_in_dev).is_empty() {
        failures.push("embedded manifest with tempfile only in dev was not flagged".to_owned());
    }

    let manifest = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"));
    failures.extend(dependency_failures(&manifest));

    assert_no_failures(&failures);
}

#[test]
fn it_should_not_use_atomics_anywhere_in_src() {
    let mut failures = prove_both_sides(
        atomic_refs,
        &[
            "use std::sync::atomic::AtomicU64;",
            "static COUNTER: AtomicBool = AtomicBool::new(false);",
            "use std::sync::atomic::Ordering;",
            "use core::sync::atomic;",
            "use std::sync::{atomic::Ordering, Arc};",
            "let n: AtomicUsize = make();",
        ],
        &[
            "// AtomicU64 is not used here",
            "const NAME: &str = \"AtomicBool\";",
            "fn atomically() {}",
            "struct Atomically;",
            "use crate::atomic_write::Thing;",
        ],
    );

    failures.extend(scan_dir(&src_dir(), atomic_refs));

    assert_no_failures(&failures);
}

// ------------------------------------------------- travas do passo 6: adaptador hook

const HOOK_SOURCE: &str = "adapters/hook.rs";
const MARKER_SEARCH: &str = "find_marker";

/// Chamadas ao `find_marker` fora de comentário e de string.
fn marker_search_refs(source: &str) -> Result<usize, String> {
    Ok(occurrences(&lex(source)?.code, MARKER_SEARCH, true))
}

#[test]
fn it_should_not_search_the_marker_twice_in_the_hook() {
    let mut failures = prove_both_sides(
        marker_search_refs,
        &[
            "let marker = find_marker(&cwd)?;",
            "let marker = project::marker::find_marker(&cwd)?;",
        ],
        &[
            "// find_marker is called by the runtime",
            "const NAME: &str = \"find_marker\";",
            "fn find_marker_twice() {}",
        ],
    );

    failures.extend(scan_paths(
        &[src_dir().join(HOOK_SOURCE)],
        marker_search_refs,
    ));

    assert_no_failures(&failures);
}
