//! Travas de camadas do crate `omhl`: o `main` sem lógica de domínio, o `native` como folha e o
//! `gate` sem conhecer o domínio. Os fontes são lidos como dado; cada regra prova o varredor
//! contra trechos que devem e que não devem ser acusados antes de varrer o código real.

use std::fs;
use std::path::{Path, PathBuf};

mod support;

use support::scan::prove_both_sides;

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
const PURE_LAYER_DIRS: &[&str] = &["extensions", "projection", "mechanical", "workflow"];
const PURE_LAYER_FORBIDDEN_MODULES: &[&str] = &["adapters", "app", "facade"];
const PURE_LAYER_FORBIDDEN_PATHS: &[&str] = &["adapters::", "app::", "facade::"];
const TEMPFILE_ALLOWED_FILE: &str = "projection/write.rs";
const EXPECTED_DEPENDENCIES: &[&str] = &[
    "clap",
    "hexlog",
    "nix",
    "regex",
    "serde",
    "serde_json",
    "sha2",
    "tempfile",
    "toml",
    "uuid",
];
const ATOMIC_PATHS: &[&str] = &["sync::atomic", "atomic::"];
const MECHANICAL_FORBIDDEN_MODULES: &[&str] =
    &["adapters", "app", "facade", "ports", "runner", "byproduct"];
const WORKFLOW_FORBIDDEN_MODULES: &[&str] = &["ports", "runner", "byproduct"];
const PORTS_FORBIDDEN_MODULES: &[&str] = &["extensions"];
const SERVICE_FORBIDDEN_MODULES: &[&str] = &["adapters", "app", "facade"];
const ADAPTERS_ALLOWED: &[&str] = &["crate::app", "crate::project::Env", "super::"];
const SPEC_GRAMMAR_LITERALS: &[&str] = &["wi-", "parte-", "emenda-"];
const SPEC_GRAMMAR_MODULE: &str = "workflow/names.rs";
const REMOVED_PORT_TERMS: &[&str] = &["mcp", "tool_call", "json-rpc"];
const NUMERIC_CUT_NEEDLES: &[&str] = &[".min(", ".max(", ".clamp(", ".take(", "saturating_"];
/// Cortes numéricos de `src/app` por arquivo, os únicos justificados; um corte novo entra aqui com a
/// razão ao lado e, se for um caso mantido, com a linha em "Avaliados e mantidos" do ADR 0007, nunca sozinho.
const NUMERIC_CUTS_IN_APP: &[(&str, usize)] = &[
    // janela da cauda: saturating_sub(wanted) devolve menos linhas que o pedido; --tail acima de 500 já recusa (I1)
    ("byproduct_tail.rs", 1),
    // output_tail do resumo da falha (20 linhas de 500 caracteres); a saída inteira fica no byproduct
    ("check_run.rs", 2),
    // listas de erros cortadas em ISSUES_MAX, sempre com omitted; o corte é avisado
    ("error.rs", 4),
    // lê 1 byte além de INPUT_MAX_BYTES para recusar a entrada grande demais; sentinela de recusa, não corte
    ("input.rs", 1),
    // lotes de BATCH_SIZE; nada é descartado, o resto vai no lote seguinte
    ("load.rs", 1),
];
const PARENT_PID_CALL: &str = "std::os::unix::process::parent_id()";

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
    count += grouped_refs(&code, "crate::", modules);

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

/// Itens de topo de cada `<prefix>{...}` que começam por um dos módulos.
fn grouped_refs(code: &str, prefix: &str, modules: &[&str]) -> usize {
    let opener = format!("{prefix}{{");
    let mut count = 0;
    let mut rest = code;
    while let Some(at) = rest.find(&opener) {
        let group = &rest[at + opener.len()..];
        count += group_items(group)
            .iter()
            .filter(|item| starts_with_module(item, modules))
            .count();
        rest = group;
    }

    count
}

/// Itens de topo de um grupo, lido a partir do texto logo depois da chave de abertura.
fn group_items(group: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0;
    let mut item_start = 0;
    for (index, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => {
                items.push(&group[item_start..index]);
                break;
            }
            '}' => depth -= 1,
            ',' if depth == 0 => {
                items.push(&group[item_start..index]);
                item_start = index + 1;
            }
            _ => {}
        }
    }

    items
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

