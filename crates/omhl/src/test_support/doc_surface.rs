//! Varredura de texto de documentação que repete o código: reprova medida sem lastro, contagem, lista copiada e bloco
//! `///` longo. Sem exceção marcada: reprovou, o autor melhora o código.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// Substantivos que, depois de um número, contam coisas que o código já define.
const COUNTED_NOUNS: [&str; 22] = [
    "códigos",
    "variantes",
    "campos",
    "tipos",
    "comandos",
    "regras",
    "etapas",
    "passos",
    "erros",
    "checks",
    "eventos",
    "papéis",
    "relações",
    "skills",
    "versões",
    "travas",
    "cópias",
    "níveis",
    "combinações",
    "camadas",
    "voltas",
    "formas",
];
const MEASURE_UNITS: &str = "ms|milissegundos?|µs|s|segundos?|minutos?|horas?|dias?|kib|mib|gib|kb|mb|gb|bytes?|núcleos?|threads?";
const TIME_CONTEXT: &str = r"(?i)secs|timeout|duration|millis|from_secs|_s\b";
const SIZE_CONTEXT: &str = r"(?i)bytes|size|max|limit|cap";
const PARALLEL_CONTEXT: &str = r"(?i)threads|cores|jobs|parallel";
const MAX_DOC_BLOCK_LINES: usize = 3;
const MIN_COPIED_IDENTIFIERS: usize = 3;
const IDENTIFIER: &str = r"`[A-Za-z_][A-Za-z0-9_]*`";
const LIST_SEPARATOR: &str = r"(?:\s*,\s*(?:(?:e|ou)\s+)?|\s+(?:e|ou)\s+|\s*[/|]\s*)";
const NUMBER_WORDS: &str = "dois|duas|três|quatro|cinco|seis|sete|oito|nove|dez";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Measure,
    Count,
    CopiedList,
    DocBlock,
}

impl Rule {
    fn label(self) -> &'static str {
        match self {
            Self::Measure => "measure",
            Self::Count => "count",
            Self::CopiedList => "copied-list",
            Self::DocBlock => "doc-block",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: Rule,
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{} {}",
            self.file,
            self.line,
            self.rule.label()
        )
    }
}

/// Linhas `///` (ou `//!`) contíguas, com o número da linha e o texto depois do marcador.
struct DocBlock {
    lines: Vec<(usize, String)>,
}

/// Números de código ou configuração (fora de comentário), por família de unidade que a mesma linha cita.
#[derive(Default)]
pub struct Backing {
    time: HashSet<String>,
    size: HashSet<String>,
    parallel: HashSet<String>,
}

impl Backing {
    pub fn from_code(code: &str) -> Self {
        let mut backing = Self::default();
        backing.add_code(code);

        backing
    }

    pub fn add_code(&mut self, code: &str) {
        let number = Regex::new(r"\b\d[\d_]*").expect("valid number pattern");
        let families = [
            (Regex::new(TIME_CONTEXT), &mut self.time),
            (Regex::new(SIZE_CONTEXT), &mut self.size),
            (Regex::new(PARALLEL_CONTEXT), &mut self.parallel),
        ];
        let mut families =
            families.map(|(pattern, numbers)| (pattern.expect("valid context pattern"), numbers));

        let lines = code
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//") && !line.starts_with('#'))
            .map(|line| line.split(" //").next().unwrap_or(line));

        for line in lines {
            for (context, numbers) in &mut families {
                if context.is_match(line) {
                    numbers.extend(
                        number
                            .find_iter(line)
                            .map(|token| token.as_str().replace('_', "")),
                    );
                }
            }
        }
    }

    fn numbers_for(&self, unit: &str) -> &HashSet<String> {
        match unit {
            "ms" | "milissegundo" | "milissegundos" | "µs" | "s" | "segundo" | "segundos"
            | "minuto" | "minutos" | "hora" | "horas" | "dia" | "dias" => &self.time,
            "núcleo" | "núcleos" | "thread" | "threads" => &self.parallel,
            _ => &self.size,
        }
    }
}

