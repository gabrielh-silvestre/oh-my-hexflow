#![cfg(target_os = "linux")]
//! `skill_examples`: executa os blocos `omhl-example` dos `references/*.md` contra o binário real.
//!
//! Gramática de um bloco (fixa; as skills a seguem):
//!
//! ````text
//! ```omhl-example fixture=git
//! $ omhl get work-items.w1
//! {"node":null}
//! $ omhl append <<'EOF'
//! {"items":[]}
//! EOF
//! 2> {"error":{"code":"invalid-input"}}
//! # exit 1
//! ```
//! ````
//!
//! - `fixture=<nome>` escolhe a fixture montada em Rust (tabela em `Fixture::build`); cada bloco recebe
//!   uma fixture nova, e os `$ omhl ...` do bloco rodam em ordem no mesmo diretório.
//! - `$ omhl <args>`: separação por espaço, aspas simples agrupam, sem shell. Terminar a linha em
//!   `<<'EOF'` faz as linhas até a linha `EOF` serem o stdin.
//! - Linhas seguintes: stdout esperado; `2> ` prefixa uma linha do stderr; `# exit N` fixa o código
//!   (padrão 0).
//! - Os dois lados são normalizados (datas, ids, hashes, durações, raízes temporárias, binário) e
//!   comparados como JSON quando os dois parseiam; senão, como texto.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::Value;

use crate::skills::coverage::{leaf_commands, read_docs, skills_dir, starts_with_path};
use crate::support::child::omhl;
use crate::support::git::{expect_git_success, git_command_with_env};
use crate::support::project::Project;
use crate::support::repo_checks::repo_root;
use crate::support::sweep_lock::serialize_sweeps;

const FENCE_TAG: &str = "omhl-example";
const FENCE: &str = "```omhl-example";
const BRANCH: &str = "feat/wi-w1__x";
const GIT_DATE: &str = "2026-01-01T00:00:00Z";
const WORK_ITEM: &str = r#"{"items":[{"body":{"kind":"node","id":"work-items.w1","type":"work-item","data":{"title":"First item","status":"open"}}}]}"#;
const PREMISE: &str = r#"{"items":[{"body":{"kind":"node","id":"premises.p1","type":"premise","data":{"text":"Money is stored in cents.","status":"confirmed","kind":"business","work_item":"work-items.w1","confirmed_by":{"by":"person:ana","question":"How is money stored?","answer":"In cents."}}}}]}"#;
const MECHANICAL_CHECK: &str = "blocking = true\nhow = \"Always passes.\"\nrun = [\"true\"]\ntimeout_secs = 60\nexpect = [{ exit = 0, outcome = \"passed\" }]\n\n[when]\nevent = \"before-pr\"\n";
const FAILING_CHECK: &str = "blocking = true\nhow = \"Always fails.\"\nrun = [\"sh\", \"-c\", \"echo first; echo second; echo boom >&2; exit 1\"]\ntimeout_secs = 60\nexpect = [{ exit = 0, outcome = \"passed\" }]\n\n[when]\nevent = \"before-pr\"\n";
const SOFT_CHECK: &str = "blocking = false\nhow = \"Fails but does not block.\"\nrun = [\"sh\", \"-c\", \"exit 1\"]\ntimeout_secs = 60\nexpect = [{ exit = 0, outcome = \"passed\" }]\n\n[when]\nevent = \"before-pr\"\n";
const GHOST_CHECK: &str = "blocking = true\nhow = \"Binary does not exist.\"\nrun = [\"omhl-ghost-binary\"]\ntimeout_secs = 60\nexpect = [{ exit = 0, outcome = \"passed\" }]\n\n[when]\nevent = \"before-pr\"\n";
const MANUAL_CHECK: &str =
    "blocking = false\nhow = \"Review it by hand.\"\n\n[when]\nevent = \"before-merge\"\n";
