mod support;

use std::fs;
use std::path::{Path, PathBuf};

use omhl::adapters::text::{
    CommandKey, Entry, Layout, NodeContainer, Outcome, Profile, Region, Scope, Shape, WILDCARD,
    profile_for, render_failure, render_success,
};
use serde_json::{Value, json};
use support::child::{Output, omhl as binary};
use support::ext::ExtTree;
use support::git::git_in;
use support::inproc::{run_inproc, run_inproc_at};
use support::omhl::{decision_data, work_item_data};
use support::project::Project;
use support::query;
use support::run::Run;

// ---------------------------------------------------------------- folhas de um `Value`

#[derive(Debug, Clone, PartialEq)]
enum Scalar {
    Text(String),
    Number(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone)]
struct Leaf {
    path: String,
    region: Region,
    node_type: Option<String>,
    parent: Option<String>,
    key: String,
    scalar: Scalar,
}

/// Percorre o `Value` coletando os escalares com o caminho de chaves; array é transparente.
fn scalars(value: &Value, segments: &mut Vec<String>, out: &mut Vec<(Vec<String>, Scalar)>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                segments.push(key.clone());
                scalars(inner, segments, out);
                segments.pop();
            }
        }
        Value::Array(items) => items.iter().for_each(|inner| scalars(inner, segments, out)),
        Value::String(text) => out.push((segments.clone(), Scalar::Text(text.clone()))),
        Value::Number(number) => out.push((segments.clone(), Scalar::Number(number.to_string()))),
        Value::Bool(flag) => out.push((segments.clone(), Scalar::Bool(*flag))),
        Value::Null => out.push((segments.clone(), Scalar::Null)),
    }
}

/// O pai de um nível: a chave nomeada logo antes da chave da folha.
fn parent_of(segments: &[String]) -> Option<String> {
    segments
        .len()
        .checked_sub(2)
        .map(|index| segments[index].clone())
}

fn plain_leaves(root_key: &str, value: &Value, leaves: &mut Vec<Leaf>) {
    let mut found = Vec::new();
    scalars(value, &mut vec![root_key.to_owned()], &mut found);
    for (segments, scalar) in found {
        let region = if segments.len() == 1 {
            Region::Root
        } else {
            Region::Data
        };
        leaves.push(Leaf {
            path: segments.join("."),
            region,
            node_type: None,
            parent: parent_of(&segments),
            key: segments.last().expect("a path has a key").clone(),
            scalar,
        });
    }
}

fn node_leaves(container: &str, node: &Value, leaves: &mut Vec<Leaf>) {
    let node_type = node["type"].as_str().map(str::to_owned);
    let mut found = Vec::new();
    scalars(node, &mut Vec::new(), &mut found);
    for (segments, scalar) in found {
        let (region, relative) = match segments[0].as_str() {
            "data" if segments.len() > 1 => (Region::Data, &segments[1..]),
            "snapshot" if segments.len() > 1 => (Region::Snapshot, &segments[1..]),
            "author" if segments.len() > 1 => (Region::Author, &segments[1..]),
            _ => (Region::Root, &segments[..]),
        };
        leaves.push(Leaf {
            path: format!("{container}.{}", segments.join(".")),
            region,
            node_type: node_type.clone(),
            parent: parent_of(relative),
            key: relative.last().expect("a path has a key").clone(),
            scalar,
        });
    }
}

/// As folhas do `Value` do desfecho: o erro lê o objeto sob `error`; o contêiner de nós do `Profile` separa os nós.
fn collect_leaves(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<Leaf> {
    let (root, nodes) = match outcome {
        Outcome::Success => (value, profile.nodes),
        Outcome::Failure => (&value["error"], None),
    };
    let container_key = nodes.map(|container| match container {
        NodeContainer::Array(key) | NodeContainer::Object(key) => key,
    });
    let mut leaves = Vec::new();
    for (key, inner) in root.as_object().expect("the value is an object") {
        if container_key != Some(key.as_str()) {
            plain_leaves(key, inner, &mut leaves);
            continue;
        }
        match (nodes, inner) {
            (Some(NodeContainer::Array(_)), Value::Array(items)) => items
                .iter()
                .for_each(|node| node_leaves(&format!("{key}[]"), node, &mut leaves)),
            (Some(NodeContainer::Object(_)), Value::Object(_)) => {
                node_leaves(key, inner, &mut leaves)
            }
            _ => plain_leaves(key, inner, &mut leaves),
        }
    }

    leaves
}

// ---------------------------------------------------------------- folhas distintas e formas

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Number,
    Bool,
}

/// Escalares de valor idêntico formam uma folha só.
struct Group<'a> {
    kind: Kind,
    text: String,
    leaves: Vec<&'a Leaf>,
    /// A forma mais exigente entre as chaves do grupo; `None` quando alguma chave está fora da tabela.
    shape: Option<Shape>,
}

impl Group<'_> {
    fn paths(&self) -> String {
        self.leaves
            .iter()
            .map(|leaf| leaf.path.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Literal, Closed, Hash, Derived, Noise: da forma mais exigente para a mais frouxa.
fn demand(shape: Shape) -> u8 {
    match shape {
        Shape::Literal => 0,
        Shape::Closed => 1,
        Shape::Hash(_) => 2,
        Shape::Derived => 3,
        Shape::Noise => 4,
    }
}

fn entry_of(profile: &Profile, outcome: Outcome, leaf: &Leaf) -> Option<&'static Entry> {
    profile.find(
        outcome,
        leaf.region,
        leaf.node_type.as_deref(),
        leaf.parent.as_deref(),
        &leaf.key,
    )
}

fn groups<'a>(profile: &Profile, outcome: Outcome, leaves: &'a [Leaf]) -> Vec<Group<'a>> {
    let mut groups: Vec<Group<'a>> = Vec::new();
    for leaf in leaves {
        let (kind, text) = match &leaf.scalar {
            Scalar::Text(text) => (Kind::Text, text.clone()),
            Scalar::Number(text) => (Kind::Number, text.clone()),
            Scalar::Bool(flag) => (Kind::Bool, flag.to_string()),
            Scalar::Null => continue,
        };
        let shape = entry_of(profile, outcome, leaf).map(|entry| entry.shape);
        match groups
            .iter_mut()
            .find(|group| group.kind == kind && group.text == text)
        {
            Some(group) => {
                group.leaves.push(leaf);
                group.shape = match (group.shape, shape) {
                    (Some(known), Some(new)) => Some(if demand(new) < demand(known) {
                        new
                    } else {
                        known
                    }),
                    _ => None,
                };
            }
            None => groups.push(Group {
                kind,
                text,
                leaves: vec![leaf],
                shape,
            }),
        }
    }
    // String vazia não é folha de `Literal`: é `Derived`, seja qual for a forma declarada (o caso afirma a ausência do trecho).
    for group in &mut groups {
        if group.kind == Kind::Text && group.text.is_empty() && group.shape.is_some() {
            group.shape = Some(Shape::Derived);
        }
    }

    groups
}

