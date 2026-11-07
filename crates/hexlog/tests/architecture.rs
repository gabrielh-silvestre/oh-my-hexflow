//! Travas de arquitetura do crate `hexlog`: vocabulário do motor, crates proibidos,
//! imports do `engine`, visibilidade dos módulos e API pública dos testes.
//!
//! Tudo que depende de arquivo ainda inexistente passa sem falhar; por isso cada teste
//! de regra também roda o verificador contra trechos embutidos e prova os dois lados.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree, token_stream};
use serde_json::{Value, json};

const VOCABULARY: &[&str] = &[
    "decision",
    "decisão",
    "premise",
    "premissa",
    "guideline",
    "directive",
    "diretriz",
    "check",
    "gate",
    "attestation",
    "unit",
    "work",
    "prd",
    "review",
    "omc",
    "omhl",
    "hexflow",
    "claude",
];
const FORBIDDEN_CRATES: &[&str] = &[
    "git2",
    "gix",
    "reqwest",
    "ureq",
    "hyper",
    "tokio",
    "async-std",
    "smol",
    "async-global-executor",
    "sqlx-macros",
];
const FORBIDDEN_SERDE_JSON_FEATURES: &[&str] = &["arbitrary_precision", "preserve_order"];
const GIT_ONLY_FILE: &str = "engine/snapshot.rs";
const SCHEMA_SQL: &str = "store/sqlite/schema.sql";
const BLOCKING_IDENTS: &[&str] = &["block_on", "futures_executor"];
const BLOCKING_HOME: &str = "store/sqlite";
const RESTRICTED_FOR_ENGINE: &[&str] = &["store", "catalog"];
const INTERNAL_MODULES: &[&str] = &["engine", "catalog", "store"];
const ENGINE_CHILDREN: &[(&str, &str)] = &[
    ("ports", "pub(crate)"),
    ("model", "pub(crate)"),
    ("snapshot", "pub(crate)"),
    ("limits", "pub(crate)"),
    ("chain", "pub(crate)"),
    ("decide", "pub(in crate::engine)"),
    ("rules", "pub(in crate::engine)"),
    ("verify", "pub(in crate::engine)"),
];
const LIB_MODULES: &[(&str, &str)] = &[("engine", ""), ("catalog", ""), ("store", "")];
const MEMORY_LINTS: &[&str] = &[
    "disallowed_types",
    "disallowed_methods",
    "mem_forget",
    "redundant_clone",
    "implicit_clone",
    "needless_pass_by_value",
    "needless_pass_by_ref_mut",
    "large_types_passed_by_value",
];
const LINT_LEVELS: &[&str] = &["allow", "warn", "forbid"];
const REGISTERED_REASON_PREFIX: &str = "memory-exception:";

struct Hit<'a> {
    line: usize,
    word: &'a str,
}

struct Finding {
    line: usize,
    what: String,
}

struct Case<'a> {
    language: &'a str,
    expectation: &'a str,
    name: &'a str,
    source: &'a str,
}

// ---------------------------------------------------------------- arquivos

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn src_dir() -> PathBuf {
    crate_dir().join("src")
}

fn display(path: &Path) -> String {
    path.strip_prefix(crate_dir())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files(dir, extension, &mut files);
    files.sort();
    files
}

fn collect_files(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, extension, files);
        } else if path.extension().is_some_and(|found| found == extension) {
            files.push(path);
        }
    }
}

fn render(path: &Path, line: usize, what: &str) -> String {
    format!("{}:{line}: `{what}`", display(path))
}

// ---------------------------------------------------------------- palavras

fn identifier_words(name: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = name.char_indices().collect();
    let mut words = Vec::new();
    let mut start = 0;
    for (position, &(index, current)) in chars.iter().enumerate() {
        if !current.is_alphanumeric() {
            if start < index {
                words.push(&name[start..index]);
            }
            start = index + current.len_utf8();
            continue;
        }
        if position == 0 || start == index {
            continue;
        }
        let previous = chars[position - 1].1;
        let next = chars.get(position + 1).map(|&(_, c)| c);
        let digit_edge = previous.is_numeric() != current.is_numeric();
        let camel_edge = current.is_uppercase() && !previous.is_uppercase();
        let acronym_edge = previous.is_uppercase()
            && current.is_uppercase()
            && next.is_some_and(char::is_lowercase);
        if digit_edge || camel_edge || acronym_edge {
            words.push(&name[start..index]);
            start = index;
        }
    }
    if start < name.len() {
        words.push(&name[start..]);
    }
    words
}