pub struct Scanner {
    measure: Regex,
    count: Regex,
    backticked: Regex,
    enumeration: Regex,
    bullet_identifier: Regex,
    defined: HashSet<String>,
    backed: Backing,
    test_literals: Vec<String>,
}

impl Scanner {
    /// `defined`: identificadores que o código define; `backed`: números de código ou configuração;
    /// `test_literals`: strings dos arquivos de teste, que guardam a seção de markdown cuja linha de título igualam.
    pub fn new(defined: HashSet<String>, backed: Backing, test_literals: Vec<String>) -> Self {
        let enumeration = format!("{IDENTIFIER}(?:{LIST_SEPARATOR}{IDENTIFIER})+");
        let measure = format!(r"(?i)\b(\d+(?:[.,]\d+)?)\s*({MEASURE_UNITS})\b");
        let count = format!(
            r"(?i)\b(?:\d+|{NUMBER_WORDS})\s+(?:{})\b",
            COUNTED_NOUNS.join("|")
        );

        Self {
            measure: Regex::new(&measure).expect("valid measure pattern"),
            count: Regex::new(&count).expect("valid count pattern"),
            backticked: Regex::new(r"`([A-Za-z_][A-Za-z0-9_]*)`").expect("valid backtick pattern"),
            enumeration: Regex::new(&enumeration).expect("valid enumeration pattern"),
            bullet_identifier: Regex::new(r"^\s*(?:[-*+]|\d+\.)\s+`([A-Za-z_]\w*)`[\s.,;:]*$")
                .expect("valid bullet pattern"),
            defined,
            backed,
            test_literals,
        }
    }

    pub fn rust_findings(&self, file: &str, source: &str) -> Vec<Finding> {
        let mut found = Vec::new();

        for block in rust_doc_blocks(source) {
            if block.lines.len() > MAX_DOC_BLOCK_LINES {
                found.push(finding(file, block.lines[0].0, Rule::DocBlock));
            }
            found.extend(self.text_findings(file, &block.lines));
        }

        found
    }

    pub fn markdown_findings(&self, file: &str, source: &str) -> Vec<Finding> {
        let lines = markdown_lines(source);
        let guarded = self.guarded_lines(&lines);

        self.text_findings(file, &lines)
            .into_iter()
            .filter(|found| !(found.rule == Rule::CopiedList && guarded.contains(&found.line)))
            .collect()
    }

    /// Linhas sob um título (e as seções filhas) cuja linha inteira, com `#`, é um literal de string de teste.
    fn guarded_lines(&self, lines: &[(usize, String)]) -> HashSet<usize> {
        let mut open_headings: Vec<(usize, bool)> = Vec::new();
        let mut guarded = HashSet::new();

        for (number, text) in lines {
            if let Some(level) = heading_level(text) {
                open_headings.retain(|(open, _)| *open < level);
                open_headings.push((level, self.is_cited_by_a_test(text.trim())));
            }
            if open_headings.iter().any(|(_, cited)| *cited) {
                guarded.insert(*number);
            }
        }

        guarded
    }

    // Igualdade: quem só contém o título não prova que um teste compara a seção com o código.
    fn is_cited_by_a_test(&self, heading_line: &str) -> bool {
        self.test_literals
            .iter()
            .any(|literal| literal == heading_line)
    }

    fn text_findings(&self, file: &str, lines: &[(usize, String)]) -> Vec<Finding> {
        let measures = lines
            .iter()
            .filter(|(_, text)| self.has_unbacked_measure(text));
        let counts = lines.iter().filter(|(_, text)| self.count.is_match(text));

        measures
            .map(|(number, _)| finding(file, *number, Rule::Measure))
            .chain(counts.map(|(number, _)| finding(file, *number, Rule::Count)))
            .chain(
                self.copied_list_starts(lines)
                    .into_iter()
                    .map(|number| finding(file, number, Rule::CopiedList)),
            )
            .collect()
    }