const GUIDELINE: &str = "kind = \"business\"\ntext = \"Be brief.\"\n";
const SPEC_PART: &str = "## Objetivo\n\nGuardar o dinheiro em centavos.\n\n## Diretivas e premissas aplicáveis\n\n- premises.p1\n\n## Comportamentos\n\n- CMP-001: o sistema guarda o valor em centavos.\n\n## Critérios de aceitação\n\n- CA-001: o valor 1,50 vira 150.\n\n## Fora de escopo\n\nMoedas.\n";
const SESSION_DECISION: &str = r#"{"items":[{"body":{"kind":"node","id":"decisions.d1","type":"decision","data":{"text":"Use integers.","evidence":[{"source":"docs","summary":"Cents are integers."}]}}},{"body":{"kind":"relation","relation_type":"supports","from":"premises.p1","to":"decisions.d1"}}]}"#;
const SESSION_GAP: &str = r#"{"items":[{"body":{"kind":"node","id":"gaps.limit","type":"gap","data":{"question":"What is the row limit?","context":"CSV export.","work_item":"work-items.w1"}}}]}"#;
const DEVELOP_MARK: &str = "omhl-develop-mark v1\n";

// Parser dos blocos

#[derive(Debug, PartialEq)]
struct Step {
    args: Vec<String>,
    stdin: Option<String>,
    stdout: String,
    stderr: String,
    code: i32,
}

#[derive(Debug)]
struct Block {
    file: String,
    line: usize,
    fixture: String,
    steps: Vec<Step>,
}

/// Separa por espaço; aspas simples agrupam (`''` é um argumento vazio); sem shell.
fn split_args(text: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let (mut started, mut quoted) = (false, false);
    for ch in text.chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                started = true;
            }
            ' ' if !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            _ => {
                current.push(ch);
                started = true;
            }
        }
    }
    if quoted {
        return Err(format!("unterminated quote in `{text}`"));
    }
    if started {
        args.push(current);
    }

    Ok(args)
}

fn join_lines(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

struct Opener<'a> {
    indent: usize,
    ch: char,
    len: usize,
    info: &'a str,
}

/// Abre uma cerca à la CommonMark: recuo, três ou mais crases ou tis e a linha de informação.
fn opener(line: &str) -> Option<Opener<'_>> {
    let rest = line.trim_start_matches(' ');
    let ch = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let len = rest.chars().take_while(|c| *c == ch).count();

    (len >= 3).then(|| Opener {
        indent: line.len() - rest.len(),
        ch,
        len,
        info: rest[len..].trim(),
    })
}

fn closes(line: &str, open: &Opener) -> bool {
    let rest = line.trim_start_matches(' ').trim_end();

    rest.len() >= open.len && rest.chars().all(|c| c == open.ch)
}

/// Reprova o bloco que parece exemplo mas não executa: o `parse_blocks` só vê a cerca exata.
fn check_fences(file: &str, text: &str) -> Result<(), String> {
    let command = Regex::new(r"^\s*(?:\$ )?omhl\s+[a-z]").expect("regex");
    let prompt = Regex::new(r"^\s*\$ omhl\s").expect("regex");
    let header = Regex::new(&format!(r"^{FENCE_TAG} fixture=\S+$")).expect("regex");
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let at = format!("{file}:{}", index + 1);
        let Some(open) = opener(line) else {
            if prompt.is_match(line) {
                return Err(format!(
                    "{at}: `$ omhl` outside a fence; use an {FENCE_TAG} block"
                ));
            }
            continue;
        };
        let body: Vec<&str> = lines
            .by_ref()
            .map(|(_, inner)| inner)
            .take_while(|inner| !closes(inner, &open))
            .collect();
        let tag = open.info.split_whitespace().next().unwrap_or("");
        if tag == FENCE_TAG {
            let exact = open.indent == 0 && open.ch == '`' && open.len == 3;
            if !exact || !header.is_match(open.info) {
                return Err(format!(
                    "{at}: the fence must be exactly ```{FENCE_TAG} fixture=<name>"
                ));
            }
            if !body.iter().any(|inner| inner.starts_with("$ omhl ")) {
                return Err(format!("{at}: the {FENCE_TAG} block has no `$ omhl` step"));
            }
        } else if tag.starts_with("omhl") {
            return Err(format!(
                "{at}: unknown fence `{tag}`; the only executable one is {FENCE_TAG}"
            ));
        } else if body.iter().any(|inner| command.is_match(inner)) {
            return Err(format!(
                "{at}: an `omhl` command block must be an {FENCE_TAG} block"
            ));
        }
    }

    Ok(())
}