fn collect<'a, 'w>(
    words: impl IntoIterator<Item = &'w str>,
    line: usize,
    banned: &[&'a str],
    hits: &mut Vec<Hit<'a>>,
) {
    for word in words {
        let lower = word.to_lowercase();
        if let Some(found) = banned.iter().copied().find(|candidate| *candidate == lower) {
            hits.push(Hit { line, word: found });
        }
    }
}

// ------------------------------------------------------- literais de string

fn string_content(source: &str) -> Option<String> {
    let rest = source
        .strip_prefix('b')
        .or_else(|| source.strip_prefix('c'))
        .unwrap_or(source);
    let (raw, rest) = match rest.strip_prefix('r') {
        Some(after) => (true, after),
        None => (false, rest),
    };
    let hashes = rest.len() - rest.trim_start_matches('#').len();
    let body = rest[hashes..].strip_prefix('"')?;
    let closing = format!("\"{}", "#".repeat(hashes));
    let body = body.strip_suffix(closing.as_str())?;
    Some(if raw {
        body.to_string()
    } else {
        unescape(body)
    })
}

fn unescape(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                out.push(decode_hex(&hex));
            }
            Some('u') => {
                chars.next();
                let hex: String = chars
                    .by_ref()
                    .take_while(|digit| *digit != '}')
                    .filter(|digit| *digit != '_')
                    .collect();
                out.push(decode_hex(&hex));
            }
            Some('\n' | '\r') => while chars.next_if(|next| next.is_whitespace()).is_some() {},
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn decode_hex(hex: &str) -> char {
    u32::from_str_radix(hex, 16)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or('\u{fffd}')
}

// ---------------------------------------------------------- varredor de Rust

fn tokens(stream: TokenStream) -> Vec<TokenTree> {
    stream.into_iter().collect()
}

fn tokenize(source: &str) -> Result<TokenStream, String> {
    TokenStream::from_str(source).map_err(|error| error.to_string())
}

fn is_ident(tree: &TokenTree, name: &str) -> bool {
    matches!(tree, TokenTree::Ident(ident) if ident == name)
}

fn is_punct(tree: Option<&TokenTree>, expected: char) -> bool {
    matches!(tree, Some(TokenTree::Punct(punct)) if punct.as_char() == expected)
}

fn scan_rust<'a>(source: &str, banned: &[&'a str]) -> Result<Vec<Hit<'a>>, String> {
    let mut hits = Vec::new();
    scan_stream(tokenize(source)?, banned, &mut hits);
    Ok(hits)
}

fn scan_stream<'a>(stream: TokenStream, banned: &[&'a str], hits: &mut Vec<Hit<'a>>) {
    let mut trees = stream.into_iter().peekable();
    while let Some(tree) = trees.next() {
        match tree {
            TokenTree::Punct(punct) if punct.as_char() == '#' => skip_doc_attribute(&mut trees),
            TokenTree::Punct(_) => {}
            TokenTree::Group(group) => scan_stream(group.stream(), banned, hits),
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                let name = name.strip_prefix("r#").unwrap_or(&name);
                collect(
                    identifier_words(name),
                    ident.span().start().line,
                    banned,
                    hits,
                );
            }
            TokenTree::Literal(literal) => {
                if let Some(text) = string_content(&literal.to_string()) {
                    let words = text
                        .split(|c: char| !c.is_alphanumeric())
                        .filter(|w| !w.is_empty());
                    collect(words, literal.span().start().line, banned, hits);
                }
            }
        }
    }
}

/// Linhas em que `stream` usa, como identificador inteiro, um dos nomes de `names`.
fn lines_using_idents(stream: TokenStream, names: &[&str]) -> Vec<usize> {
    let mut lines = Vec::new();
    for tree in stream {
        match tree {
            TokenTree::Group(group) => lines.extend(lines_using_idents(group.stream(), names)),
            TokenTree::Ident(ident) if names.iter().any(|name| ident == name) => {
                lines.push(ident.span().start().line);
            }
            _ => {}
        }
    }
    lines
}

fn skip_doc_attribute(trees: &mut Peekable<token_stream::IntoIter>) {
    if is_punct(trees.peek(), '!') {
        trees.next();
    }
    if matches!(trees.peek(), Some(TokenTree::Group(group)) if starts_with_doc(group)) {
        trees.next();
    }
}

fn starts_with_doc(group: &Group) -> bool {
    group.delimiter() == Delimiter::Bracket
        && group
            .stream()
            .into_iter()
            .next()
            .is_some_and(|first| is_ident(&first, "doc"))
}

// ----------------------------------------------------------- varredor de SQL

fn strip_sql_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_literal = false;
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_literal = !in_literal;
                out.push(c);
            }
            '-' if !in_literal && chars.peek() == Some(&'-') => {
                while chars.next_if(|next| *next != '\n').is_some() {}
            }
            '/' if !in_literal && chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                    }
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Cada sequência de letras, dígitos e `_`, com a linha e o texto que vem depois dela.
fn sql_runs(text: &str) -> Vec<(usize, &str, &str)> {
    let mut runs = Vec::new();
    let mut line = 1;
    let mut start: Option<usize> = None;
    for (index, c) in text.char_indices() {
        let in_run = c.is_alphanumeric() || c == '_';
        match (in_run, start) {
            (true, None) => start = Some(index),
            (false, Some(from)) => {
                runs.push((line, &text[from..index], &text[index..]));
                start = None;
            }
            _ => {}
        }
        if c == '\n' {
            line += 1;
        }
    }
    if let Some(from) = start {
        runs.push((line, &text[from..], ""));
    }
    runs
}

fn scan_sql<'a>(source: &str, banned: &[&'a str]) -> Vec<Hit<'a>> {
    let cleaned = strip_sql_comments(source);
    let mut hits = Vec::new();
    for (line, run, tail) in sql_runs(&cleaned) {
        let is_keyword = run.eq_ignore_ascii_case("constraint")
            || (run.eq_ignore_ascii_case("check") && tail.trim_start().starts_with('('));
        if !is_keyword {
            collect(identifier_words(run), line, banned, &mut hits);
        }
    }
    hits
}