// ---------------------------------------------------------------- tokens

fn is_token_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

/// Onde `needle` ocorre com limite de token à esquerda e, se `bounded_right`, também à direita.
fn occurrences(text: &str, needle: &str, bounded_right: bool) -> Vec<(usize, usize)> {
    text.match_indices(needle)
        .map(|(start, found)| (start, start + found.len()))
        .filter(|(start, end)| {
            let before = text[..*start].chars().next_back();
            let after = text[*end..].chars().next();
            !before.is_some_and(is_token_char)
                && !(bounded_right && after.is_some_and(is_token_char))
        })
        .collect()
}

/// Troca por um espaço cada ocorrência de `needle` como token inteiro.
fn erase_token(text: &str, needle: &str) -> String {
    let mut erased = String::new();
    let mut cursor = 0;
    for (start, end) in occurrences(text, needle, true) {
        erased.push_str(&text[cursor..start]);
        erased.push(' ');
        cursor = end;
    }
    erased.push_str(&text[cursor..]);

    erased
}

// ---------------------------------------------------------------- as checagens do CA-2

/// (4) chave fora da tabela e booleano fora de `Derived`/`Noise`.
fn table_problems(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);
    let mut problems: Vec<String> = leaves
        .iter()
        .filter(|leaf| leaf.scalar != Scalar::Null)
        .filter(|leaf| entry_of(profile, outcome, leaf).is_none())
        .map(|leaf| {
            format!(
                "{}: key not in the profile table (region {:?}, type {:?}, parent {:?}, key {})",
                leaf.path, leaf.region, leaf.node_type, leaf.parent, leaf.key
            )
        })
        .collect();
    for group in groups(profile, outcome, &leaves) {
        let demanding = matches!(
            group.shape,
            Some(Shape::Literal | Shape::Closed | Shape::Hash(_))
        );
        if group.kind == Kind::Bool && demanding {
            problems.push(format!(
                "{}: a boolean admits only Derived or Noise",
                group.paths()
            ));
        }
    }

    problems
}

/// (5) entrada não opcional da tabela do desfecho sem chave no `Value`; a de tipo de nó só vale se o `Value` tem um nó do tipo.
fn required_problems(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);
    // Um `Value` sem nó (`{"node":null}` do `get`, página vazia do `list`) não tem de onde cobrar as chaves de nó.
    let node_profile = profile.nodes.is_some() && outcome == Outcome::Success;
    if node_profile && leaves.iter().all(|leaf| leaf.node_type.is_none()) {
        return Vec::new();
    }

    profile
        .entries(outcome)
        .iter()
        .filter(|entry| !entry.optional && entry.key != WILDCARD)
        .filter(|entry| match entry.scope {
            Scope::Any => true,
            Scope::NodeType(wanted) => leaves
                .iter()
                .any(|leaf| leaf.node_type.as_deref() == Some(wanted)),
        })
        .filter(|entry| {
            !leaves.iter().any(|leaf| {
                leaf.region == entry.region
                    && leaf.key == entry.key
                    && (entry.parent.is_none() || leaf.parent.as_deref() == entry.parent)
            })
        })
        .map(|entry| {
            format!(
                "declared key missing from the value: {:?} {:?} parent {:?} key {}",
                entry.region, entry.scope, entry.parent, entry.key
            )
        })
        .collect()
}

/// (6) pré-condição de fixture: o que a fixture escolhe livremente (`Literal`) tem de ser distinguível no texto.
fn fixture_problems(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);
    let all = groups(profile, outcome, &leaves);
    let mut problems = Vec::new();
    for group in all.iter().filter(|g| g.shape == Some(Shape::Literal)) {
        if group.kind == Kind::Text && group.text.chars().count() < 4 {
            problems.push(format!(
                "{}: a free literal needs at least 4 characters",
                group.paths()
            ));
        }
        if group.text.contains('\n') {
            problems.push(format!("{}: a free literal is one line", group.paths()));
        }
        for other in all.iter().filter(|other| !same(group, other)) {
            if other.shape == Some(Shape::Literal)
                && !occurrences(&other.text, &group.text, true).is_empty()
                && other.text != group.text
            {
                problems.push(format!(
                    "{}: `{}` also occurs as a token inside the free leaf `{}`",
                    group.paths(),
                    group.text,
                    other.text
                ));
            }
            if other.shape == Some(Shape::Closed)
                && other.text != group.text
                && !occurrences(&group.text, &other.text, true).is_empty()
            {
                problems.push(format!(
                    "{}: contains the closed value `{}` as a token",
                    group.paths(),
                    other.text
                ));
            }
        }
    }

    problems
}

fn same(left: &Group<'_>, right: &Group<'_>) -> bool {
    left.kind == right.kind && left.text == right.text
}

/// (4) `Literal` e `Closed` aparecem como token inteiro; `Hash(n)` pelos `n` primeiros caracteres, com limite só à esquerda.
/// As ocorrências de uma folha contêiner (que contém o valor) são apagadas antes da busca.
fn presence_problems(
    profile: &Profile,
    outcome: Outcome,
    value: &Value,
    text: &str,
) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);
    let all = groups(profile, outcome, &leaves);
    let mut problems = Vec::new();
    for group in &all {
        let (needle, bounded_right) = match group.shape {
            Some(Shape::Literal | Shape::Closed) => (group.text.clone(), true),
            Some(Shape::Hash(size)) => (group.text.chars().take(size).collect(), false),
            _ => continue,
        };
        let mut remaining = text.to_owned();
        for container in all
            .iter()
            .filter(|other| !same(group, other) && other.text != needle)
            .filter(|other| !occurrences(&other.text, &needle, bounded_right).is_empty())
        {
            remaining = erase_token(&remaining, &container.text);
        }
        if occurrences(&remaining, &needle, bounded_right).is_empty() {
            problems.push(format!(
                "{}: `{needle}` is not shown as a whole token in the text",
                group.paths()
            ));
        }
    }

    problems
}

// ---------------------------------------------------------------- fixtures e casos

struct Fixture {
    project: Project,
    project_dir: PathBuf,
    /// `Some(stdin)`: o comando roda pelo binário, com a pasta do projeto como cwd (o `check run` precisa de git e
    /// de processo próprio); `None`: roda em processo, com `--project-dir`.
    spawned_with: Option<String>,
}

impl Fixture {
    fn of(project: Project) -> Fixture {
        let project_dir = project.dir().to_path_buf();

        Fixture {
            project,
            project_dir,
            spawned_with: None,
        }
    }