fn parse_blocks(file: &str, text: &str) -> Result<Vec<Block>, String> {
    let mut blocks = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let Some(header) = line.strip_prefix(FENCE) else {
            continue;
        };
        let at = format!("{file}:{}", index + 1);
        let fixture = header
            .trim()
            .strip_prefix("fixture=")
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("{at}: the fence needs `fixture=<name>`"))?;
        let mut steps: Vec<Step> = Vec::new();
        let mut stdin: Option<Vec<&str>> = None;
        let mut closed = false;
        let (mut out, mut err): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
        for (_, body) in lines.by_ref() {
            if let Some(heredoc) = stdin.as_mut() {
                if body == "EOF" {
                    let step = steps.last_mut().expect("a heredoc follows a command");
                    step.stdin = Some(join_lines(heredoc));
                    stdin = None;
                } else {
                    heredoc.push(body);
                }
                continue;
            }
            if body == "```" {
                closed = true;
                break;
            }
            if let Some(command) = body.strip_prefix("$ omhl ") {
                if let Some(step) = steps.last_mut() {
                    step.stdout = out.join("\n");
                    step.stderr = err.join("\n");
                }
                (out, err) = (Vec::new(), Vec::new());
                let (command, heredoc) = match command.strip_suffix("<<'EOF'") {
                    Some(command) => (command, true),
                    None => (command, false),
                };
                steps.push(Step {
                    args: split_args(command).map_err(|error| format!("{at}: {error}"))?,
                    stdin: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    code: 0,
                });
                stdin = heredoc.then(Vec::new);
                continue;
            }
            let step = steps
                .last_mut()
                .ok_or_else(|| format!("{at}: output before any `$ omhl` line"))?;
            if let Some(code) = body.strip_prefix("# exit ") {
                step.code = code
                    .trim()
                    .parse()
                    .map_err(|_| format!("{at}: `{body}` is not an exit code"))?;
            } else if let Some(line) = body.strip_prefix("2> ") {
                err.push(line);
            } else {
                out.push(body);
            }
        }
        if !closed || stdin.is_some() {
            return Err(format!("{at}: the block is not closed"));
        }
        if let Some(step) = steps.last_mut() {
            step.stdout = out.join("\n");
            step.stderr = err.join("\n");
        }
        blocks.push(Block {
            file: file.to_owned(),
            line: index + 1,
            fixture: fixture.to_owned(),
            steps,
        });
    }

    Ok(blocks)
}

// Normalização

