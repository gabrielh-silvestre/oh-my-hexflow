mod support;

use std::fs;
use std::path::PathBuf;

use omhl::adapters::text::{
    CommandKey, Entry, Layout, NodeContainer, Outcome, Profile, Region, Scope, Shape, WILDCARD,
    profile_for, render_failure, render_success,
};
use serde_json::{Value, json};
use support::child::{Output, omhl as binary};
use support::inproc::{run_inproc, run_inproc_at};
use support::omhl::decision_data;
use support::project::Project;
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

/// (4) chave fora da tabela, booleano fora de `Derived`/`Noise` e string vazia fora de `Derived`.
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
        if group.text.is_empty() && demanding {
            problems.push(format!(
                "{}: an empty string is Derived, never a literal",
                group.paths()
            ));
        }
    }

    problems
}

/// (5) entrada não opcional da tabela do desfecho sem chave no `Value`; a de tipo de nó só vale se o `Value` tem um nó do tipo.
fn required_problems(profile: &Profile, outcome: Outcome, value: &Value) -> Vec<String> {
    let leaves = collect_leaves(profile, outcome, value);

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
}

impl Fixture {
    fn of(project: Project) -> Fixture {
        let project_dir = project.dir().to_path_buf();

        Fixture {
            project,
            project_dir,
        }
    }

    fn run(&self, args: &[&str]) -> Run {
        run_inproc_at(&self.project_dir, self.project.xdg(), args, "")
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
}

const CASES: [Case; 4] = [
    Case {
        name: "verify ok",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: verify_ok,
        derived: verify_ok_facts,
    },
    Case {
        name: "verify broken chain",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Success,
        error_code: None,
        fixture: verify_broken_chain,
        derived: verify_broken_facts,
    },
    Case {
        name: "verify invalid project dir",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Failure,
        error_code: Some("invalid-project-dir"),
        fixture: verify_invalid_project_dir,
        derived: no_facts,
    },
    Case {
        name: "verify no marker",
        key: CommandKey::Verify,
        args: &["verify"],
        outcome: Outcome::Failure,
        error_code: Some("no-marker"),
        fixture: verify_no_marker,
        derived: no_facts,
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

fn assert_no_problems(case: &Case, problems: &[String]) {
    assert!(problems.is_empty(), "{}: {problems:#?}", case.name);
}

// ---------------------------------------------------------------- testes de cobertura das folhas (CA-2)

#[test]
fn it_should_declare_every_scalar_of_each_real_value_in_the_profile_table() {
    for case in &CASES {
        let value = json_of(case, &(case.fixture)());

        assert_no_problems(
            case,
            &table_problems(profile_for(case.key), case.outcome, &value),
        );
    }
}

#[test]
fn it_should_find_every_required_profile_key_in_each_real_value() {
    for case in &CASES {
        let value = json_of(case, &(case.fixture)());

        assert_no_problems(
            case,
            &required_problems(profile_for(case.key), case.outcome, &value),
        );
    }
}

#[test]
fn it_should_build_each_fixture_so_that_its_free_literals_are_distinguishable() {
    for case in &CASES {
        let value = json_of(case, &(case.fixture)());

        assert_no_problems(
            case,
            &fixture_problems(profile_for(case.key), case.outcome, &value),
        );
    }
}

fn assert_leaves_in_text(layout: Layout) {
    for case in &CASES {
        let value = json_of(case, &(case.fixture)());
        let text = text_of(case, &value, layout);

        assert_no_problems(
            case,
            &presence_problems(profile_for(case.key), case.outcome, &value, &text),
        );
    }
}

#[test]
fn it_should_show_every_leaf_of_the_real_value_in_the_agent_text() {
    assert_leaves_in_text(Layout::Agent);
}

#[test]
fn it_should_show_every_leaf_of_the_real_value_in_the_human_text() {
    assert_leaves_in_text(Layout::Human);
}

fn assert_derived_facts_in_text(layout: Layout) {
    for case in &CASES {
        let value = json_of(case, &(case.fixture)());
        let text = text_of(case, &value, layout);

        for fact in (case.derived)(&value) {
            assert!(
                text.contains(&fact),
                "{}: `{fact}` missing from:\n{text}",
                case.name
            );
        }
    }
}

#[test]
fn it_should_show_every_derived_fact_in_the_agent_text() {
    assert_derived_facts_in_text(Layout::Agent);
}

#[test]
fn it_should_show_every_derived_fact_in_the_human_text() {
    assert_derived_facts_in_text(Layout::Human);
}

#[test]
fn it_should_not_suggest_a_next_step_when_the_chain_is_ok() {
    let case = &CASES[0];
    let value = json_of(case, &(case.fixture)());

    let text = text_of(case, &value, Layout::Agent);

    assert!(!text.contains("Next:"), "{text}");
}

// ---------------------------------------------------------------- costura comando <-> chave (CA-2 (7))

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
        assert_eq!(printed, expected, "{}", case.name);
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
fn it_should_reject_an_empty_string_declared_as_literal() {
    let profile = profile!(root("path", Shape::Literal));

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