fn rust_findings(files: &[PathBuf], banned: &[&str]) -> Vec<String> {
    let mut findings = Vec::new();
    for path in files {
        match scan_rust(&read(path), banned) {
            Ok(hits) => findings.extend(hits.iter().map(|hit| render(path, hit.line, hit.word))),
            Err(error) => findings.push(format!("{}: cannot tokenize: {error}", display(path))),
        }
    }
    findings
}

fn schema_findings(banned: &[&str]) -> Vec<String> {
    let path = src_dir().join(SCHEMA_SQL);
    if !path.exists() {
        return Vec::new();
    }
    scan_sql(&read(&path), banned)
        .iter()
        .map(|hit| render(&path, hit.line, hit.word))
        .collect()
}

// -------------------------------------------------------------- cargo metadata

/// Fecho de dependências normais de `hexlog` já podado pelas features (uma linha por crate).
fn cargo_tree_closure() -> String {
    let cargo = std::env::var_os("CARGO").expect("cargo sets CARGO for test processes");
    let root = crate_dir()
        .ancestors()
        .nth(2)
        .expect("workspace root above the crate");
    let output = Command::new(cargo)
        .args(["tree", "-p", "hexlog", "-e", "normal", "--prefix", "none"])
        .args(["--target", "all", "--locked", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("cargo tree should start");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("cargo tree should print UTF-8")
}

/// Nome de cada linha `nome vVERSÃO [sufixos]`, sem versão nem `(*)`.
fn tree_crate_names(tree: &str) -> BTreeSet<&str> {
    tree.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            fields.next()?.starts_with('v').then_some(name)
        })
        .collect()
}

fn forbidden_crates_in(tree: &str) -> Vec<&'static str> {
    let names = tree_crate_names(tree);
    FORBIDDEN_CRATES
        .iter()
        .copied()
        .filter(|name| names.contains(name))
        .collect()
}

fn cargo_metadata() -> Value {
    let cargo = std::env::var_os("CARGO").expect("cargo sets CARGO for test processes");
    let root = crate_dir()
        .ancestors()
        .nth(2)
        .expect("workspace root above the crate");
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("cargo metadata should start");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata should print JSON")
}

fn items(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

/// Features resolvidas de todos os nós do `serde_json` no `cargo metadata`.
fn serde_json_features(metadata: &Value) -> BTreeSet<&str> {
    let json_ids: BTreeSet<&str> = items(&metadata["packages"])
        .iter()
        .filter(|package| package["name"] == "serde_json")
        .filter_map(|package| package["id"].as_str())
        .collect();
    items(&metadata["resolve"]["nodes"])
        .iter()
        .filter(|node| node["id"].as_str().is_some_and(|id| json_ids.contains(id)))
        .flat_map(|node| items(&node["features"]))
        .filter_map(Value::as_str)
        .collect()
}

fn forbidden_serde_json_features(metadata: &Value) -> Vec<&'static str> {
    let features = serde_json_features(metadata);
    FORBIDDEN_SERDE_JSON_FEATURES
        .iter()
        .copied()
        .filter(|name| features.contains(name))
        .collect()
}

/// Nomes dos pacotes alcançáveis a partir de `root` só por dependências normais.
fn runtime_closure<'m>(metadata: &'m Value, root: &str) -> BTreeSet<&'m str> {
    let names: HashMap<&str, &str> = items(&metadata["packages"])
        .iter()
        .filter_map(|package| Some((package["id"].as_str()?, package["name"].as_str()?)))
        .collect();
    let nodes: HashMap<&str, &Value> = items(&metadata["resolve"]["nodes"])
        .iter()
        .filter_map(|node| Some((node["id"].as_str()?, node)))
        .collect();
    let mut pending: Vec<&str> = names
        .iter()
        .filter(|(_, name)| **name == root)
        .map(|(id, _)| *id)
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(id) else {
            continue;
        };
        for dep in items(&node["deps"]) {
            let is_normal = items(&dep["dep_kinds"])
                .iter()
                .any(|kind| kind["kind"].is_null());
            if let (true, Some(package)) = (is_normal, dep["pkg"].as_str()) {
                pending.push(package);
            }
        }
    }
    seen.into_iter()
        .filter_map(|id| names.get(id).copied())
        .collect()
}

// ------------------------------------------------------ imports do `engine`

fn restricted(name: &str) -> Option<&'static str> {
    RESTRICTED_FOR_ENGINE
        .iter()
        .copied()
        .find(|candidate| *candidate == name)
}

fn is_colon_pair(trees: &[TokenTree], at: usize) -> bool {
    matches!(
        trees.get(at..at + 2),
        Some([TokenTree::Punct(first), TokenTree::Punct(second)])
            if first.as_char() == ':' && second.as_char() == ':'
    )
}

fn is_cfg_test(group: &Group) -> bool {
    if group.delimiter() != Delimiter::Bracket {
        return false;
    }
    let inner = tokens(group.stream());
    match inner.as_slice() {
        [TokenTree::Ident(cfg), TokenTree::Group(args)] if cfg == "cfg" => {
            args.delimiter() == Delimiter::Parenthesis
                && matches!(tokens(args.stream()).as_slice(), [only] if is_ident(only, "test"))
        }
        _ => false,
    }
}