    fn spawned(project: Project, stdin: &str) -> Fixture {
        Fixture {
            spawned_with: Some(stdin.to_owned()),
            ..Fixture::of(project)
        }
    }

    /// O binário com o ambiente do projeto; o `check run` nunca roda no processo do teste (varre os próprios filhos).
    fn spawn(&self, args: &[&str]) -> Output {
        binary(args)
            .cwd(&self.project_dir)
            .xdg(self.project.xdg())
            .stdin(self.spawned_with.as_deref().unwrap_or_default())
            .run()
    }

    fn run(&self, args: &[&str]) -> Run {
        if self.spawned_with.is_none() {
            return run_inproc_at(&self.project_dir, self.project.xdg(), args, "");
        }
        let output = self.spawn(args);

        Run {
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }
}

fn append_decision(project: &Project, id: &str, text: &str) {
    let request = json!({
        "items": [{ "body": {
            "kind": "node", "id": id, "type": "decision",
            "data": decision_data(text, None),
        } }]
    });

    let appended = run_inproc(
        project,
        &["--format", "json", "append"],
        &request.to_string(),
    );

    assert_eq!(appended.stderr, "", "the fixture append failed");
}

/// Três decisões, uma cadeia fechada.
fn chain_of_three() -> Project {
    let project = Project::new("acme");
    append_decision(&project, "decisions.d1", "Chain marker one");
    append_decision(&project, "decisions.d2", "Chain marker two");
    append_decision(&project, "decisions.d3", "Chain marker three");

    project
}

/// Troca um caractere do texto do 2º record no arquivo do banco: o hash gravado deixa de bater.
fn tamper_second_record(project: &Project) {
    let path = project.db_path();
    let mut bytes = fs::read(&path).expect("the database is read");
    let (from, to) = (b"Chain marker two", b"Chain marker twp");
    let at = bytes
        .windows(from.len())
        .position(|window| window == from)
        .expect("the stored text is in the database file");
    bytes[at..at + to.len()].copy_from_slice(to);
    fs::write(&path, bytes).expect("the database is written");
}

fn verify_ok() -> Fixture {
    Fixture::of(chain_of_three())
}

fn verify_broken_chain() -> Fixture {
    let project = chain_of_three();
    tamper_second_record(&project);

    Fixture::of(project)
}

fn verify_no_marker() -> Fixture {
    let project = Project::new("acme");
    fs::remove_file(project.dir().join(".omhl.toml")).expect("the marker is removed");

    Fixture::of(project)
}

fn verify_invalid_project_dir() -> Fixture {
    let mut fixture = Fixture::of(Project::new("acme"));
    fixture.project_dir = fixture.project.dir().join("missing");

    fixture
}

fn verify_ok_facts(value: &Value) -> Vec<String> {
    vec![
        "chain ok".to_owned(),
        format!("{} records", value["total_records"]),
    ]
}

fn verify_broken_facts(value: &Value) -> Vec<String> {
    vec![
        "chain broken".to_owned(),
        format!("{} records", value["total_records"]),
        format!("{} breaks", value["total_breaks"]),
        "Next: ".to_owned(),
    ]
}

fn no_facts(_value: &Value) -> Vec<String> {
    Vec::new()
}

// ---------------------------------------------------------------- fixtures do `check run` (sempre pelo binário)

const WORK_ITEM: &str = "work-items.payroll";
const WORK_BRANCH: &str = "feat/wi-payroll__x";
const CHECK_RUN: &[&str] = &["check", "run", "--event", "before-pr"];
const CHECK_RUN_ON_PUSH: &[&str] = &["check", "run", "--event", "before-pr", "--pre-push"];
/// Um push que só apaga uma ref: não há nada a atestar.
const DELETE_ONLY_PUSH: &str = "(delete) 0000000000000000000000000000000000000000 refs/heads/old bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n";

fn git(dir: &Path, args: &[&str]) {
    let output = git_in(dir, args);

    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn omhl_succeeds(project: &Project, args: &[&str], stdin: &str) {
    let output = binary(args)
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(stdin)
        .run();

    assert_eq!(
        output.code,
        Some(0),
        "omhl {args:?} failed: {}",
        output.stderr
    );
}

/// TOML de um check `before-pr` bloqueante que roda `command` num shell.
fn check_file(concern: &str, variant: &str, command: &str) -> String {
    let run = serde_json::to_string(&["sh", "-c", command]).expect("the argv serializes");

    format!(
        "blocking = true\nhow = \"Guards the payroll.\"\nconcern = \"{concern}\"\nvariant = \"{variant}\"\nrun = {run}\ntimeout_secs = 60\nexpect = [{{ exit = 0, outcome = \"passed\" }}]\n\n[when]\nevent = \"before-pr\"\n"
    )
}

/// Repositório git na branch do work item, com o check no commit base, o work item gravado e o `load` feito.
fn check_repo(slug: &str, check_toml: &str) -> Project {
    let project = Project::new("acme");
    let dir = project.dir();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "Test Author"]);
    git(dir, &["config", "user.email", "author@example.com"]);
    ExtTree::at(dir).check(slug, check_toml);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "base"]);
    git(dir, &["switch", "-q", "-c", WORK_BRANCH]);
    let work_item = json!({
        "items": [{ "body": {
            "kind": "node", "id": WORK_ITEM, "type": "work-item",
            "data": work_item_data("Payroll rollout"),
        } }]
    });
    omhl_succeeds(
        &project,
        &["--format", "json", "append"],
        &work_item.to_string(),
    );
    omhl_succeeds(&project, &["--format", "json", "load"], "");

    project
}

fn check_run_ok() -> Fixture {
    let toml = check_file("style", "format", "exit 0");

    Fixture::spawned(check_repo("payroll-format", &toml), "")
}

/// O comando sai com 7 e escreve nas duas saídas: o check reprova e o `checks-failed` mostra o resumo.
fn check_run_blocked() -> Fixture {
    let toml = check_file(
        "test",
        "suite",
        "echo emitted-on-stdout; echo emitted-on-stderr >&2; exit 7",
    );

    Fixture::spawned(check_repo("payroll-suite", &toml), "")
}

/// O arquivo commitado difere da definição guardada no banco: o check não roda.
fn check_run_stale() -> Fixture {
    let project = check_repo("payroll-gate", &check_file("compile", "build", "exit 0"));
    ExtTree::at(project.dir()).check("payroll-gate", &check_file("compile", "build", "exit 3"));
    git(project.dir(), &["add", "-A"]);
    git(project.dir(), &["commit", "-q", "-m", "edit gate"]);

    Fixture::spawned(project, "")
}

fn check_run_nothing_to_attest() -> Fixture {
    let toml = check_file("style", "format", "exit 0");

    Fixture::spawned(check_repo("payroll-format", &toml), DELETE_ONLY_PUSH)
}