/// Número de literais de string que começam por um dos prefixos da gramática da spec.
fn spec_grammar_refs(source: &str) -> Result<usize, String> {
    Ok(lex(source)?
        .literals
        .iter()
        .filter(|text| {
            SPEC_GRAMMAR_LITERALS
                .iter()
                .any(|prefix| text.starts_with(prefix))
        })
        .count())
}

/// Cortes numéricos (`.min(`, `.max(`, `.clamp(`, `.take(`, `saturating_`) fora de comentário e de string.
fn numeric_cut_refs(source: &str) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);

    Ok(NUMERIC_CUT_NEEDLES
        .iter()
        .map(|needle| code.matches(needle).count())
        .sum())
}

/// Arquivos de `dir` cuja contagem de cortes numéricos difere da lista fechada.
fn numeric_cut_mismatches(dir: &Path, allowed: &[(&str, usize)]) -> Vec<String> {
    let mut failures = Vec::new();
    for file in rust_files(dir) {
        let relative = file
            .strip_prefix(dir)
            .unwrap_or(&file)
            .to_string_lossy()
            .into_owned();
        let found = match numeric_cut_refs(&read(&file)) {
            Ok(found) => found,
            Err(error) => {
                failures.push(format!("{relative}: {error}"));
                continue;
            }
        };
        let expected = allowed
            .iter()
            .find(|(name, _)| *name == relative)
            .map_or(0, |(_, count)| *count);
        if found > expected {
            failures.push(format!(
                "{relative}: {found} numeric cut(s), the closed list allows {expected}; refuse the option or report the cut, or record the kept case in docs/adr/0007-opcoes-da-cli-sao-respeitadas-ou-recusadas.md (\"Avaliados e mantidos\") and a one-line reason beside the entry, before raising the list"
            ));
        } else if found < expected {
            failures.push(format!(
                "{relative}: {found} numeric cut(s), the closed list expects {expected}; lower the list"
            ));
        }
    }

    failures
}

// ------------------------------------------------------ prova dos dois lados

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