/// Reconhece `#[cfg(test)] [outros atributos] [pub(...)] mod nome { ... }` a partir do `#`.
fn cfg_test_module(trees: &[TokenTree], at: usize) -> Option<(Vec<TokenTree>, usize)> {
    let TokenTree::Group(attribute) = trees.get(at + 1)? else {
        return None;
    };
    if !is_cfg_test(attribute) {
        return None;
    }
    let mut cursor = at + 2;
    loop {
        match trees.get(cursor)? {
            TokenTree::Punct(punct) if punct.as_char() == '#' => cursor += 2,
            tree if is_ident(tree, "pub") => {
                cursor += 1;
                if matches!(trees.get(cursor), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis)
                {
                    cursor += 1;
                }
            }
            tree if is_ident(tree, "mod") => break,
            _ => return None,
        }
    }
    let TokenTree::Group(body) = trees.get(cursor + 2)? else {
        return None;
    };
    (body.delimiter() == Delimiter::Brace).then(|| (tokens(body.stream()), cursor + 3))
}

fn import_findings(source: &str) -> Result<Vec<Finding>, String> {
    let mut found = Vec::new();
    walk_imports(&tokens(tokenize(source)?), false, &mut found);
    Ok(found)
}

fn walk_imports(trees: &[TokenTree], in_test_module: bool, found: &mut Vec<Finding>) {
    let mut at = 0;
    while at < trees.len() {
        match &trees[at] {
            tree if is_ident(tree, "use") => {
                let end = trees[at..]
                    .iter()
                    .position(|tree| is_punct(Some(tree), ';'))
                    .map_or(trees.len(), |offset| at + offset);
                check_use(&trees[at + 1..end], in_test_module, found);
                at = end + 1;
            }
            TokenTree::Punct(punct) if punct.as_char() == '#' => {
                if let Some((body, next)) = cfg_test_module(trees, at) {
                    walk_imports(&body, true, found);
                    at = next;
                } else {
                    at += 1;
                }
            }
            TokenTree::Ident(ident) => {
                if let Some(name) = restricted(&ident.to_string())
                    && (is_colon_pair(trees, at + 1) || (at >= 2 && is_colon_pair(trees, at - 2)))
                {
                    found.push(Finding {
                        line: ident.span().start().line,
                        what: name.to_string(),
                    });
                }
                at += 1;
            }
            TokenTree::Group(group) => {
                walk_imports(&tokens(group.stream()), in_test_module, found);
                at += 1;
            }
            _ => at += 1,
        }
    }
}

fn check_use(trees: &[TokenTree], in_test_module: bool, found: &mut Vec<Finding>) {
    for tree in trees {
        match tree {
            TokenTree::Group(group) => check_use(&tokens(group.stream()), in_test_module, found),
            TokenTree::Ident(ident) => {
                if let Some(name) = restricted(&ident.to_string()) {
                    found.push(Finding {
                        line: ident.span().start().line,
                        what: name.to_string(),
                    });
                }
            }
            TokenTree::Punct(punct) if punct.as_char() == '*' && !in_test_module => {
                found.push(Finding {
                    line: punct.span().start().line,
                    what: "*".to_string(),
                });
            }
            _ => {}
        }
    }
}

// ------------------------------------------------- visibilidade dos módulos

struct ModuleDecl {
    name: String,
    visibility: String,
    line: usize,
}

fn compact(text: &str) -> String {
    text.split_whitespace().collect()
}

fn visibility_before(trees: &[TokenTree], at: usize) -> String {
    match at.checked_sub(1).and_then(|previous| trees.get(previous)) {
        Some(tree) if is_ident(tree, "pub") => "pub".to_string(),
        Some(TokenTree::Group(group))
            if group.delimiter() == Delimiter::Parenthesis
                && at >= 2
                && is_ident(&trees[at - 2], "pub") =>
        {
            format!("pub({})", compact(&group.stream().to_string()))
        }
        _ => String::new(),
    }
}

fn module_declarations(source: &str) -> Result<Vec<ModuleDecl>, String> {
    let trees = tokens(tokenize(source)?);
    let mut declarations = Vec::new();
    for (at, tree) in trees.iter().enumerate() {
        let (true, Some(TokenTree::Ident(name))) = (is_ident(tree, "mod"), trees.get(at + 1))
        else {
            continue;
        };
        let terminated = match trees.get(at + 2) {
            Some(TokenTree::Punct(punct)) => punct.as_char() == ';',
            Some(TokenTree::Group(group)) => group.delimiter() == Delimiter::Brace,
            _ => false,
        };
        if terminated {
            declarations.push(ModuleDecl {
                name: name.to_string(),
                visibility: visibility_before(&trees, at),
                line: name.span().start().line,
            });
        }
    }
    Ok(declarations)
}

fn visibility_findings(
    source: &str,
    expected: &[(&str, &str)],
    reject_unknown: bool,
) -> Result<Vec<Finding>, String> {
    let mut found = Vec::new();
    for declaration in module_declarations(source)? {
        let known = expected.iter().find(|(name, _)| *name == declaration.name);
        let what = match known {
            Some((_, visibility)) if compact(visibility) != declaration.visibility => format!(
                "{} is declared as `{}`, expected `{visibility}`",
                declaration.name, declaration.visibility
            ),
            None if reject_unknown => format!("unexpected child module {}", declaration.name),
            _ => continue,
        };
        found.push(Finding {
            line: declaration.line,
            what,
        });
    }
    Ok(found)
}

