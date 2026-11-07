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