/// Termos da porta removida em qualquer parte do fonte, comentário e string inclusos, sem diferenciar maiúsculas.
fn removed_port_refs(source: &str) -> Result<usize, String> {
    let lowered = source.to_lowercase();

    Ok(REMOVED_PORT_TERMS
        .iter()
        .map(|term| lowered.matches(term).count())
        .sum())
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

/// Falhas de um `Cargo.toml` lido como dado: `[dependencies]` mais toda `[target.*.dependencies]`
/// precisam ter exatamente os pacotes esperados (`tempfile` só em dev-dependencies não conta).
fn dependency_failures(manifest: &str) -> Vec<String> {
    let table: toml::Table = match toml::from_str(manifest) {
        Ok(table) => table,
        Err(error) => return vec![format!("manifest is not valid TOML: {error}")],
    };
    let Some(dependencies) = table.get("dependencies").and_then(toml::Value::as_table) else {
        return vec!["manifest has no [dependencies] table".to_owned()];
    };
    let target_dependencies = table
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(toml::Table::values)
        .filter_map(|target| target.get("dependencies").and_then(toml::Value::as_table));
    let found: Vec<&str> = std::iter::once(dependencies)
        .chain(target_dependencies)
        .flat_map(|dependencies| dependencies.keys().map(String::as_str))
        .collect();
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

/// `[dependencies].sha2` de um manifesto: texto da versão (string ou campo `version` da tabela inline).
fn sha2_declared_version(manifest: &str) -> Result<Option<String>, ()> {
    let table: toml::Table = toml::from_str(manifest).map_err(|_| ())?;
    let declared = table
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .and_then(|dependencies| dependencies.get("sha2"));
    let version = match declared {
        Some(toml::Value::String(version)) => Some(version.clone()),
        Some(toml::Value::Table(inline)) => inline
            .get("version")
            .and_then(toml::Value::as_str)
            .map(str::to_owned),
        _ => None,
    };

    Ok(version)
}

/// Falhas do pino do `sha2`: o `omhl` e o motor declaram a mesma versão em `[dependencies]`.
fn sha2_pin_failures(omhl_manifest: &str, hexlog_manifest: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let mut versions = Vec::new();
    for (name, manifest) in [("omhl", omhl_manifest), ("hexlog", hexlog_manifest)] {
        match sha2_declared_version(manifest) {
            Err(()) => failures.push(format!("manifest is not valid TOML: {name}")),
            Ok(None) => failures.push(format!("sha2 missing in manifest: {name}")),
            Ok(Some(version)) => versions.push(version),
        }
    }
    if let [omhl, hexlog] = versions.as_slice()
        && omhl != hexlog
    {
        failures.push(format!(
            "sha2 version differs: omhl {omhl}, hexlog {hexlog}"
        ));
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
regex = "=1.13.1"
serde = "1"
serde_json = "1"
sha2 = "=0.11.1"
tempfile = "3"
toml = "1"
uuid = "1"

[target.'cfg(target_os = "linux")'.dependencies]
nix = { version = "=0.31.3", default-features = false, features = ["signal", "process"] }
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
    let with_extra = EXACT_MANIFEST.replace("uuid = \"1\"\n", "uuid = \"1\"\ntokio = \"1\"\n");
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

// ------------------------------------------------- travas dos gates mecânicos

/// Referências aos módulos por `crate::` (solto ou agrupado), por caminho solto e por `super::`.
fn forbidden_module_refs(source: &str, modules: &[&str]) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);
    let mut count = module_refs(source, modules)?;
    for module in modules {
        count += occurrences(&code, &format!("{module}::"), false)
            + occurrences(&code, &format!("super::{module}"), true);
    }

    Ok(count)
}

fn mechanical_refs(source: &str) -> Result<usize, String> {
    forbidden_module_refs(source, MECHANICAL_FORBIDDEN_MODULES)
}

fn workflow_refs(source: &str) -> Result<usize, String> {
    forbidden_module_refs(source, WORKFLOW_FORBIDDEN_MODULES)
}

fn ports_refs(source: &str) -> Result<usize, String> {
    forbidden_module_refs(source, PORTS_FORBIDDEN_MODULES)
}

fn service_refs(source: &str) -> Result<usize, String> {
    forbidden_module_refs(source, SERVICE_FORBIDDEN_MODULES)
}

/// Caminhos `nix::` e a importação da crate inteira (`use nix`).
fn nix_refs(source: &str) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);

    Ok(occurrences(&code, "nix::", false) + occurrences(&code, "use nix", true))
}

/// `Command`, `Child` e `Stdio` de `std::process`, soltos, agrupados ou pelo módulo importado.
fn process_spawn_refs(source: &str) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);
    let spawn_types = ["Command", "Child", "Stdio"];
    let module_imported = occurrences(&code, "use std::process;", false) > 0;
    let mut count = grouped_refs(&code, "std::process::", &spawn_types);
    for name in spawn_types {
        count += occurrences(&code, &format!("std::process::{name}"), true);
        if module_imported {
            count += occurrences(&code, &format!("process::{name}"), true);
        }
    }

    Ok(count)
}

fn thread_scope_refs(source: &str) -> Result<usize, String> {
    Ok(occurrences(
        &tighten(&lex(source)?.code),
        "thread::scope",
        true,
    ))
}

/// Um caminho é aceito quando começa por um prefixo de `ADAPTERS_ALLOWED` sem identificador colado.
fn adapters_allowed(path: &str) -> bool {
    ADAPTERS_ALLOWED.iter().any(|allowed| {
        path.strip_prefix(allowed).is_some_and(|after| {
            allowed.ends_with("::") || after.chars().next().is_none_or(|c| !is_ident(c))
        })
    })
}