// -------------------------------------------- API pública dos testes de integração

fn is_path_attribute(group: &Group) -> bool {
    group.delimiter() == Delimiter::Bracket
        && matches!(
            tokens(group.stream()).as_slice(),
            [first, second, ..] if is_ident(first, "path") && is_punct(Some(second), '=')
        )
}

fn mentions_src_path(stream: TokenStream) -> bool {
    stream.into_iter().any(|tree| match tree {
        TokenTree::Literal(literal) => string_content(&literal.to_string())
            .is_some_and(|text| text.split(['/', '\\']).any(|part| part == "src")),
        TokenTree::Group(group) => mentions_src_path(group.stream()),
        _ => false,
    })
}

fn internal_path_findings(next: Option<&TokenTree>, found: &mut Vec<Finding>) {
    match next {
        Some(TokenTree::Ident(name)) if INTERNAL_MODULES.contains(&name.to_string().as_str()) => {
            found.push(Finding {
                line: name.span().start().line,
                what: format!("hexlog::{name}"),
            });
        }
        Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace => {
            let mut at_start = true;
            for tree in group.stream() {
                if let (true, TokenTree::Ident(name)) = (at_start, &tree)
                    && INTERNAL_MODULES.contains(&name.to_string().as_str())
                {
                    found.push(Finding {
                        line: name.span().start().line,
                        what: format!("hexlog::{name}"),
                    });
                }
                at_start = is_punct(Some(&tree), ',');
            }
        }
        _ => {}
    }
}

fn api_findings(source: &str) -> Result<Vec<Finding>, String> {
    let mut found = Vec::new();
    walk_api(&tokens(tokenize(source)?), &mut found);
    Ok(found)
}

fn walk_api(trees: &[TokenTree], found: &mut Vec<Finding>) {
    for (at, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Group(group) => {
                if is_path_attribute(group) {
                    found.push(Finding {
                        line: group.span().start().line,
                        what: "#[path]".to_string(),
                    });
                }
                walk_api(&tokens(group.stream()), found);
            }
            TokenTree::Ident(ident)
                if matches!(
                    ident.to_string().as_str(),
                    "include" | "include_str" | "include_bytes"
                ) && is_punct(trees.get(at + 1), '!') =>
            {
                if let Some(TokenTree::Group(args)) = trees.get(at + 2)
                    && mentions_src_path(args.stream())
                {
                    found.push(Finding {
                        line: ident.span().start().line,
                        what: format!("{ident}! of a file under src"),
                    });
                }
            }
            TokenTree::Ident(ident) if ident == "hexlog" && is_colon_pair(trees, at + 1) => {
                internal_path_findings(trees.get(at + 3), found);
            }
            _ => {}
        }
    }
}

// ----------------------------------------------------- prova dos dois lados

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
            Err(error) => failures.push(format!(
                "embedded snippet did not tokenize ({error}): {snippet}"
            )),
        }
    }
    for snippet in accepted {
        match count(snippet) {
            Ok(0) => {}
            Ok(_) => failures.push(format!("embedded snippet was flagged: {snippet}")),
            Err(error) => failures.push(format!(
                "embedded snippet did not tokenize ({error}): {snippet}"
            )),
        }
    }
    failures
}

fn assert_no_failures(failures: &[String]) {
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

// ------------------------------------------------- guarda da regra de memória

fn workspace_crates() -> Vec<PathBuf> {
    let crates = crate_dir()
        .parent()
        .expect("crates directory above the crate");
    let mut dirs: Vec<PathBuf> = fs::read_dir(crates)
        .expect("crates directory should be readable")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("Cargo.toml").is_file())
        .collect();
    dirs.sort();
    dirs
}

fn inherits_workspace_lints(manifest: &str) -> bool {
    let mut in_lints = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_lints = line == "[lints]";
        } else if in_lints && compact(line) == "workspace=true" {
            return true;
        }
    }
    false
}

fn cited_memory_lints(args: &[TokenTree]) -> Vec<&'static str> {
    let mut cited = Vec::new();
    for (at, tree) in args.iter().enumerate() {
        if is_ident(tree, "clippy")
            && is_colon_pair(args, at + 1)
            && let Some(TokenTree::Ident(name)) = args.get(at + 3)
            && let Some(lint) = MEMORY_LINTS.iter().copied().find(|lint| name == *lint)
        {
            cited.push(lint);
        }
    }
    cited
}

fn has_registered_reason(args: &[TokenTree]) -> bool {
    args.windows(3).any(|window| {
        is_ident(&window[0], "reason")
            && is_punct(Some(&window[1]), '=')
            && matches!(&window[2], TokenTree::Literal(reason)
                if string_content(&reason.to_string()).is_some_and(|text| text.starts_with(REGISTERED_REASON_PREFIX)))
    })
}