/// HEAD solto contido em duas branches: não há branch do work item para resolver.
fn check_run_head_ambiguous() -> Fixture {
    let toml = check_file("style", "format", "exit 0");
    let project = check_repo("payroll-format", &toml);
    git(project.dir(), &["branch", "payroll-copy"]);
    git(project.dir(), &["switch", "-q", "--detach"]);

    Fixture::spawned(project, "")
}

fn text_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("`{key}` is not a string in {value}"))
}

/// A duração do check reprovado como o texto a mostra: `Nms` abaixo de 1 s, `S.Ts` acima.
fn duration_text(duration_ms: u64) -> String {
    if duration_ms < 1000 {
        return format!("{duration_ms}ms");
    }

    format!("{:.1}s", duration_ms as f64 / 1000.0)
}

fn check_run_ok_facts(value: &Value) -> Vec<String> {
    let result = &value["results"][0];
    let id = text_at(&value["snapshot"], "id");

    vec![
        format!(
            "✔ {} · {} · recorded (git-commit {}",
            text_at(value, "event"),
            text_at(value, "work_item"),
            &id[..7]
        ),
        format!(
            "✔ {}  {}/{}  {}  {}ms",
            text_at(result, "check"),
            text_at(result, "concern"),
            text_at(result, "variant"),
            text_at(result, "outcome"),
            result["duration_ms"]
        ),
        "Nothing blocked. Next: go on with the push.".to_owned(),
    ]
}

fn check_run_ok_absent(_value: &Value) -> Vec<String> {
    ["non-blocking", "timed out", "truncated", "exit "]
        .map(str::to_owned)
        .to_vec()
}

fn check_run_blocked_facts(value: &Value) -> Vec<String> {
    let context = &value["error"]["details"][0]["context"];
    let summary = &value["error"]["context"]["summary"];
    assert_eq!(
        summary["failed"], summary["blocking_failed"],
        "the fixture has no non-blocking failure"
    );

    vec![
        format!(
            "✘ BLOCKED · {} · {} · {} blocking failed, {} passed, {} not run",
            text_at(&value["error"]["context"], "event"),
            text_at(&value["error"]["context"], "work_item"),
            summary["blocking_failed"],
            summary["passed"],
            summary["not_run"]
        ),
        format!(
            "✘ {} · {}/{} · blocking · timeout {}s · exit {} · {}",
            text_at(&value["error"]["details"][0], "path"),
            text_at(context, "concern"),
            text_at(context, "variant"),
            context["timeout_secs"],
            context["exit_code"],
            duration_text(context["duration_ms"].as_u64().expect("a duration"))
        ),
        "command:".to_owned(),
        context["run"]
            .as_array()
            .expect("the run is a list")
            .iter()
            .map(|part| part.as_str().expect("a run part is text"))
            .collect::<Vec<_>>()
            .join(" "),
        format!(
            "how to read the output: {}",
            text_at(&value["error"]["details"][0], "message")
        ),
        "The output may contain secrets".to_owned(),
        "Next: read the output with the command above".to_owned(),
    ]
}

fn check_run_blocked_absent(value: &Value) -> Vec<String> {
    vec![
        text_at(&value["error"], "code").to_owned(),
        text_at(&value["error"], "message").to_owned(),
        "non-blocking".to_owned(),
        "timed out".to_owned(),
        "truncated".to_owned(),
        "kind:".to_owned(),
    ]
}

fn check_run_stale_facts(value: &Value) -> Vec<String> {
    let error = &value["error"];
    let detail = &error["details"][0];

    vec![
        format!(
            "✘ {} · {}",
            text_at(error, "code"),
            text_at(error, "message")
        ),
        format!(
            "- {} at {}",
            text_at(detail, "code"),
            text_at(detail, "path")
        ),
        text_at(detail, "message").to_owned(),
        format!("check: {}", text_at(&detail["context"], "check")),
        format!("hint: {}", text_at(&detail["context"], "hint")),
    ]
}

/// O cabeçalho `BLOCKED` é o único lugar do evento e do work item; `kind` igual ao `code` não ganha linha.
fn check_run_stale_absent(value: &Value) -> Vec<String> {
    let context = &value["error"]["details"][0]["context"];

    vec![
        "BLOCKED".to_owned(),
        text_at(context, "event").to_owned(),
        text_at(context, "work_item").to_owned(),
        "kind:".to_owned(),
    ]
}

fn check_run_nothing_facts(value: &Value) -> Vec<String> {
    vec![format!(
        "✔ {} · nothing to attest in this push; no check ran",
        text_at(value, "event")
    )]
}

fn check_run_nothing_absent(_value: &Value) -> Vec<String> {
    ["recorded", "Nothing blocked", "Next:"]
        .map(str::to_owned)
        .to_vec()
}

fn check_run_ambiguous_facts(value: &Value) -> Vec<String> {
    let error = &value["error"];

    vec![
        format!(
            "✘ {} · {}",
            text_at(error, "code"),
            text_at(error, "message")
        ),
        format!("- {}", text_at(&error["details"][0], "code")),
        format!("hint: {}", text_at(&error["details"][0]["context"], "hint")),
    ]
}

/// `path` vazio some o ` at <path>`; `event` fica fora do texto; `kind` igual ao `code` não ganha linha.
fn check_run_ambiguous_absent(value: &Value) -> Vec<String> {
    let detail = &value["error"]["details"][0];

    vec![
        format!("{} at", text_at(detail, "code")),
        text_at(&detail["context"], "event").to_owned(),
        "kind:".to_owned(),
    ]
}

// ---------------------------------------------------------------- fixtures do `list` e do `get` (em processo)

fn query_state() -> Fixture {
    Fixture::of(query::query_project())
}

fn items_of(value: &Value) -> &Vec<Value> {
    value["items"].as_array().expect("the list has items")
}

/// `N nodes · seq A–B` e as palavras dos fatos `Derived` do nó (`shadowed`, `blocking`).
fn list_facts(value: &Value) -> Vec<String> {
    let items = items_of(value);
    let first = &items[0]["seq"];
    let last = &items[items.len() - 1]["seq"];

    vec![
        format!("{} nodes · seq {first}–{last}", items.len()),
        "shadowed".to_owned(),
        "blocking".to_owned(),
    ]
}

fn list_page_facts(value: &Value) -> Vec<String> {
    let items = items_of(value);
    let last = &items[items.len() - 1]["seq"];

    vec![
        format!("{} nodes · seq 1–{last}", items.len()),
        format!(
            "Next: omhl list --after-seq {} with the same filters",
            value["next_after_seq"]
        ),
    ]
}

fn list_last_page_absent(_value: &Value) -> Vec<String> {
    vec!["Next:".to_owned()]
}