    /// Número em título é de seção, não medida.
    fn has_unbacked_measure(&self, text: &str) -> bool {
        if text.trim_start().starts_with('#') {
            return false;
        }

        self.measure.captures_iter(text).any(|captures| {
            let number: f64 = captures[1].replace(',', ".").parse().unwrap_or(0.0);

            let unit = captures[2].to_lowercase();
            let backed = self.backed.numbers_for(&unit);

            !unit_factors(&unit)
                .iter()
                .map(|factor| number * factor)
                .filter(|value| value.fract() == 0.0)
                .any(|value| backed.contains(&format!("{value:.0}")))
        })
    }

    /// Enumeração de identificadores que a varredura bloqueia, na linha ou em bullets seguidos de um identificador cada.
    fn copied_list_starts(&self, lines: &[(usize, String)]) -> Vec<usize> {
        let enumerated = lines
            .iter()
            .filter(|(_, text)| {
                self.enumeration
                    .find_iter(text)
                    .any(|run| self.defined_identifiers_in(run.as_str()) >= MIN_COPIED_IDENTIFIERS)
            })
            .map(|(number, _)| *number);

        enumerated
            .chain(self.identifier_bullet_blocks(lines))
            .collect()
    }

    fn identifier_bullet_blocks(&self, lines: &[(usize, String)]) -> Vec<usize> {
        let mut blocks: Vec<Vec<(usize, &str)>> = Vec::new();

        for (number, text) in lines {
            let Some(identifier) = self
                .bullet_identifier
                .captures(text)
                .and_then(|captures| captures.get(1))
            else {
                continue;
            };

            match blocks.last_mut() {
                Some(block)
                    if block
                        .last()
                        .is_some_and(|(previous, _)| previous + 1 == *number) =>
                {
                    block.push((*number, identifier.as_str()));
                }
                _ => blocks.push(vec![(*number, identifier.as_str())]),
            }
        }

        blocks
            .into_iter()
            .filter(|block| {
                let distinct: HashSet<&str> = block
                    .iter()
                    .map(|(_, identifier)| *identifier)
                    .filter(|identifier| self.defined.contains(*identifier))
                    .collect();

                distinct.len() >= MIN_COPIED_IDENTIFIERS
            })
            .map(|block| block[0].0)
            .collect()
    }

    fn defined_identifiers_in(&self, text: &str) -> usize {
        let distinct: HashSet<&str> = self
            .backticked
            .captures_iter(text)
            .filter_map(|captures| captures.get(1))
            .map(|identifier| identifier.as_str())
            .filter(|identifier| self.defined.contains(*identifier))
            .collect();

        distinct.len()
    }
}

fn heading_level(text: &str) -> Option<usize> {
    let level = text.chars().take_while(|c| *c == '#').count();
    let title = text[level..].strip_prefix(' ')?;

    ((1..=6).contains(&level) && !title.trim().is_empty()).then_some(level)
}

/// Conteúdo entre aspas das linhas de código; comentário (`//`, `///`) não conta.
pub fn string_literals(source: &str) -> Vec<String> {
    source
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .map(|line| line.split(" //").next().unwrap_or(line))
        .flat_map(|line| line.split('"').skip(1).step_by(2))
        .map(str::to_owned)
        .collect()
}

fn finding(file: &str, line: usize, rule: Rule) -> Finding {
    Finding {
        file: file.to_owned(),
        line,
        rule,
    }
}