fn attribute_findings(inner: &[TokenTree], found: &mut Vec<Finding>) {
    for (at, tree) in inner.iter().enumerate() {
        match tree {
            TokenTree::Ident(level) => {
                let Some(TokenTree::Group(args)) = inner.get(at + 1) else {
                    continue;
                };
                let level_name = level.to_string();
                let is_expect = level_name == "expect";
                if args.delimiter() != Delimiter::Parenthesis
                    || !(is_expect || LINT_LEVELS.contains(&level_name.as_str()))
                {
                    continue;
                }
                let args = tokens(args.stream());
                for lint in cited_memory_lints(&args) {
                    if !(is_expect && has_registered_reason(&args)) {
                        found.push(Finding {
                            line: level.span().start().line,
                            what: format!("{level_name}(clippy::{lint}) without a `{REGISTERED_REASON_PREFIX}` expect"),
                        });
                    }
                }
            }
            TokenTree::Group(group) => attribute_findings(&tokens(group.stream()), found),
            _ => {}
        }
    }
}

fn walk_attributes(trees: &[TokenTree], found: &mut Vec<Finding>) {
    for (at, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Punct(punct) if punct.as_char() == '#' => {
                let bracket = if is_punct(trees.get(at + 1), '!') {
                    at + 2
                } else {
                    at + 1
                };
                if let Some(TokenTree::Group(group)) = trees.get(bracket)
                    && group.delimiter() == Delimiter::Bracket
                {
                    attribute_findings(&tokens(group.stream()), found);
                }
            }
            TokenTree::Group(group) => walk_attributes(&tokens(group.stream()), found),
            _ => {}
        }
    }
}

fn memory_lint_findings(source: &str) -> Result<Vec<Finding>, String> {
    let mut found = Vec::new();
    walk_attributes(&tokens(tokenize(source)?), &mut found);
    Ok(found)
}

fn engine_files() -> Vec<PathBuf> {
    files_with_extension(&src_dir().join("engine"), "rs")
}

fn parse_cases(content: &str) -> Vec<Case<'_>> {
    content
        .split("\n@@ ")
        .skip(1)
        .map(|chunk| {
            let (header, source) = chunk.split_once('\n').unwrap_or((chunk, ""));
            let mut fields = header.splitn(3, '|').map(str::trim);
            let mut field = |label: &str| {
                fields
                    .next()
                    .unwrap_or_else(|| panic!("case header lacks {label}: {header}"))
            };
            Case {
                language: field("language"),
                expectation: field("expectation"),
                name: field("name"),
                source,
            }
        })
        .collect()
}

fn expected_words(expectation: &str) -> BTreeSet<&str> {
    match expectation.strip_prefix("flags ") {
        Some(words) => words.split(',').map(str::trim).collect(),
        None => {
            assert_eq!(expectation, "clean", "unknown expectation");
            BTreeSet::new()
        }
    }
}

// ---------------------------------------------------------------- testes

#[test]
fn it_should_not_use_domain_vocabulary_in_identifiers_or_string_literals_of_the_hexlog_sources() {
    let sources = files_with_extension(&src_dir(), "rs");

    let mut findings = rust_findings(&sources, VOCABULARY);
    findings.extend(schema_findings(VOCABULARY));

    assert_no_failures(&findings);
}

#[test]
fn it_should_confine_block_on_to_the_sqlite_store() {
    let home = src_dir().join(BLOCKING_HOME);
    let sources: Vec<PathBuf> = files_with_extension(&src_dir(), "rs")
        .into_iter()
        .filter(|path| !path.starts_with(&home))
        .collect();

    let mut findings = Vec::new();
    for path in &sources {
        match tokenize(&read(path)) {
            Ok(stream) => findings.extend(
                lines_using_idents(stream, BLOCKING_IDENTS)
                    .into_iter()
                    .map(|line| render(path, line, "block_on or futures_executor")),
            ),
            Err(error) => findings.push(format!("{}: cannot tokenize: {error}", display(path))),
        }
    }

    assert_no_failures(&findings);
}

#[test]
fn it_should_mention_git_only_in_engine_snapshot() {
    let sources: Vec<PathBuf> = files_with_extension(&src_dir(), "rs")
        .into_iter()
        .filter(|path| !path.ends_with(GIT_ONLY_FILE))
        .collect();

    let mut findings = rust_findings(&sources, &["git"]);
    findings.extend(schema_findings(&["git"]));

    assert_no_failures(&findings);
}

#[test]
fn it_should_flag_forbidden_words_in_identifiers_and_literals_and_ignore_them_in_comments_and_sql_keywords()
 {
    let fixture = read(&crate_dir().join("tests/fixtures/scanner_cases.txt"));
    let cases = parse_cases(&fixture);

    let mut failures = Vec::new();
    for case in &cases {
        let found: BTreeSet<&str> = match case.language {
            "rust" => match scan_rust(case.source, VOCABULARY) {
                Ok(hits) => hits.iter().map(|hit| hit.word).collect(),
                Err(error) => {
                    failures.push(format!("{}: did not tokenize: {error}", case.name));
                    continue;
                }
            },
            "sql" => scan_sql(case.source, VOCABULARY)
                .iter()
                .map(|hit| hit.word)
                .collect(),
            other => panic!("unknown language `{other}` in case `{}`", case.name),
        };
        let expected = expected_words(case.expectation);
        if found != expected {
            failures.push(format!(
                "{}: expected {expected:?}, found {found:?}",
                case.name
            ));
        }
    }

    assert!(
        !cases.is_empty(),
        "the fixture should declare at least one case"
    );
    assert_no_failures(&failures);
}