/// O cabeçalho do `get` e os fatos `Derived` do nó; `data` busca cada fato pelo ponteiro JSON em `node.data`.
fn get_facts(value: &Value, data: &[(&str, &str)]) -> Vec<String> {
    let node = &value["node"];
    let mut facts = vec![
        format!(
            "✔ get · {} · {} · seq {}",
            text_at(node, "id"),
            text_at(node, "type"),
            node["seq"]
        ),
        format!("shadowed: {}", node["shadowed"]),
    ];
    for (label, pointer) in data {
        let found = node["data"]
            .pointer(pointer)
            .unwrap_or_else(|| panic!("`{pointer}` is not in {node}"));
        facts.push(format!("{label}: {found}"));
    }

    facts
}

fn get_header_facts(value: &Value) -> Vec<String> {
    get_facts(value, &[])
}

fn get_attestation_facts(value: &Value) -> Vec<String> {
    get_facts(
        value,
        &[
            ("ok", "/ok"),
            ("exit_code", "/exit_code"),
            ("duration_ms", "/duration_ms"),
            ("timed_out", "/timed_out"),
            ("truncated", "/truncated"),
        ],
    )
}

fn get_closed_work_item_facts(value: &Value) -> Vec<String> {
    get_facts(value, &[("gate.passed", "/gate/passed")])
}

fn get_check_facts(value: &Value) -> Vec<String> {
    get_facts(
        value,
        &[
            ("blocking", "/blocking"),
            ("expect[0].exit", "/expect/0/exit"),
        ],
    )
}

fn get_blocking_facts(value: &Value) -> Vec<String> {
    get_facts(value, &[("blocking", "/blocking")])
}

fn get_missing_node_facts(_value: &Value) -> Vec<String> {
    vec!["⚠ get · node not found".to_owned()]
}

/// O erro genérico do app: o cabeçalho `código · mensagem` e a linha do detalhe, que só repete o código se difere.
fn generic_error_facts(value: &Value) -> Vec<String> {
    let error = &value["error"];
    let detail = &error["details"][0];
    let detail_line = match detail["code"] == error["code"] {
        true => format!("- {}", text_at(detail, "path")),
        false => format!(
            "- {} · {}",
            text_at(detail, "path"),
            text_at(detail, "code")
        ),
    };

    vec![
        format!(
            "✘ {} · {}",
            text_at(error, "code"),
            text_at(error, "message")
        ),
        detail_line,
    ]
}

/// Um estado de fixture de um comando: cada onda das etapas 3 e 4 acrescenta os seus casos.
struct Case {
    name: &'static str,
    key: CommandKey,
    args: &'static [&'static str],
    outcome: Outcome,
    /// O `error.code` literal esperado no `--format json` (só nos casos de erro).
    error_code: Option<&'static str>,
    fixture: fn() -> Fixture,
    /// Os fatos `Derived` que o texto mostra nos dois layouts, tirados do `Value`.
    derived: fn(&Value) -> Vec<String>,
    /// O que o texto omite nos dois layouts (o cabeçalho que substitui `code · message`, a linha `kind:`).
    absent: fn(&Value) -> Vec<String>,
}