/// Fatores pelos quais o número escrito vira outro literal que o código pode ter (ms, s, bytes).
fn unit_factors(unit: &str) -> &'static [f64] {
    match unit {
        "ms" | "milissegundo" | "milissegundos" => &[1.0, 0.001],
        "s" | "segundo" | "segundos" => &[1.0, 1e3],
        "minuto" | "minutos" => &[1.0, 60.0, 6e4],
        "hora" | "horas" => &[1.0, 3600.0, 3.6e6],
        "dia" | "dias" => &[1.0, 86_400.0, 8.64e7],
        "kb" => &[1.0, 1e3],
        "mb" => &[1.0, 1e3, 1e6],
        "gb" => &[1.0, 1e3, 1e6, 1e9],
        "kib" => &[1.0, 1024.0],
        "mib" => &[1.0, 1024.0, 1_048_576.0],
        "gib" => &[1.0, 1024.0, 1_048_576.0, 1_073_741_824.0],
        _ => &[1.0],
    }
}

enum LexState {
    Code,
    LineComment,
    Str,
    RawStr(usize),
    BlockComment(usize),
}

/// Para cada linha, se ela começa em código (e não dentro de string ou de comentário de bloco).
fn lines_starting_in_code(source: &str) -> Vec<bool> {
    let chars: Vec<char> = source.chars().collect();
    let mut starts = vec![true];
    let mut state = LexState::Code;
    let mut i = 0;

    while i < chars.len() {
        let current = chars[i];
        let next = chars.get(i + 1).copied();

        if current == '\n' {
            if matches!(state, LexState::LineComment) {
                state = LexState::Code;
            }
            starts.push(matches!(state, LexState::Code));
            i += 1;
            continue;
        }

        match state {
            LexState::Code => match (current, next) {
                ('/', Some('/')) => {
                    state = LexState::LineComment;
                    i += 2;
                }
                ('/', Some('*')) => {
                    state = LexState::BlockComment(1);
                    i += 2;
                }
                ('"', _) => {
                    state = LexState::Str;
                    i += 1;
                }
                ('r', _) if raw_string_hashes(&chars, i).is_some() => {
                    let hashes = raw_string_hashes(&chars, i).unwrap_or(0);
                    state = LexState::RawStr(hashes);
                    i += 2 + hashes;
                }
                ('\'', Some('\\')) => i = skip_escaped_char_literal(&chars, i),
                ('\'', _) if chars.get(i + 2) == Some(&'\'') => i += 3,
                _ => i += 1,
            },
            LexState::LineComment => i += 1,
            LexState::Str => match current {
                '\\' if next == Some('\n') => i += 1,
                '\\' => i += 2,
                '"' => {
                    state = LexState::Code;
                    i += 1;
                }
                _ => i += 1,
            },
            LexState::RawStr(hashes) => {
                let closes = current == '"' && (1..=hashes).all(|n| chars.get(i + n) == Some(&'#'));
                if closes {
                    state = LexState::Code;
                    i += 1 + hashes;
                } else {
                    i += 1;
                }
            }
            LexState::BlockComment(depth) => match (current, next) {
                ('/', Some('*')) => {
                    state = LexState::BlockComment(depth + 1);
                    i += 2;
                }
                ('*', Some('/')) => {
                    state = if depth == 1 {
                        LexState::Code
                    } else {
                        LexState::BlockComment(depth - 1)
                    };
                    i += 2;
                }
                _ => i += 1,
            },
        }
    }

    starts
}

/// Quantos `#` abrem a string crua que começa em `r` (`r"`, `r#"`), ou `None` se não é um começo.
fn raw_string_hashes(chars: &[char], at: usize) -> Option<usize> {
    let after_identifier = at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_');
    if after_identifier && chars[at - 1] != 'b' {
        return None;
    }

    let hashes = chars[at + 1..].iter().take_while(|c| **c == '#').count();

    (chars.get(at + 1 + hashes) == Some(&'"')).then_some(hashes)
}