/// Cada item de `crate::` (solto ou dentro de `crate::{...}`) fora de `ADAPTERS_ALLOWED`, mais
/// todo `super::super::`.
fn adapters_inner_refs(source: &str) -> Result<usize, String> {
    let code = tighten(&lex(source)?.code);
    let mut count = occurrences(&code, "super::super::", false);
    for (at, _) in code.match_indices("crate::") {
        if code[..at].chars().next_back().is_some_and(is_ident) {
            continue;
        }
        count += match code[at + "crate::".len()..].strip_prefix('{') {
            Some(group) => group_items(group)
                .iter()
                .filter(|item| !adapters_allowed(&format!("crate::{item}")))
                .count(),
            None => usize::from(!adapters_allowed(&code[at..])),
        };
    }

    Ok(count)
}

/// 1 se o primeiro comando do corpo de `fn main` (o primeiro trecho terminado em `;` na
/// profundidade 0) não lê o pid do pai, ou se não há `fn main`; 0 caso contrário.
fn parent_pid_not_first(source: &str) -> Result<usize, String> {
    let code = lex(source)?.code;
    let body = code
        .match_indices("fn main")
        .find(|(at, needle)| !code[at + needle.len()..].starts_with(is_ident))
        .and_then(|(at, _)| code[at..].find('{').map(|open| &code[at + open + 1..]));
    let Some(body) = body else {
        return Ok(1);
    };
    let mut depth = 0;
    for (index, c) in body.char_indices() {
        match c {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' if depth == 0 => return Ok(1),
            '}' | ')' | ']' => depth -= 1,
            ';' if depth == 0 => {
                return Ok(usize::from(!body[..index].contains(PARENT_PID_CALL)));
            }
            _ => {}
        }
    }

    Ok(1)
}

#[test]
fn it_should_count_a_target_specific_dependency_as_declared() {
    let without_target = EXACT_MANIFEST.split("[target.").next().unwrap();

    let exact = dependency_failures(EXACT_MANIFEST);
    let missing_nix = dependency_failures(without_target);

    assert_eq!(exact, Vec::<String>::new());
    assert_eq!(missing_nix, ["missing dependency: nix"]);
}

#[test]
fn it_should_report_a_target_specific_dependency_that_is_not_expected() {
    let with_libc = format!("{EXACT_MANIFEST}libc = \"0.2\"\n");

    let failures = dependency_failures(&with_libc);

    assert_eq!(failures, ["unexpected dependency: libc"]);
}

#[test]
fn it_should_pin_sha2_in_the_omhl_manifest_to_the_same_version_as_the_engine_manifest() {
    let pinned = "[dependencies]\nsha2 = \"=0.11.1\"\n";
    let older = "[dependencies]\nsha2 = \"=0.10.9\"\n";
    let without_sha2 = "[dependencies]\nserde = \"1\"\n";
    let omhl_manifest = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"));
    let hexlog_manifest = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../hexlog/Cargo.toml"));

    let same = sha2_pin_failures(pinned, pinned);
    let differing = sha2_pin_failures(older, pinned);
    let missing = sha2_pin_failures(without_sha2, pinned);
    let real = sha2_pin_failures(&omhl_manifest, &hexlog_manifest);

    assert_eq!(same, Vec::<String>::new());
    assert_eq!(
        differing,
        ["sha2 version differs: omhl =0.10.9, hexlog =0.11.1"]
    );
    assert_eq!(missing, ["sha2 missing in manifest: omhl"]);
    assert_eq!(real, Vec::<String>::new());
}