const CASES: [Case; 24] = [
    Case {
        name: "verify ok",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: verify_ok,
        derived: verify_ok_facts,
        absent: no_facts,
    },
    Case {
        name: "verify broken chain",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: verify_broken_chain,
        derived: verify_broken_facts,
        absent: no_facts,
    },
    Case {
        name: "verify invalid project dir",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Failure,
        error_code: Some("invalid-project-dir"),
        fixture: verify_invalid_project_dir,
        derived: no_facts,
        absent: no_facts,
    },
    Case {
        name: "verify no marker",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Failure,
        error_code: Some("no-marker"),
        fixture: verify_no_marker,
        derived: no_facts,
        absent: no_facts,
    },
    Case {
        name: "check run ok",
        key: CommandKey::CheckRun,
        args: CHECK_RUN,
        outcome: Outcome::Success,
        error_code: None,
        fixture: check_run_ok,
        derived: check_run_ok_facts,
        absent: check_run_ok_absent,
    },
    Case {
        name: "check run checks-failed",
        key: CommandKey::CheckRun,
        args: CHECK_RUN,
        outcome: Outcome::Failure,
        error_code: Some("checks-failed"),
        fixture: check_run_blocked,
        derived: check_run_blocked_facts,
        absent: check_run_blocked_absent,
    },
    Case {
        name: "check run check-not-run",
        key: CommandKey::CheckRun,
        args: CHECK_RUN,
        outcome: Outcome::Failure,
        error_code: Some("check-not-run"),
        fixture: check_run_stale,
        derived: check_run_stale_facts,
        absent: check_run_stale_absent,
    },
    Case {
        name: "check run pre-push with nothing to attest",
        key: CommandKey::CheckRun,
        args: CHECK_RUN_ON_PUSH,
        outcome: Outcome::Success,
        error_code: None,
        fixture: check_run_nothing_to_attest,
        derived: check_run_nothing_facts,
        absent: check_run_nothing_absent,
    },
    Case {
        name: "check run head-ambiguous",
        key: CommandKey::CheckRun,
        args: CHECK_RUN,
        outcome: Outcome::Failure,
        error_code: Some("head-ambiguous"),
        fixture: check_run_head_ambiguous,
        derived: check_run_ambiguous_facts,
        absent: check_run_ambiguous_absent,
    },
    Case {
        name: "list with the nine types and a shadowed check",
        key: CommandKey::List,
        args: &["list"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: list_facts,
        absent: list_last_page_absent,
    },
    Case {
        name: "list page with a next page",
        key: CommandKey::List,
        args: &["list", "--limit", "3"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: list_page_facts,
        absent: no_facts,
    },
    Case {
        name: "get decision",
        key: CommandKey::Get,
        args: &["get", query::DECISION],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get attestation",
        key: CommandKey::Get,
        args: &["get", query::ATTESTATION],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_attestation_facts,
        absent: no_facts,
    },
    Case {
        name: "get premise",
        key: CommandKey::Get,
        args: &["get", query::PREMISE],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get closed work item with gate",
        key: CommandKey::Get,
        args: &["get", query::CLOSED_WORK_ITEM],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_closed_work_item_facts,
        absent: no_facts,
    },
    Case {
        name: "get project check",
        key: CommandKey::Get,
        args: &["get", query::CHECK],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_check_facts,
        absent: no_facts,
    },
    Case {
        name: "get shadowed check",
        key: CommandKey::Get,
        args: &["get", query::SHADOWED_CHECK],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_blocking_facts,
        absent: no_facts,
    },
    Case {
        name: "get divergence",
        key: CommandKey::Get,
        args: &["get", query::DIVERGENCE],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get pull request",
        key: CommandKey::Get,
        args: &["get", query::PULL_REQUEST],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get gap",
        key: CommandKey::Get,
        args: &["get", query::GAP],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get guideline",
        key: CommandKey::Get,
        args: &["get", query::GUIDELINE],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_header_facts,
        absent: no_facts,
    },
    Case {
        name: "get missing node",
        key: CommandKey::Get,
        args: &["get", "checks.absent-probe"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: query_state,
        derived: get_missing_node_facts,
        absent: no_facts,
    },
    Case {
        name: "get invalid id",
        key: CommandKey::Get,
        args: &["get", "Bad_Id"],
        outcome: Outcome::Failure,
        error_code: Some("INVALID_INPUT"),
        fixture: query_state,
        derived: generic_error_facts,
        absent: no_facts,
    },
    Case {
        name: "list limit zero",
        key: CommandKey::List,
        args: &["list", "--limit", "0"],
        outcome: Outcome::Failure,
        error_code: Some("invalid-arguments"),
        fixture: query_state,
        derived: generic_error_facts,
        absent: no_facts,
    },
];

fn command(case: &Case, format: &str) -> Vec<String> {
    ["--format", format]
        .into_iter()
        .chain(case.args.iter().copied())
        .map(str::to_owned)
        .collect()
}

fn run_case(case: &Case, fixture: &Fixture, format: &str) -> Run {
    let args = command(case, format);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    fixture.run(&args)
}

/// O `Value` do comando em `--format json`: o stdout do sucesso ou o `{"error": ...}` do stderr.
fn json_of(case: &Case, fixture: &Fixture) -> Value {
    let run = run_case(case, fixture, "json");
    let (stream, other) = match case.outcome {
        Outcome::Success => (run.stdout, run.stderr),
        Outcome::Failure => (run.stderr, run.stdout),
    };

    assert_eq!(other, "", "{}: the other stream is not empty", case.name);
    serde_json::from_str(stream.trim_end())
        .unwrap_or_else(|error| panic!("{}: the output is not JSON ({error}): {stream}", case.name))
}

fn text_of(case: &Case, value: &Value, layout: Layout) -> String {
    match case.outcome {
        Outcome::Success => render_success(case.key, value, layout),
        Outcome::Failure => render_failure(case.key, value, layout),
    }
}

fn layout_name(layout: Layout) -> &'static str {
    match layout {
        Layout::Agent => "agent",
        Layout::Human => "human",
    }
}

/// Roda a checagem em todos os casos e reprova uma vez, com os problemas de todos: um caso vermelho não esconde os outros.
fn assert_cases_clean(check: impl Fn(&Case, &Value) -> Vec<String>) {
    let problems: Vec<String> = CASES
        .iter()
        .flat_map(|case| {
            let value = json_of(case, &(case.fixture)());

            check(case, &value)
                .into_iter()
                .map(|problem| format!("{}: {problem}", case.name))
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(problems.is_empty(), "{problems:#?}");
}

// ---------------------------------------------------------------- testes de cobertura das folhas (CA-2)

#[test]
fn it_should_declare_every_scalar_of_each_real_value_in_the_profile_table() {
    assert_cases_clean(|case, value| table_problems(profile_for(case.key), case.outcome, value));
}

#[test]
fn it_should_find_every_required_profile_key_in_each_real_value() {
    assert_cases_clean(|case, value| required_problems(profile_for(case.key), case.outcome, value));
}

#[test]
fn it_should_build_each_fixture_so_that_its_free_literals_are_distinguishable() {
    assert_cases_clean(|case, value| fixture_problems(profile_for(case.key), case.outcome, value));
}

fn assert_leaves_in_text(layout: Layout) {
    assert_cases_clean(|case, value| {
        let text = text_of(case, value, layout);

        presence_problems(profile_for(case.key), case.outcome, value, &text)
    });
}

#[test]
fn it_should_show_every_leaf_of_the_real_value_in_the_agent_text() {
    assert_leaves_in_text(Layout::Agent);
}

#[test]
fn it_should_show_every_leaf_of_the_real_value_in_the_human_text() {
    assert_leaves_in_text(Layout::Human);
}

/// O `human` alinha os rótulos com espaços extras: os fatos se comparam com cada sequência de espaços reduzida a um.
fn squeeze(text: &str) -> String {
    text.split(' ')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn assert_derived_facts_in_text(layout: Layout) {
    assert_cases_clean(|case, value| {
        let text = squeeze(&text_of(case, value, layout));

        (case.derived)(value)
            .into_iter()
            .filter(|fact| !text.contains(&squeeze(fact)))
            .map(|fact| format!("`{fact}` missing from:\n{text}"))
            .collect()
    });
}

#[test]
fn it_should_show_every_derived_fact_in_the_agent_text() {
    assert_derived_facts_in_text(Layout::Agent);
}

#[test]
fn it_should_show_every_derived_fact_in_the_human_text() {
    assert_derived_facts_in_text(Layout::Human);
}

fn assert_omitted_facts_absent_from_text(layout: Layout) {
    assert_cases_clean(|case, value| {
        let text = squeeze(&text_of(case, value, layout));

        (case.absent)(value)
            .into_iter()
            .filter(|fact| text.contains(&squeeze(fact)))
            .map(|fact| format!("`{fact}` must not be in:\n{text}"))
            .collect()
    });
}

#[test]
fn it_should_omit_the_facts_the_agent_text_replaces_or_hides() {
    assert_omitted_facts_absent_from_text(Layout::Agent);
}

#[test]
fn it_should_omit_the_facts_the_human_text_replaces_or_hides() {
    assert_omitted_facts_absent_from_text(Layout::Human);
}

#[test]
fn it_should_not_suggest_a_next_step_when_the_chain_is_ok() {
    let case = &CASES[0];
    let value = json_of(case, &(case.fixture)());

    let text = text_of(case, &value, Layout::Agent);

    assert!(!text.contains("Next:"), "{text}");
}

// ---------------------------------------------------------------- costura comando <-> chave (CA-2 (7))

/// Troca por `<dur>` cada duração (`12ms`, `2.6s`, `60s`): o comando real leva um tempo diferente a cada execução.
fn mask_durations(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut masked = String::new();
    let mut index = 0;
    while index < chars.len() {
        let starts_number =
            chars[index].is_ascii_digit() && (index == 0 || !is_token_char(chars[index - 1]));
        if starts_number {
            let digits_end = (index..chars.len())
                .find(|&at| !(chars[at].is_ascii_digit() || chars[at] == '.'))
                .unwrap_or(chars.len());
            let unit_len = if chars[digits_end..].starts_with(&['m', 's']) {
                2
            } else {
                usize::from(chars.get(digits_end) == Some(&'s'))
            };
            let end = digits_end + unit_len;
            if unit_len > 0 && !chars.get(end).is_some_and(|&next| is_token_char(next)) {
                masked.push_str("<dur>");
                index = end;
                continue;
            }
        }
        masked.push(chars[index]);
        index += 1;
    }

    masked
}

fn assert_command_prints_the_renderer_output(layout: Layout) {
    for case in &CASES {
        let fixture = (case.fixture)();
        let value = json_of(case, &fixture);
        let expected = format!("{}\n", text_of(case, &value, layout));

        let run = run_case(case, &fixture, layout_name(layout));

        let (printed, silent) = match case.outcome {
            Outcome::Success => (run.stdout, run.stderr),
            Outcome::Failure => (run.stderr, run.stdout),
        };
        assert_eq!(
            mask_durations(&printed),
            mask_durations(&expected),
            "{}",
            case.name
        );
        assert_eq!(silent, "", "{}", case.name);
    }
}

#[test]
fn it_should_print_the_agent_render_of_the_json_value_for_each_command() {
    assert_command_prints_the_renderer_output(Layout::Agent);
}

#[test]
fn it_should_print_the_human_render_of_the_json_value_for_each_command() {
    assert_command_prints_the_renderer_output(Layout::Human);
}

// ---------------------------------------------------------------- `error.code` congelado (CA-7 v)

#[test]
fn it_should_answer_the_expected_error_code_in_the_json_of_each_error_case() {
    for case in CASES.iter().filter(|case| case.outcome == Outcome::Failure) {
        let value = json_of(case, &(case.fixture)());

        assert_eq!(
            value["error"]["code"],
            json!(case.error_code.expect("an error case names its code")),
            "{}",
            case.name
        );
    }
}

// ---------------------------------------------------------------- as checagens também reprovam

/// A tabela de teste: o `const` interno dá a `'static` que o `Profile` pede, sem vazar memória.
macro_rules! profile {
    ($($entry:expr),* $(,)?) => {{
        const ENTRIES: &[Entry] = &[$($entry),*];

        Profile {
            label: "test",
            nodes: None,
            success: ENTRIES,
            failure: &[],
        }
    }};
}

const fn root(key: &'static str, shape: Shape) -> Entry {
    Entry::new(Region::Root, key, shape)
}

#[test]
fn it_should_reject_a_key_missing_from_the_table_naming_its_path() {
    let profile = profile!(root("name", Shape::Literal));
    let value = json!({ "name": "alpha", "made": "beta" });

    let problems = table_problems(&profile, Outcome::Success, &value);

    assert_eq!(problems.len(), 1);
    assert!(problems[0].starts_with("made:"), "{problems:?}");
}

#[test]
fn it_should_reject_a_boolean_declared_as_literal() {
    let profile = profile!(root("ok", Shape::Literal));

    let problems = table_problems(&profile, Outcome::Success, &json!({ "ok": true }));

    assert_eq!(problems.len(), 1);
}

#[test]
fn it_should_accept_a_boolean_declared_as_derived() {
    let profile = profile!(root("ok", Shape::Derived));

    let problems = table_problems(&profile, Outcome::Success, &json!({ "ok": true }));

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_treat_an_empty_string_under_a_literal_key_as_derived_in_the_table_check() {
    let profile = profile!(root("path", Shape::Literal));

    let problems = table_problems(&profile, Outcome::Success, &json!({ "path": "" }));

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_not_apply_the_literal_minimum_to_an_empty_string() {
    let profile = profile!(root("path", Shape::Literal));

    let problems = fixture_problems(&profile, Outcome::Success, &json!({ "path": "" }));

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_not_require_an_empty_string_to_be_shown_in_the_text() {
    let profile = profile!(root("path", Shape::Literal));

    let problems = presence_problems(
        &profile,
        Outcome::Success,
        &json!({ "path": "" }),
        "nothing here",
    );

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_still_reject_an_empty_string_whose_key_is_missing_from_the_table() {
    let profile = profile!(root("name", Shape::Literal));

    let problems = table_problems(&profile, Outcome::Success, &json!({ "path": "" }));

    assert_eq!(problems.len(), 1);
}

#[test]
fn it_should_reject_a_declared_required_key_absent_from_the_value() {
    let profile = profile!(root("created", Shape::Literal));

    let problems = required_problems(&profile, Outcome::Success, &json!({ "made": "alpha" }));

    assert_eq!(problems.len(), 1);
    assert!(problems[0].ends_with("key created"), "{problems:?}");
}

#[test]
fn it_should_not_require_an_optional_key() {
    let profile = profile!(root("created", Shape::Literal).optional());

    let problems = required_problems(&profile, Outcome::Success, &json!({ "made": "alpha" }));

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_not_require_a_node_type_entry_when_the_value_has_no_such_node() {
    let mut profile = profile!(Entry::new(Region::Data, "text", Shape::Literal).scoped("premise"));
    profile.nodes = Some(NodeContainer::Object("node"));
    let value = json!({ "node": { "type": "decision", "id": "decisions.x", "data": {} } });

    let problems = required_problems(&profile, Outcome::Success, &value);

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_reject_a_free_literal_string_shorter_than_four_characters() {
    let profile = profile!(root("name", Shape::Literal));

    let problems = fixture_problems(&profile, Outcome::Success, &json!({ "name": "abc" }));

    assert_eq!(problems.len(), 1);
}

#[test]
fn it_should_reject_a_free_literal_that_occurs_as_a_token_inside_another_free_leaf() {
    let profile = profile!(root("long", Shape::Literal), root("short", Shape::Literal));
    let value = json!({ "long": "alpha beta", "short": "alpha" });

    let problems = fixture_problems(&profile, Outcome::Success, &value);

    assert_eq!(problems.len(), 1);
}

#[test]
fn it_should_reject_a_free_literal_that_contains_a_closed_value_as_a_token() {
    let profile = profile!(root("title", Shape::Literal), root("kind", Shape::Closed));
    let value = json!({ "title": "plan draft", "kind": "draft" });

    let problems = fixture_problems(&profile, Outcome::Success, &value);

    assert_eq!(problems.len(), 1);
}

#[test]
fn it_should_reject_a_literal_found_only_inside_a_longer_token() {
    let profile = profile!(root("name", Shape::Literal));

    let problems = presence_problems(
        &profile,
        Outcome::Success,
        &json!({ "name": "check" }),
        "checks.x",
    );

    assert_eq!(problems.len(), 1);
}

#[test]
fn it_should_accept_a_literal_shown_as_a_whole_token() {
    let profile = profile!(root("name", Shape::Literal));

    let problems = presence_problems(
        &profile,
        Outcome::Success,
        &json!({ "name": "check" }),
        "✔ run · check · done",
    );

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_not_accept_a_number_found_only_inside_a_longer_number() {
    let profile = profile!(root("seq", Shape::Literal));

    let problems = presence_problems(&profile, Outcome::Success, &json!({ "seq": 7 }), "seq 17");

    assert_eq!(problems.len(), 1);
}

#[test]
fn it_should_accept_a_hash_prefix_that_continues_to_the_right() {
    let profile = profile!(root("hash", Shape::Hash(8)));

    let problems = presence_problems(
        &profile,
        Outcome::Success,
        &json!({ "hash": "abcdef1234567890" }),
        "snapshot abcdef1234",
    );

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_reject_a_hash_prefix_glued_to_a_token_on_the_left() {
    let profile = profile!(root("hash", Shape::Hash(8)));

    let problems = presence_problems(
        &profile,
        Outcome::Success,
        &json!({ "hash": "abcdef1234567890" }),
        "snapshot xabcdef12",
    );

    assert_eq!(problems.len(), 1);
}

#[test]
fn it_should_not_accept_a_value_found_only_inside_its_container_leaf() {
    let profile = profile!(root("id", Shape::Closed), root("name", Shape::Literal));
    let value = json!({ "id": "alpha.beta", "name": "alpha" });

    let problems = presence_problems(&profile, Outcome::Success, &value, "alpha.beta");

    assert_eq!(problems.len(), 1);
    assert!(problems[0].starts_with("name:"), "{problems:?}");
}

#[test]
fn it_should_accept_a_value_shown_inside_and_outside_its_container_leaf() {
    let profile = profile!(root("id", Shape::Closed), root("name", Shape::Literal));
    let value = json!({ "id": "alpha.beta", "name": "alpha" });

    let problems = presence_problems(&profile, Outcome::Success, &value, "alpha.beta · alpha");

    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn it_should_require_presence_when_any_key_sharing_a_value_is_literal() {
    let profile = profile!(root("one", Shape::Noise), root("two", Shape::Literal));
    let value = json!({ "one": "gamma-delta", "two": "gamma-delta" });

    let problems = presence_problems(&profile, Outcome::Success, &value, "nothing here");

    assert_eq!(problems.len(), 1);
}

// ---------------------------------------------------------------- CA-1: o `verify` sem `--format`, com `human` e com `json`

fn binary_verify(fixture: &Fixture, format: Option<&str>) -> Output {
    let dir = fixture
        .project_dir
        .to_str()
        .expect("the project directory is UTF-8");
    let mut args = vec!["--project-dir", dir];
    if let Some(format) = format {
        args.extend(["--format", format]);
    }
    args.push("verify");

    binary(&args).xdg(fixture.project.xdg()).run()
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

fn assert_text_with_symbol(text: &str, symbols: &[&str]) {
    assert!(serde_json::from_str::<Value>(text).is_err(), "{text}");
    assert!(
        symbols
            .iter()
            .any(|symbol| first_line(text).starts_with(symbol)),
        "{text}"
    );
}

#[test]
fn it_should_print_agent_text_for_verify_success_when_no_format_is_given() {
    let output = binary_verify(&verify_ok(), None);

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_agent_text_for_verify_error_when_no_format_is_given() {
    let output = binary_verify(&verify_no_marker(), None);

    assert_eq!(output.code, Some(1));
    assert_text_with_symbol(&output.stderr, &["✘"]);
}

#[test]
fn it_should_print_human_text_for_verify_success() {
    let output = binary_verify(&verify_ok(), Some("human"));

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_human_text_for_verify_error() {
    let output = binary_verify(&verify_no_marker(), Some("human"));

    assert_eq!(output.code, Some(1));
    assert_text_with_symbol(&output.stderr, &["✘"]);
}

#[test]
fn it_should_print_a_single_json_line_for_verify_success_with_format_json() {
    let output = binary_verify(&verify_ok(), Some("json"));

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stdout.trim_end()).expect("the line is JSON")["ok"],
        json!(true)
    );
}

#[test]
fn it_should_print_a_single_json_line_for_verify_error_with_format_json() {
    let output = binary_verify(&verify_no_marker(), Some("json"));

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stderr.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stderr.trim_end()).expect("the line is JSON")["error"]
            ["code"],
        json!("no-marker")
    );
}

#[test]
fn it_should_exit_zero_with_a_broken_chain_symbol_when_the_verify_chain_is_broken() {
    let output = binary_verify(&verify_broken_chain(), None);

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✘"]);
}

// ---------------------------------------------------------------- CA-1: o `check run` sem `--format`, com `human` e com `json`

fn binary_check_run(fixture: &Fixture, format: Option<&str>) -> Output {
    let mut args = Vec::new();
    if let Some(format) = format {
        args.extend(["--format", format]);
    }
    args.extend(CHECK_RUN);

    fixture.spawn(&args)
}

#[test]
fn it_should_print_agent_text_for_check_run_success_when_no_format_is_given() {
    let output = binary_check_run(&check_run_ok(), None);

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_agent_text_for_check_run_error_when_no_format_is_given() {
    let output = binary_check_run(&check_run_blocked(), None);

    assert_eq!(output.code, Some(1));
    assert_text_with_symbol(&output.stderr, &["✘"]);
    assert_eq!(output.stdout, "");
}

#[test]
fn it_should_print_agent_text_for_a_check_run_with_nothing_to_attest_when_no_format_is_given() {
    let fixture = check_run_nothing_to_attest();

    let output = fixture.spawn(CHECK_RUN_ON_PUSH);

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_human_text_for_check_run_success() {
    let output = binary_check_run(&check_run_ok(), Some("human"));

    assert_eq!(output.code, Some(0));
    assert_text_with_symbol(&output.stdout, &["✔"]);
}

#[test]
fn it_should_print_human_text_for_check_run_error() {
    let output = binary_check_run(&check_run_blocked(), Some("human"));

    assert_eq!(output.code, Some(1));
    assert_text_with_symbol(&output.stderr, &["✘"]);
    assert_eq!(output.stdout, "");
}

#[test]
fn it_should_print_a_single_json_line_for_check_run_success_with_format_json() {
    let output = binary_check_run(&check_run_ok(), Some("json"));

    assert_eq!(output.code, Some(0));
    assert_eq!(output.stdout.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stdout.trim_end()).expect("the line is JSON")["event"],
        json!("before-pr")
    );
}

#[test]
fn it_should_print_a_single_json_line_for_check_run_error_with_format_json() {
    let output = binary_check_run(&check_run_blocked(), Some("json"));

    assert_eq!(output.code, Some(1));
    assert_eq!(output.stderr.trim_end().lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(output.stderr.trim_end()).expect("the line is JSON")["error"]
            ["code"],
        json!("checks-failed")
    );
}

#[test]
fn it_should_mask_every_duration_and_keep_the_other_numbers() {
    let masked = mask_durations("✔ checks.x2  passed  12ms · timeout 60s · took 2.6s · exit 7");

    assert_eq!(
        masked,
        "✔ checks.x2  passed  <dur> · timeout <dur> · took <dur> · exit 7"
    );
}