/// `'\n'`, `'\''`, `'\u{..}'`: pula o escape e vai até a aspa que fecha.
fn skip_escaped_char_literal(chars: &[char], at: usize) -> usize {
    chars
        .get(at + 3..)
        .unwrap_or_default()
        .iter()
        .position(|c| *c == '\'')
        .map_or(at + 1, |offset| at + 3 + offset + 1)
}

fn bracket_delta(line: &str) -> isize {
    let opens = line.matches('[').count();
    let closes = line.matches(']').count();

    opens as isize - closes as isize
}

fn doc_marker(line: &str) -> Option<&'static str> {
    match line {
        _ if line.starts_with("////") => None,
        _ if line.starts_with("///") => Some("///"),
        _ if line.starts_with("//!") => Some("//!"),
        _ => None,
    }
}

/// Blocos de `///` (ou `//!`) contíguos; um atributo `#[...]` entre as linhas não quebra o bloco.
fn rust_doc_blocks(source: &str) -> Vec<DocBlock> {
    let starts_in_code = lines_starting_in_code(source);
    let mut blocks: Vec<DocBlock> = Vec::new();
    let mut open: Option<(&str, DocBlock)> = None;
    let mut attribute_depth = 0;

    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim_start();

        if attribute_depth > 0 {
            attribute_depth += bracket_delta(line);
            continue;
        }

        let marker = doc_marker(line).filter(|_| starts_in_code[index]);
        let text = marker.map(|marker| line[marker.len()..].to_owned());

        match (marker, text, open.take()) {
            (Some(marker), Some(text), Some((open_marker, mut block))) if open_marker == marker => {
                block.lines.push((index + 1, text));
                open = Some((marker, block));
            }
            (Some(marker), Some(text), previous) => {
                blocks.extend(previous.map(|(_, block)| block));
                open = Some((
                    marker,
                    DocBlock {
                        lines: vec![(index + 1, text)],
                    },
                ));
            }
            (_, _, Some((marker, block))) if line.starts_with("#[") => {
                attribute_depth = bracket_delta(line);
                open = Some((marker, block));
            }
            (_, _, previous) => blocks.extend(previous.map(|(_, block)| block)),
        }
    }

    blocks.extend(open.map(|(_, block)| block));

    blocks
}

/// Abertura de cerca (crases ou til): o caractere e o tamanho, que a cerca de fechamento iguala ou passa.
fn fence_opening(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let length = trimmed.chars().take_while(|c| *c == marker).count();

    (length >= 3).then_some((marker, length))
}

/// Linhas de markdown fora de blocos de código cercados.
fn markdown_lines(source: &str) -> Vec<(usize, String)> {
    let mut open_fence: Option<(char, usize)> = None;
    let mut lines = Vec::new();

    for (index, line) in source.lines().enumerate() {
        match (open_fence, fence_opening(line)) {
            (None, Some(opening)) => open_fence = Some(opening),
            (Some((marker, length)), Some((found, found_length)))
                if found == marker
                    && found_length >= length
                    && line.trim().chars().all(|c| c == marker) =>
            {
                open_fence = None;
            }
            (None, None) => lines.push((index + 1, line.to_owned())),
            _ => {}
        }
    }

    lines
}

/// Nomes definidos em código: definições Rust e variantes de enum.
pub fn defined_identifiers(source: &str) -> HashSet<String> {
    let definition = Regex::new(
        r"\b(?:fn|struct|enum|trait|const|static|type|mod)\s+(?:(?:fn|mut|dyn|unsafe|extern)\s+)?([A-Za-z_]\w*)",
    )
    .expect("valid definition pattern");
    let variant = Regex::new(r"^([A-Z]\w*)\s*(?:[,({=]|$)").expect("valid variant pattern");
    let mut names = HashSet::new();
    let mut enum_depth: Option<isize> = None;

    for raw in source.lines() {
        let line = raw.trim();
        if line.starts_with("//") {
            continue;
        }

        if let (Some(1), Some(captures)) = (enum_depth, variant.captures(line)) {
            names.insert(captures[1].to_owned());
        }
        names.extend(
            definition
                .captures_iter(line)
                .map(|captures| captures[1].to_owned()),
        );

        let delta = line.matches('{').count() as isize - line.matches('}').count() as isize;
        enum_depth = match enum_depth {
            Some(depth) if depth + delta > 0 => Some(depth + delta),
            Some(_) => None,
            None if line.contains("enum ") && delta > 0 => Some(delta),
            None => None,
        };
    }

    names
}