#[test]
fn it_should_not_depend_on_forbidden_crates() {
    let metadata = cargo_metadata();

    let flagged_document = json!({
        "packages": [{"id": "a", "name": "serde_json"}],
        "resolve": {"nodes": [{"id": "a", "features": ["std", "arbitrary_precision", "preserve_order"]}]}
    });
    let accepted_document = json!({
        "packages": [{"id": "a", "name": "serde_json"}, {"id": "b", "name": "other"}],
        "resolve": {"nodes": [
            {"id": "a", "features": ["std", "float_roundtrip"]},
            {"id": "b", "features": ["arbitrary_precision"]}
        ]}
    });

    let flagged_tree =
        "hexlog v0.1.0 (/work/crates/hexlog)\nserde v1.0.0\nreqwest v0.12.0\ntokio v1.0.0 (*)\n";
    let accepted_tree = "hexlog v0.1.0 (/work/crates/hexlog)\nsqlx v0.9.0\nsqlx-core v0.9.0 (*)\nsmol_str v0.3.0\nhyper-util v0.1.0 (proc-macro)\n";

    let closure = cargo_tree_closure();
    let features = serde_json_features(&metadata);

    assert_eq!(
        forbidden_crates_in(flagged_tree),
        ["reqwest", "tokio"],
        "the closure check missed forbidden crates of the embedded tree"
    );
    assert!(
        forbidden_crates_in(accepted_tree).is_empty(),
        "the closure check flagged an accepted embedded tree"
    );
    assert_eq!(
        forbidden_serde_json_features(&flagged_document),
        ["arbitrary_precision", "preserve_order"],
        "the feature check missed forbidden features of the embedded document"
    );
    assert!(
        forbidden_serde_json_features(&accepted_document).is_empty(),
        "the feature check flagged an accepted embedded document"
    );
    assert!(
        tree_crate_names(&closure).contains("sqlx"),
        "the closure calculation lost a known dependency: {closure}"
    );
    assert!(
        !features.is_empty(),
        "the feature calculation found no serde_json features"
    );
    let present = forbidden_crates_in(&closure);
    assert!(
        present.is_empty(),
        "hexlog depends on forbidden crates: {present:?}"
    );
    let forbidden = forbidden_serde_json_features(&metadata);
    assert!(
        forbidden.is_empty(),
        "serde_json is resolved with forbidden features: {forbidden:?}"
    );
}

#[test]
fn it_should_not_depend_on_omhl() {
    let metadata = cargo_metadata();

    let hexlog_closure = runtime_closure(&metadata, "hexlog");
    let omhl_closure = runtime_closure(&metadata, "omhl");

    assert!(
        omhl_closure.contains("hexlog"),
        "the closure calculation lost a known dependency: {omhl_closure:?}"
    );
    assert!(
        !hexlog_closure.contains("omhl"),
        "hexlog depends on omhl: {hexlog_closure:?}"
    );
}

#[test]
fn it_should_not_import_store_or_catalog_from_the_engine_sources() {
    let flagged = [
        "use crate::store::X;",
        "use crate::{store::X};",
        "use crate::{engine::a, catalog};",
        "use crate::*;",
        "use super::*;",
        "use super::super::*;",
    ];
    let accepted = [
        "use crate::engine::ports::Store;",
        "use super::model::AppendItem;",
        "let store = 1;",
        "#[cfg(test)] mod tests { use super::*; }",
    ];

    let mut failures = prove_both_sides(
        |snippet| import_findings(snippet).map(|found| found.len()),
        &flagged,
        &accepted,
    );
    for path in engine_files() {
        match import_findings(&read(&path)) {
            Ok(found) => failures.extend(
                found
                    .iter()
                    .map(|finding| render(&path, finding.line, &finding.what)),
            ),
            Err(error) => failures.push(format!("{}: cannot tokenize: {error}", display(&path))),
        }
    }

    assert_no_failures(&failures);
}

#[test]
fn it_should_declare_every_engine_child_module_with_restricted_visibility() {
    let engine_flagged = [
        "pub mod ports;",
        "mod model;",
        "pub(crate) mod decide;",
        "pub(in crate::engine) mod chain;",
        "pub(crate) mod extra;",
    ];
    let engine_accepted = [
        "pub(crate) mod ports; pub(crate) mod model; pub(crate) mod snapshot; pub(crate) mod limits; pub(crate) mod chain; pub(in crate::engine) mod decide; pub(in crate::engine) mod rules; pub(in crate::engine) mod verify;",
    ];
    let lib_flagged = [
        "pub mod engine;",
        "pub(crate) mod catalog;",
        "pub mod ids; pub mod store;",
    ];
    let lib_accepted = ["mod engine; mod catalog; mod store; pub mod ids;"];

    let mut failures = prove_both_sides(
        |snippet| visibility_findings(snippet, ENGINE_CHILDREN, true).map(|found| found.len()),
        &engine_flagged,
        &engine_accepted,
    );
    failures.extend(prove_both_sides(
        |snippet| visibility_findings(snippet, LIB_MODULES, false).map(|found| found.len()),
        &lib_flagged,
        &lib_accepted,
    ));
    let real_files = [
        (src_dir().join("engine/mod.rs"), ENGINE_CHILDREN, true),
        (src_dir().join("lib.rs"), LIB_MODULES, false),
    ];
    for (path, expected, reject_unknown) in &real_files {
        if !path.exists() {
            continue;
        }
        match visibility_findings(&read(path), expected, *reject_unknown) {
            Ok(found) => failures.extend(
                found
                    .iter()
                    .map(|finding| render(path, finding.line, &finding.what)),
            ),
            Err(error) => failures.push(format!("{}: cannot tokenize: {error}", display(path))),
        }
    }

    assert_no_failures(&failures);
}