struct Normalizer {
    paths: Vec<(String, &'static str)>,
    rules: Vec<(Regex, &'static str)>,
}

impl Normalizer {
    fn new(dir: &Path, xdg: &Path) -> Normalizer {
        let binary = PathBuf::from(env!("CARGO_BIN_EXE_omhl"));
        let mut paths = Vec::new();
        for (path, label) in [
            (xdg, "<xdg>"),
            (dir, "<repo>"),
            (binary.as_path(), "<omhl>"),
        ] {
            paths.push((path.to_string_lossy().into_owned(), label));
            if let Ok(canonical) = fs::canonicalize(path) {
                paths.push((canonical.to_string_lossy().into_owned(), label));
            }
        }
        // O mais longo primeiro: o xdg e o binário não podem sobrar sob a raiz do repositório.
        paths.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
        let rules = [
            (r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?", "<at>"),
            (r"[a-z0-9][a-z0-9-]*:[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}", "<record-id>"),
            (r"\b[0-9a-f]{64}\b", "<hash>"),
            (r"\b[0-9a-f]{40}\b", "<commit>"),
            (r#"("(?:duration[a-z_]*|[a-z_]*_ms|elapsed[a-z_]*)"\s*:\s*)\d+(?:\.\d+)?"#, r#"${1}"<ms>""#),
            // Texto do `check run`: durações (`12ms`, `2.6s`) e o id curto do snapshot, `(git-commit c43356d)`.
            (r"\b\d+ms\b", "<ms>"),
            (r"\b\d+\.\ds\b", "<s>"),
            (r"\((git-commit|git-tree) [0-9a-f]{7}\)", "(${1} <snapshot>)"),
            // Texto do `attest`: `snapshot 2dbf9e3`.
            (r"\bsnapshot [0-9a-f]{7}\b", "snapshot <snapshot>"),
            (r#"("(?:pid|pgid|[a-z_]*_pid)"\s*:\s*)\d+"#, r#"${1}"<pid>""#),
            // `check sweep` vale para qualquer raiz da máquina: `swept` e `unreadable` não são determinísticos.
            (r#"("(?:swept|unreadable)"\s*:\s*)(?:\d+|\[[^\]]*\])"#, r#"${1}"<swept>""#),
            // Texto do `check sweep`: `- sleep · pid 4242 · owner 4100` e `2 swept · 0 unreadable`.
            (r"\b(pid|owner) \d+\b", "${1} <pid>"),
            (r"\b\d+ (swept|unreadable)\b", "<n> ${1}"),
        ]
        .map(|(pattern, to)| (Regex::new(pattern).expect("regex"), to));

        Normalizer {
            paths,
            rules: rules.to_vec(),
        }
    }

    fn apply(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for (path, label) in &self.paths {
            text = text.replace(path.as_str(), label);
        }
        for (rule, to) in &self.rules {
            text = rule.replace_all(&text, *to).into_owned();
        }

        text.trim_end().to_owned()
    }

    /// JSON quando os dois lados parseiam; senão, texto normalizado.
    fn same(&self, expected: &str, obtained: &str) -> bool {
        let (expected, obtained) = (self.apply(expected), self.apply(obtained));
        match (
            serde_json::from_str::<Value>(&expected),
            serde_json::from_str::<Value>(&obtained),
        ) {
            (Ok(left), Ok(right)) => left == right,
            _ => expected == obtained,
        }
    }
}

// Fixtures

struct Fixture {
    project: Project,
}

impl Fixture {
    fn dir(&self) -> &Path {
        self.project.dir()
    }

    fn xdg(&self) -> &Path {
        self.project.xdg()
    }

    fn append(&self, request: &str) {
        let output = omhl(&["append"])
            .cwd(self.dir())
            .xdg(self.xdg())
            .env("CLAUDECODE", "1")
            .stdin(request)
            .run();
        assert_eq!(output.code, Some(0), "fixture append: {}", output.stderr);
    }

    /// O filho roda com o ambiente limpo: só assim um exemplo tem uma sessão gravada.
    fn append_in_session(&self, session: &str, request: &str) {
        let output = omhl(&["append"])
            .cwd(self.dir())
            .xdg(self.xdg())
            .env("CLAUDECODE", "1")
            .env("CLAUDE_CODE_SESSION_ID", session)
            .stdin(request)
            .run();
        assert_eq!(output.code, Some(0), "fixture append: {}", output.stderr);
    }

    fn omhl_ok(&self, args: &[&str]) {
        let output = omhl(args)
            .cwd(self.dir())
            .xdg(self.xdg())
            .env("CLAUDECODE", "1")
            .run();
        assert_eq!(output.code, Some(0), "fixture {args:?}: {}", output.stderr);
    }

    /// Datas do git fixas só aqui: o hash do commit entra no `snapshot` dos exemplos.
    fn git(&self, args: &[&str]) {
        let output = git_command_with_env(self.dir(), self.xdg(), self.xdg(), args)
            .env("GIT_AUTHOR_DATE", GIT_DATE)
            .env("GIT_COMMITTER_DATE", GIT_DATE)
            .output()
            .expect("git runs");
        expect_git_success(&output, args);
    }

    fn write(&self, rel: &str, content: &str) {
        let target = self.dir().join(rel);
        fs::create_dir_all(target.parent().expect("a parent")).expect("parent");
        fs::write(target, content).expect("fixture file");
    }

    fn seed(&self) {
        self.append(WORK_ITEM);
        self.append(PREMISE);
    }

    /// Repositório com o marcador (e tudo que já está na pasta) commitado, na branch do work item.
    fn init_git(&self) {
        self.git(&["init", "-b", "main"]);
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", "base"]);
        self.git(&["switch", "-c", BRANCH]);
    }

    /// Fixture por nome. Cada uma estende a anterior da mesma linha.
    fn build(name: &str) -> Result<Fixture, String> {
        let fixture = Fixture {
            project: Project::new("acme"),
        };
        match name {
            "project" => fixture.seed(),
            "sessions" => {
                fixture.seed();
                fixture.append_in_session("5b1c0f3e-77a2-4c1e-9d0b-2f6a8e41c9aa", SESSION_DECISION);
                fixture.append_in_session("0e4d2a96-3c58-4f0b-8a17-d6b9c1e75f02", SESSION_GAP);
            }
            "git" => {
                fixture.seed();
                fixture.init_git();
            }
            "spec" => {
                fixture.seed();
                fixture.init_git();
                fixture.omhl_ok(&["spec", "init", "work-items.w1", "x"]);
                fixture.write(".omhl/specs/wi-w1__x/part-01.md", SPEC_PART);
            }
            "checks" | "retire-check" | "dirty-checks" => {
                fixture.write(".omhl/checks/ok.toml", MECHANICAL_CHECK);
                fixture.write(".omhl/checks/review.toml", MANUAL_CHECK);
                fixture.seed();
                fixture.init_git();
                fixture.omhl_ok(&["load"]);
                if name == "dirty-checks" {
                    fixture.write("notes.txt", "not committed\n");
                }
                if name == "retire-check" {
                    fs::remove_file(fixture.dir().join(".omhl/checks/ok.toml"))
                        .expect("the check file is removed");
                }
            }
            "failing-check" | "soft-check" | "missing-binary" => {
                let (file, toml) = match name {
                    "failing-check" => ("bad", FAILING_CHECK),
                    "soft-check" => ("soft", SOFT_CHECK),
                    _ => ("ghost", GHOST_CHECK),
                };
                fixture.write(&format!(".omhl/checks/{file}.toml"), toml);
                fixture.seed();
                fixture.init_git();
                fixture.omhl_ok(&["load"]);
            }
            "retire-guideline" => {
                fixture.seed();
                fixture.write(".omhl/guidelines/brief.toml", GUIDELINE);
                fixture.omhl_ok(&["load"]);
                fs::remove_file(fixture.dir().join(".omhl/guidelines/brief.toml"))
                    .expect("the guideline file is removed");
            }
            "develop-mark" => {
                fixture.seed();
                fixture.init_git();
                fixture.git(&["switch", "-c", "develop"]);
                fixture.write(".git/omhl-develop-mark", DEVELOP_MARK);
            }
            other => return Err(format!("unknown fixture `{other}`")),
        }

        Ok(fixture)
    }
}

// Execução

fn run_block(block: &Block) -> Result<(), String> {
    let at = format!("{}:{}", block.file, block.line);
    let fixture = Fixture::build(&block.fixture).map_err(|error| format!("{at}: {error}"))?;
    let normalizer = Normalizer::new(fixture.dir(), fixture.xdg());
    for step in &block.steps {
        let args: Vec<&str> = step.args.iter().map(String::as_str).collect();
        let mut command = omhl(&args)
            .cwd(fixture.dir())
            .xdg(fixture.xdg())
            .env("CLAUDECODE", "1");
        if let Some(stdin) = &step.stdin {
            command = command.stdin(stdin);
        }
        let output = command.run();
        let code = output.code.unwrap_or(-1);
        if code != step.code
            || !normalizer.same(&step.stdout, &output.stdout)
            || !normalizer.same(&step.stderr, &output.stderr)
        {
            return Err(format!(
                "{at}: $ omhl {}\n  expected exit {} stdout {:?} stderr {:?}\n  obtained exit {code} stdout {:?} stderr {:?}",
                step.args.join(" "),
                step.code,
                normalizer.apply(&step.stdout),
                normalizer.apply(&step.stderr),
                normalizer.apply(&output.stdout),
                normalizer.apply(&output.stderr),
            ));
        }
    }

    Ok(())
}

fn real_blocks() -> Vec<Block> {
    let mut blocks = Vec::new();
    for doc in &read_docs(&skills_dir()) {
        let file = format!("plugin/skills/{}", doc.path);
        check_fences(&file, &doc.text).unwrap_or_else(|error| panic!("{error}"));
        if doc.path.contains("/references/") {
            blocks.extend(parse_blocks(&file, &doc.text).unwrap_or_else(|error| panic!("{error}")));
        }
    }

    blocks
}

/// Folhas sem nenhum passo `$ omhl <caminho da folha>` nos blocos da própria página `references/<folha>.md`.
fn leaves_without_example(blocks: &[Block], leaves: Vec<String>) -> Vec<String> {
    leaves
        .into_iter()
        .filter(|leaf| {
            let page = format!("/references/{}.md", leaf.replace(' ', "-"));

            !blocks
                .iter()
                .filter(|block| block.file.ends_with(&page))
                .flat_map(|block| &block.steps)
                .any(|step| starts_with_path(&step.args, leaf))
        })
        .collect()
}

#[test]
fn skill_examples() {
    let _serial = serialize_sweeps();
    let blocks = real_blocks();

    let missing = leaves_without_example(&blocks, leaf_commands());

    assert!(
        missing.is_empty(),
        "subcommands without an omhl-example block in their own page under {}: {missing:?}",
        repo_root().join("plugin/skills").display()
    );
    let failures: Vec<String> = blocks
        .iter()
        .filter_map(|block| run_block(block).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Provas sintéticas: o parser, as fixtures e o relatório de divergência

fn one_block(fixture: &str, body: &str) -> Block {
    let text = format!("intro\n{FENCE} fixture={fixture}\n{body}```\n");
    let mut blocks = parse_blocks("synthetic.md", &text).expect("the block parses");
    assert_eq!(blocks.len(), 1);

    blocks.remove(0)
}

fn assert_runs(fixture: &str, body: &str) {
    let result = run_block(&one_block(fixture, body));

    assert!(result.is_ok(), "{}", result.unwrap_err());
}

#[test]
fn it_should_split_arguments_by_space_and_single_quotes_without_a_shell() {
    let args = split_args("list --where 'text=two words' '' $HOME").expect("it splits");

    assert_eq!(args, ["list", "--where", "text=two words", "", "$HOME"]);
}

#[test]
fn it_should_parse_several_commands_stdin_stderr_and_exit_code_of_a_block() {
    let block = one_block(
        "project",
        "$ omhl get work-items.w1\n{\"node\":null}\n$ omhl append <<'EOF'\nnot json\nsecond line\nEOF\n2> boom\n2> bang\n# exit 1\n$ omhl verify\n",
    );

    assert_eq!(
        (block.file, block.line, block.fixture, block.steps),
        (
            "synthetic.md".to_owned(),
            2,
            "project".to_owned(),
            vec![
                Step {
                    args: vec!["get".into(), "work-items.w1".into()],
                    stdin: None,
                    stdout: "{\"node\":null}".into(),
                    stderr: String::new(),
                    code: 0,
                },
                Step {
                    args: vec!["append".into()],
                    stdin: Some("not json\nsecond line\n".into()),
                    stdout: String::new(),
                    stderr: "boom\nbang".into(),
                    code: 1,
                },
                Step {
                    args: vec!["verify".into()],
                    stdin: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    code: 0,
                },
            ]
        )
    );
}

#[test]
fn it_should_refuse_a_block_without_fixture_or_never_closed() {
    let no_fixture = parse_blocks("a.md", &format!("{FENCE}\n$ omhl verify\n```\n"));
    let unclosed = parse_blocks("a.md", &format!("{FENCE} fixture=git\n$ omhl verify\n"));

    assert_eq!((no_fixture.is_err(), unclosed.is_err()), (true, true));
}

#[test]
fn it_should_not_count_an_example_of_a_leaf_that_sits_in_another_page() {
    let body = "```omhl-example fixture=project\n$ omhl list\n{\"items\":[]}\n```\n";
    let own = parse_blocks("s/references/list.md", body).expect("parses");
    let other = parse_blocks("s/references/errors.md", body).expect("parses");
    let leaves = || vec!["list".to_owned()];

    let (with_own, with_other) = (
        leaves_without_example(&own, leaves()),
        leaves_without_example(&other, leaves()),
    );

    assert_eq!((with_own, with_other), (vec![], vec!["list".to_owned()]));
}

fn fences(text: &str) -> Result<(), String> {
    check_fences("synthetic.md", text)
}

#[test]
fn it_should_refuse_an_indented_omhl_example_fence() {
    let result = fences("  ```omhl-example fixture=git\n$ omhl verify\n  ```\n");

    assert!(result.is_err());
}

#[test]
fn it_should_refuse_an_omhl_example_fence_with_four_backticks_or_tildes() {
    let backticks = fences("````omhl-example fixture=git\n$ omhl verify\n````\n");
    let tildes = fences("~~~omhl-example fixture=git\n$ omhl verify\n~~~\n");

    assert_eq!((backticks.is_err(), tildes.is_err()), (true, true));
}

#[test]
fn it_should_refuse_an_omhl_example_fence_without_fixture() {
    let result = fences("```omhl-example\n$ omhl verify\n```\n");

    assert!(result.is_err());
}

#[test]
fn it_should_refuse_a_fence_tag_that_starts_with_omhl_but_is_not_omhl_example() {
    let typo = fences("```omhl-exemple fixture=git\n{\"ok\":true}\n```\n");
    let bare = fences("```omhl\n{\"ok\":true}\n```\n");

    assert_eq!((typo.is_err(), bare.is_err()), (true, true));
}

#[test]
fn it_should_refuse_an_omhl_example_block_without_a_command_step() {
    let only_output = fences("```omhl-example fixture=git\n{\"ok\":true}\n```\n");
    let empty = fences("```omhl-example fixture=git\n```\n");

    assert_eq!((only_output.is_err(), empty.is_err()), (true, true));
}

#[test]
fn it_should_refuse_an_omhl_command_in_a_block_that_is_not_omhl_example() {
    let result = fences("```bash\nomhl list --type divergence\n```\n");

    assert!(result.is_err());
}

#[test]
fn it_should_refuse_a_dollar_omhl_line_outside_any_fence() {
    let result = fences("intro\n$ omhl verify\n");

    assert!(result.is_err());
}

#[test]
fn it_should_accept_a_valid_omhl_example_and_blocks_that_do_not_run_omhl() {
    let text = "```omhl-example fixture=git\n$ omhl verify\n{\"ok\":true}\n```\n\n```bash\ncargo test\n```\n\n```text\nrun omhl list later\n```\n";

    let result = fences(text);

    assert_eq!(result, Ok(()));
}

#[test]
fn it_should_report_file_line_command_and_both_outputs_on_a_mismatch() {
    let result = run_block(&one_block(
        "project",
        "$ omhl get work-items.w1 --format json\n{\"node\":1}\n",
    ));

    let message = result.expect_err("the output differs");

    assert!(
        message.starts_with("synthetic.md:2: $ omhl get work-items.w1 --format json\n  expected exit 0 stdout \"{\\\"node\\\":1}\"")
            && message.contains("obtained exit 0 stdout \"{\\\"node\\\":{"),
        "{message}"
    );
}

#[test]
fn it_should_feed_the_heredoc_to_stdin_and_match_stderr_and_exit_code() {
    assert_runs(
        "project",
        "$ omhl append --format json <<'EOF'\nnot json\nEOF\n2> {\"error\":{\"code\":\"input-invalid-json\",\"details\":[],\"message\":\"input is not valid JSON at line 1 column 2\"}}\n# exit 1\n",
    );
}

fn text_normalizer() -> Normalizer {
    Normalizer::new(Path::new("/nowhere/repo"), Path::new("/nowhere/xdg"))
}

#[test]
fn it_should_match_check_run_text_durations_and_short_snapshot_against_placeholders() {
    let expected = "✔ before-pr · work-items.w1 · recorded (git-commit <snapshot>)\n  ✘ checks.bad · blocking · exit 1 · <s>\n  ✔ checks.ok  passed  <ms>";
    let obtained = "✔ before-pr · work-items.w1 · recorded (git-commit c43356d)\n  ✘ checks.bad · blocking · exit 1 · 2.6s\n  ✔ checks.ok  passed  12ms";

    assert!(text_normalizer().same(expected, obtained));
}

#[test]
fn it_should_keep_the_snapshot_kind_when_normalizing_the_short_id() {
    let expected = "✔ before-commit · work-items.w1 · recorded (git-tree <snapshot>)";
    let obtained = "✔ before-commit · work-items.w1 · recorded (git-commit c43356d)";

    assert!(!text_normalizer().same(expected, obtained));
}

#[test]
fn it_should_match_check_sweep_text_pids_and_counts_against_placeholders() {
    let expected = "✔ check sweep · <n> swept · <n> unreadable\n- sleep · pid <pid> · owner <pid>";
    let obtained = "✔ check sweep · 2 swept · 33 unreadable\n- sleep · pid 4242 · owner 4100";

    assert!(text_normalizer().same(expected, obtained));
}

#[test]
fn it_should_keep_the_process_name_when_normalizing_check_sweep_text() {
    let expected = "- sleep · pid <pid> · owner <pid>";
    let obtained = "- sh · pid 4242 · owner 4100";

    assert!(!text_normalizer().same(expected, obtained));
}

#[test]
fn it_should_keep_counts_that_are_not_swept_or_unreadable() {
    let expected = "✔ hooks install · 4 scripts";
    let obtained = "✔ hooks install · 5 scripts";

    assert!(!text_normalizer().same(expected, obtained));
}