fn files_under(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            files_under(&path, extension, out);
        } else if path.extension().is_some_and(|found| found == extension) {
            out.push(path);
        }
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// O texto de `--help` da CLI fica de fora: é interface, não a documentação que a varredura vigia.
fn is_scanned_rust(root: &Path, path: &Path) -> bool {
    let relative_path = relative(root, path);
    let under_src = relative_path.split('/').nth(2) == Some("src");

    under_src && relative_path != "crates/omhl/src/adapters/cli.rs" && !is_test_path(&relative_path)
}

fn is_test_path(relative_path: &str) -> bool {
    relative_path
        .split('/')
        .any(|part| part == "tests" || part == "tests.rs")
}

/// Fixture de teste não dá lastro: o número dela só prova que o teste existe.
fn backing_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    files_under(&root.join("crates"), "rs", &mut files);
    files.retain(|path| !is_test_path(&relative(root, path)));
    files_under(&root.join(".omhl"), "toml", &mut files);
    files_under(&root.join("crates"), "toml", &mut files);
    files_under(&root.join(".github/workflows"), "yml", &mut files);
    files.extend(Some(root.join("Cargo.toml")).filter(|path| path.is_file()));

    files
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ["README.md", "GLOSSARY.md", "AGENTS.md"]
        .iter()
        .map(|name| root.join(name))
        .filter(|path| path.is_file())
        .collect();
    let mut docs = Vec::new();
    files_under(&root.join("docs"), "md", &mut docs);
    files.extend(
        docs.into_iter()
            .filter(|path| path.parent() == Some(&root.join("docs"))),
    );

    files
}

fn is_test_file(root: &Path, path: &Path) -> bool {
    let relative_path = relative(root, path);
    let mut parts = relative_path.split('/').skip(2);

    match parts.next() {
        Some("tests") => true,
        Some("src") => path.file_name().is_some_and(|name| name == "tests.rs"),
        _ => false,
    }
}

pub fn scan_repo(root: &Path) -> Vec<Finding> {
    let mut all_rust = Vec::new();
    files_under(&root.join("crates"), "rs", &mut all_rust);
    let rust_files: Vec<&PathBuf> = all_rust
        .iter()
        .filter(|path| relative(root, path).split('/').nth(2) == Some("src"))
        .collect();
    let test_literals = all_rust
        .iter()
        .filter(|path| is_test_file(root, path))
        .flat_map(|path| string_literals(&read(path)))
        .collect();

    let defined = rust_files
        .iter()
        .flat_map(|path| defined_identifiers(&read(path)))
        .collect();
    let mut backed = Backing::default();
    for path in backing_files(root) {
        backed.add_code(&read(&path));
    }
    let scanner = Scanner::new(defined, backed, test_literals);

    let rust_found = rust_files
        .iter()
        .filter(|path| is_scanned_rust(root, path))
        .flat_map(|path| scanner.rust_findings(&relative(root, path), &read(path)));
    let markdown_found = markdown_files(root)
        .into_iter()
        .flat_map(|path| scanner.markdown_findings(&relative(root, &path), &read(&path)));

    let mut found: Vec<Finding> = rust_found.chain(markdown_found).collect();
    found.sort_by(|a, b| (&a.file, a.line, a.rule.label()).cmp(&(&b.file, b.line, b.rule.label())));

    found
}

#[cfg(test)]
mod tests;