#[test]
fn it_should_keep_the_mechanical_module_free_of_adapters_app_facade_ports_runner_and_byproduct() {
    let mut failures = prove_both_sides(
        mechanical_refs,
        &[
            "use crate::app::App;",
            "use crate::adapters::cli::Cli;",
            "use crate::facade::Omhl;",
            "use crate::ports::git::GitProbe;",
            "use crate::runner::RunSpec;",
            "use crate::byproduct::ByproductHash;",
            "use crate::{project::Env, app::Action};",
        ],
        &[
            "use crate::project::Env;",
            "use super::spec::CheckSpec;",
            "// use crate::app::App;",
            "const P: &str = \"crate::runner::RunSpec\";",
            "use crate::application::Thing;",
        ],
    );

    failures.extend(scan_dir(&src_dir().join("mechanical"), mechanical_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_the_workflow_module_free_of_ports_runner_and_byproduct() {
    let mut failures = prove_both_sides(
        workflow_refs,
        &[
            "use crate::ports::git::GitProbe;",
            "use crate::runner::RunSpec;",
            "use crate::byproduct::ByproductHash;",
            "use crate::{project::Env, ports::git::GitProbe};",
            "use super::ports::Anything;",
            "let probe = ports::git::GitProbe::new(dir);",
        ],
        &[
            "use crate::project::layout::OMHL_DIR;",
            "use super::config::WorkflowConfig;",
            "// use crate::ports::git::GitProbe;",
            "const P: &str = \"crate::runner::RunSpec\";",
            "use crate::reports::Thing;",
        ],
    );

    failures.extend(scan_dir(&src_dir().join("workflow"), workflow_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_the_ports_module_free_of_extensions() {
    let mut failures = prove_both_sides(
        ports_refs,
        &[
            "use crate::extensions::files::read_regular_utf8;",
            "use crate::{project::Env, extensions::files::list_sorted_capped};",
            "use super::extensions::files::read_regular_utf8;",
            "let text = extensions::files::read_regular_utf8(path, max);",
        ],
        &[
            "use crate::ports::files::read_regular_utf8;",
            "use super::files::list_sorted_capped;",
            "// use crate::extensions::files::read_regular_utf8;",
            "const P: &str = \"crate::extensions::files\";",
            "use crate::extensions_extra::Thing;",
        ],
    );

    failures.extend(scan_dir(&src_dir().join("ports"), ports_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_runner_and_byproduct_free_of_adapters_app_and_facade() {
    let mut failures = prove_both_sides(
        service_refs,
        &[
            "use crate::adapters::cli::Cli;",
            "use crate::app::engine::Engine;",
            "use crate::facade::Omhl;",
        ],
        &[
            "use crate::mechanical::spec::CheckSpec;",
            "use std::time::Duration;",
            "// use crate::app::App;",
        ],
    );
    let services = [src_dir().join("runner.rs"), src_dir().join("byproduct.rs")];

    failures.extend(scan_paths(&services, service_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_confine_nix_to_the_process_port() {
    let mut failures = prove_both_sides(
        nix_refs,
        &[
            "use nix::sys::signal::killpg;",
            "let pid = nix::unistd::getpid();",
            "use nix::{sys::signal, unistd::Pid};",
        ],
        &[
            "// nix::sys::signal",
            "const NAME: &str = \"nix::sys\";",
            "use my_nix::Thing;",
            "fn nixie() {}",
            "let p = std::os::unix::process::parent_id();",
        ],
    );
    let outside_process_port: Vec<PathBuf> = rust_files(&src_dir())
        .into_iter()
        .filter(|file| relative_to_src(file) != "ports/process.rs")
        .collect();

    failures.extend(scan_paths(&outside_process_port, nix_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_confine_process_spawning_types_to_the_ports_module() {
    let mut failures = prove_both_sides(
        process_spawn_refs,
        &[
            "use std::process::Command;",
            "use std::process::{Child, Stdio};",
            "let c = std::process::Command::new(\"x\");",
            "use std::process::{Command as Cmd};",
            "use std::process; fn f() { process::Command::new(\"x\"); }",
        ],
        &[
            "use std::process::ExitCode;",
            "// std::process::Command::new(\"x\")",
            "const S: &str = \"std::process::Command\";",
        ],
    );
    let outside_ports: Vec<PathBuf> = rust_files(&src_dir())
        .into_iter()
        .filter(|file| !relative_to_src(file).starts_with("ports/"))
        .collect();

    failures.extend(scan_paths(&outside_ports, process_spawn_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_not_flag_the_exit_code_type_or_the_clap_command_enum_outside_the_ports_module() {
    let mut failures = prove_both_sides(
        process_spawn_refs,
        &[],
        &[
            "use std::process::ExitCode;",
            "pub enum Command { Hook }",
            "Command::Hook",
            "ActionCommand",
            "use clap::Command;",
        ],
    );
    let entry_points = [
        src_dir().join("main.rs"),
        src_dir().join("adapters/cli.rs"),
        src_dir().join("adapters/hook.rs"),
    ];

    failures.extend(scan_paths(&entry_points, process_spawn_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_not_scope_the_threads_of_the_pipe_readers() {
    let mut failures = prove_both_sides(
        thread_scope_refs,
        &[
            "std::thread::scope(|s| { s.spawn(|| {}); });",
            "thread::scope(|s| {});",
        ],
        &[
            "thread::spawn(move || {})",
            "// thread::scope",
            "const S: &str = \"thread::scope\";",
        ],
    );
    let pipe_readers = [
        src_dir().join("ports/pipes.rs"),
        src_dir().join("ports/process.rs"),
    ];

    failures.extend(scan_paths(&pipe_readers, thread_scope_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_adapters_from_importing_the_inner_modules_other_than_app_and_the_project_env() {
    let mut failures = prove_both_sides(
        adapters_inner_refs,
        &[
            "use crate::mechanical::spec::CheckSpec;",
            "use crate::render::x;",
            "use crate::ports::git::GitProbe;",
            "use crate::{app::Action, runner::RunSpec};",
            "use crate::facade::Omhl;",
            "use crate::application::Thing;",
            "use crate::project::layout::OMHL_DIR;",
        ],
        &[
            "use crate::app::RUNNABLE_EVENTS;",
            "use crate::app::action::{Action, Caller};",
            "use crate::project::Env;",
            "use super::hook;",
            "// use crate::render::x;",
            "use crate::{app::Action, project::Env};",
        ],
    );
    let adapters = [
        src_dir().join("adapters/cli.rs"),
        src_dir().join("adapters/hook.rs"),
    ];

    failures.extend(scan_paths(&adapters, adapters_inner_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_read_the_parent_pid_before_anything_else_in_main() {
    let mut failures = prove_both_sides(
        parent_pid_not_first,
        &[
            "fn main() -> ExitCode {\n    let home = std::env::var_os(\"HOME\");\n    let parent_pid = std::os::unix::process::parent_id();\n    ExitCode::SUCCESS\n}",
            "fn main() -> ExitCode {\n    ExitCode::SUCCESS\n}",
        ],
        &[
            "fn main() -> ExitCode {\n    let parent_pid = std::os::unix::process::parent_id();\n    let home = std::env::var_os(\"HOME\");\n    ExitCode::SUCCESS\n}",
        ],
    );

    failures.extend(scan_paths(
        &[src_dir().join("main.rs")],
        parent_pid_not_first,
    ));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_the_spec_grammar_literals_inside_the_names_module() {
    let mut failures = prove_both_sides(
        spec_grammar_refs,
        &[
            "const PREFIX: &str = \"wi-\";",
            "let name = format!(\"wi-{id}__\");",
            "let file = \"parte-01.md\";",
            "let file = \"emenda-02.md\";",
        ],
        &[
            "use crate::workflow::names::FOLDER_PREFIX;",
            "// const PREFIX: &str = \"wi-\";",
            "let text = \"a wi- in the middle\";",
            "let text = \"parte\";",
        ],
    );
    let outside_names: Vec<PathBuf> = rust_files(&src_dir())
        .into_iter()
        .filter(|path| !path.ends_with(SPEC_GRAMMAR_MODULE))
        .collect();

    failures.extend(scan_paths(&outside_names, spec_grammar_refs));

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_the_removed_mcp_port_out_of_the_src_layers() {
    let mut failures = prove_both_sides(
        removed_port_refs,
        &[
            "// serves the MCP tools",
            "fn parse_tool_call() {}",
            "const PROTOCOL: &str = \"JSON-RPC\";",
        ],
        &["fn run() {}", "// serves the CLI actions"],
    );
    let layers: Vec<PathBuf> = rust_files(&src_dir())
        .into_iter()
        .filter(|file| {
            let relative = relative_to_src(file);
            !relative.starts_with("adapters/") && relative != "main.rs"
        })
        .collect();

    failures.extend(scan_paths(&layers, removed_port_refs));
    failures.extend(prove_synthetic_dir_is_flagged());

    assert_no_failures(&failures);
}

#[test]
fn it_should_not_grow_the_numeric_cuts_in_src_app_without_an_inventory_entry() {
    let mut failures = prove_both_sides(
        numeric_cut_refs,
        &[
            "fn f(v: &[u8]) -> usize { v.len().min(3) }",
            "fn f(v: &[u8]) -> usize { v.len().saturating_sub(3) }",
            "fn f(v: Vec<u8>) { v.into_iter().take(3); }",
        ],
        &[
            "fn f() {}",
            "// v.len().min(3)",
            "const NOTE: &str = \".take(3)\";",
        ],
    );
    failures.extend(numeric_cut_mismatches(
        &src_dir().join("app"),
        NUMERIC_CUTS_IN_APP,
    ));
    failures.extend(prove_numeric_cut_list_is_enforced());
    failures.extend(prove_every_numeric_cut_needle_is_counted());

    assert_no_failures(&failures);
}

/// Um diretório sintético com um único fonte `cut.rs`.
fn synthetic_cut_dir(source: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temporary directory");
    fs::write(dir.path().join("cut.rs"), source).expect("write the synthetic source");

    dir
}

/// A lista exata passa; um corte além dela, um a menos e um em arquivo fora da lista reprovam.
fn prove_numeric_cut_list_is_enforced() -> Vec<String> {
    let two_cuts = synthetic_cut_dir("fn f() { 1u8.min(2); 3u8.max(4); }\n");
    let one_cut = synthetic_cut_dir("fn f() { 1u8.min(2); }\n");
    let mut failures = Vec::new();

    if !numeric_cut_mismatches(two_cuts.path(), &[("cut.rs", 2)]).is_empty() {
        failures.push("synthetic directory at its allowed count was flagged".to_owned());
    }
    if numeric_cut_mismatches(two_cuts.path(), &[("cut.rs", 1)]).is_empty() {
        failures.push("synthetic directory with an extra numeric cut was not flagged".to_owned());
    }
    if numeric_cut_mismatches(two_cuts.path(), &[]).is_empty() {
        failures.push("numeric cut in a file outside the closed list was not flagged".to_owned());
    }
    let below = numeric_cut_mismatches(one_cut.path(), &[("cut.rs", 2)]);
    if !below
        .iter()
        .any(|failure| failure.contains("lower the list"))
    {
        failures.push("synthetic directory below its allowed count was not flagged".to_owned());
    }

    failures
}

/// Cada agulha de `NUMERIC_CUT_NEEDLES` tem um trecho que a conta uma vez; agulha nova sem trecho reprova.
fn prove_every_numeric_cut_needle_is_counted() -> Vec<String> {
    let snippets: [(&str, &str); 5] = [
        (".min(", "fn f() { 5u8.min(2); }"),
        (".max(", "fn f() { 5u8.max(2); }"),
        (".clamp(", "fn f() { 5u8.clamp(1, 2); }"),
        (".take(", "fn f(v: Vec<u8>) { v.into_iter().take(3); }"),
        ("saturating_", "fn f() { 5u8.saturating_sub(2); }"),
    ];
    let mut failures = Vec::new();

    let needles: Vec<&str> = snippets.iter().map(|(needle, _)| *needle).collect();
    if needles != NUMERIC_CUT_NEEDLES {
        failures.push(format!(
            "the needle table {needles:?} differs from NUMERIC_CUT_NEEDLES {NUMERIC_CUT_NEEDLES:?}"
        ));
    }
    for (needle, snippet) in snippets {
        if numeric_cut_refs(snippet) != Ok(1) {
            failures.push(format!("the snippet for {needle:?} was not counted once"));
        }
    }

    failures
}

/// O varredor de diretório reprova um fonte que traz o termo.
fn prove_synthetic_dir_is_flagged() -> Vec<String> {
    let dir = tempfile::tempdir().expect("temporary directory");
    fs::write(dir.path().join("leak.rs"), "// MCP stdio\n").expect("write the synthetic source");

    if scan_dir(dir.path(), removed_port_refs).is_empty() {
        return vec!["synthetic directory with a port term was not flagged".to_owned()];
    }

    Vec::new()
}