#[test]
fn it_should_keep_integration_tests_on_the_public_api() {
    let flagged = [
        "#[path = \"../src/engine/model.rs\"] mod model;",
        "const A: &str = include_str!(\"../src/store/sqlite/schema.sql\");",
        "include!(\"../src/lib.rs\");",
        "use hexlog::engine::ports::Store;",
        "use hexlog::{catalog, Facade};",
        "let x = hexlog::store::SqliteStore::open();",
    ];
    let accepted = [
        "use hexlog::Facade;",
        "use hexlog::chain::verify;",
        "const A: &str = include_str!(\"fixtures/golden_records.json\");",
        "let options = sqlx::sqlite::SqliteConnectOptions::new();",
    ];

    let mut failures = prove_both_sides(
        |snippet| api_findings(snippet).map(|found| found.len()),
        &flagged,
        &accepted,
    );
    for path in files_with_extension(&crate_dir().join("tests"), "rs") {
        match api_findings(&read(&path)) {
            Ok(found) => failures.extend(
                found
                    .iter()
                    .map(|finding| render(&path, finding.line, &finding.what)),
            ),
            Err(error) => failures.push(format!("{}: cannot tokenize: {error}", display(&path))),
        }
    }

    assert_no_failures(&failures);
}

#[test]
fn it_should_inherit_the_workspace_lints_in_every_crate() {
    let flagged = [
        "[package]\nname = \"a\"\n",
        "[package]\nname = \"a\"\n\n[lints]\nworkspace = false\n",
        "[package]\nname = \"a\"\n\n[lints.rust]\nunsafe_code = \"forbid\"\n",
        "[lints.rust]\nunsafe_code = \"forbid\"\n\n[dependencies]\nworkspace = true\n",
    ];
    let accepted = [
        "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n",
        "[lints]\nworkspace=true # inherit\n\n[dependencies]\n",
    ];

    let mut failures = prove_both_sides(
        |manifest| Ok(usize::from(!inherits_workspace_lints(manifest))),
        &flagged,
        &accepted,
    );
    let crates = workspace_crates();
    if crates.is_empty() {
        failures.push("no crate manifest found under crates/".to_string());
    }
    for dir in &crates {
        let manifest = dir.join("Cargo.toml");
        if !inherits_workspace_lints(&read(&manifest)) {
            failures.push(format!(
                "{}: missing `[lints]` with `workspace = true`",
                manifest.display()
            ));
        }
    }

    assert_no_failures(&failures);
}

#[test]
fn it_should_refuse_allow_for_the_memory_lints_and_require_a_registered_reason_on_expect() {
    let flagged = [
        "#![allow(clippy::disallowed_types, reason = \"x\")]",
        "#[allow(clippy::disallowed_types)] fn f() {}",
        "#[warn(clippy::mem_forget)] fn f() {}",
        "#[forbid(clippy::redundant_clone)] fn f() {}",
        "#[expect(clippy::disallowed_types, reason = \"because\")] fn f() {}",
        "#[expect(clippy::disallowed_methods)] fn f() {}",
        "#[cfg_attr(test, allow(clippy::needless_pass_by_value))] fn f() {}",
        "#[allow(clippy::too_many_arguments, clippy::disallowed_types)] fn f() {}",
        "#[allow(clippy::needless_pass_by_ref_mut, clippy::large_types_passed_by_value, clippy::implicit_clone)] fn f() {}",
        "fn f() { #[allow(clippy::disallowed_types)] let x = 1; }",
    ];
    let accepted = [
        "#[expect(clippy::disallowed_types, reason = \"memory-exception: two owners are required; alternatives discarded: borrowing\")] fn f() {}",
        "#![expect(clippy::mem_forget, reason = \"memory-exception: x\")]",
        "#[allow(clippy::too_many_arguments)] fn f() {}",
        "#[allow(dead_code)] fn f() {}",
        "#[derive(Debug)] struct S;",
    ];

    let mut failures = prove_both_sides(
        |snippet| memory_lint_findings(snippet).map(|found| found.len()),
        &flagged,
        &accepted,
    );
    for dir in workspace_crates() {
        for folder in ["src", "tests"] {
            for path in files_with_extension(&dir.join(folder), "rs") {
                match memory_lint_findings(&read(&path)) {
                    Ok(found) => failures.extend(
                        found
                            .iter()
                            .map(|finding| render(&path, finding.line, &finding.what)),
                    ),
                    Err(error) => {
                        failures.push(format!("{}: cannot tokenize: {error}", display(&path)))
                    }
                }
            }
        }
    }

    assert_no_failures(&failures);
}
