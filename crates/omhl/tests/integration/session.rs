use crate::support;

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::process::ExitCode;

use hexlog::{Author, RelationQuery};
use omhl::adapters::text::{CommandKey, profile_for};
use omhl::app::App;
use omhl::app::action::{Action, Caller};
use omhl::app::output;
use omhl::app::retry::RetryPolicy;
use omhl::error::{OmhlError, RuleCode};
use omhl::facade::{OUTSIDE_CLAUDE_CODE, Omhl, OmhlDeps, SessionSource};
use omhl::probe::{ProbeOutcome, TreeSource};
use omhl::project::layout::{OMHL_DIR, PROJECTION_DIR};
use serde_json::{Value, json};
use support::ext::ExtTree;
use support::omhl::{
    PR_7, acme, decision_data, node_item, premise_data, request, seed_pull_request, work_item_data,
};
use support::pr_gate::{all_native_enabled, attest_required, mechanical_check_data, snapshots};
use support::project::Project;
use support::raw::RawHarness;
use support::run::{Run, run_omhl_with_session};
use support::session::{Stamp, chain_head, raw_session, stamps};
use support::{FakeProbe, author, loader_author, rty, sid, snap};

const SESSION: &str = "5b1c0f3e-77a2-4c1e-9d0b-2f6a8e41c9aa";
const OTHER_SESSION: &str = "0e4d2a96-3c58-4f0b-8a17-d6b9c1e75f02";
const WORK_ITEM: &str = "work-items.w1";
const PREMISE: &str = "premises.p1";
const DECISION: &str = "decisions.d1";
const EDGE: &str = "premises.p1>decisions.d1";
const CHECK: &str = "checks.fmt";
const GUIDELINE: &str = "guidelines.be-brief";

fn tree(fill: char) -> hexlog::Snapshot {
    hexlog::Snapshot::new(hexlog::SnapshotKind::GitTree, &fill.to_string().repeat(40))
        .expect("valid snapshot")
}

fn node_wire(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "body": { "kind": "node", "id": id, "type": type_name, "data": data } })
}

fn relation_wire(from: &str, to: &str) -> Value {
    json!({ "body": { "kind": "relation", "relation_type": "supports", "from": from, "to": to } })
}

fn append_items(
    project: &Project,
    claude_code: bool,
    session: Option<OsString>,
    items: &[Value],
) -> (ExitCode, Run) {
    let request = json!({ "items": items });

    run_omhl_with_session(
        project,
        &["--format", "json", "append"],
        &request.to_string(),
        claude_code,
        None,
        session,
    )
}

fn append_in_session(project: &Project, session: &str, items: &[Value]) -> Run {
    append_items(project, false, Some(OsString::from(session)), items).1
}

fn premise_wire(text: &str) -> Value {
    node_wire(PREMISE, "premise", &premise_data(text, None))
}

fn decision_wire() -> Value {
    node_wire(DECISION, "decision", &decision_data("Use WAL", None))
}

fn record_id(run: &Run, index: usize) -> String {
    let body: Value = serde_json::from_str(run.stdout.trim_end()).expect("the output is JSON");

    body["records"][index]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the append was refused: {}", run.stderr))
        .to_owned()
}

fn error_of(run: &Run) -> Value {
    serde_json::from_str::<Value>(run.stderr.trim_end()).expect("the error is JSON")["error"]
        .clone()
}

fn stamped(project: &Project) -> Vec<Stamp> {
    stamps(&project.db_path())
}

#[test]
fn it_should_record_the_environment_session_on_a_new_node() {
    let project = Project::new("acme");

    append_in_session(&project, SESSION, &[premise_wire("SQLite is fast")]);

    assert_eq!(stamped(&project), [Stamp::new(PREMISE, false, SESSION)]);
}

#[test]
fn it_should_record_the_environment_session_on_a_new_edge() {
    let project = Project::new("acme");
    append_in_session(
        &project,
        OTHER_SESSION,
        &[premise_wire("SQLite is fast"), decision_wire()],
    );

    append_in_session(&project, SESSION, &[relation_wire(PREMISE, DECISION)]);

    assert_eq!(
        stamped(&project).last(),
        Some(&Stamp::new(EDGE, false, SESSION))
    );
}

#[test]
fn it_should_record_the_environment_session_on_a_revision() {
    let project = Project::new("acme");
    let seeded = append_in_session(&project, OTHER_SESSION, &[premise_wire("SQLite is fast")]);
    let mut revised = premise_wire("SQLite is fast enough");
    revised["base"] = json!(record_id(&seeded, 0));

    append_in_session(&project, SESSION, &[revised]);

    assert_eq!(
        stamped(&project),
        [
            Stamp::new(PREMISE, false, OTHER_SESSION),
            Stamp::new(PREMISE, false, SESSION)
        ]
    );
}

#[test]
fn it_should_record_the_environment_session_on_a_retirement() {
    let project = Project::new("acme");
    let seeded = append_in_session(&project, OTHER_SESSION, &[premise_wire("SQLite is fast")]);
    let retirement = json!({
        "body": { "kind": "retirement", "lineage": { "kind": "node", "id": PREMISE } },
        "base": record_id(&seeded, 0),
    });

    append_in_session(&project, SESSION, &[retirement]);

    assert_eq!(
        stamped(&project),
        [
            Stamp::new(PREMISE, false, OTHER_SESSION),
            Stamp::new(PREMISE, true, SESSION)
        ]
    );
}

/// Banco, sonda limpa no HEAD e raiz de projeto de um teste; cada chamada reabre o que empresta, com a sessão dada.
struct Fixture {
    raw: RawHarness,
    probe: FakeProbe,
    root: ExtTree,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture {
            raw: RawHarness::new(),
            probe: FakeProbe::clean(snap('a'))
                .with_tree(TreeSource::Head, Ok(ProbeOutcome::Reproducible(tree('c')))),
            root: ExtTree::new(),
        }
    }

    fn deps<'a>(&'a self, session: &'a SessionSource) -> OmhlDeps<'a> {
        OmhlDeps {
            clock: &self.raw.clock,
            ids: &self.raw.ids,
            probe: &self.probe,
            session,
        }
    }

    fn omhl<'a>(&'a self, session: &'a SessionSource) -> Omhl<'a> {
        let mut omhl = Omhl::open(&self.raw.config(), self.deps(session)).expect("omhl opens");
        omhl.ensure_tenant(&acme()).expect("the tenant is created");

        omhl
    }

    fn app<'a>(&'a self, session: &'a SessionSource) -> App<'a> {
        let app = App::open(
            &self.raw.db_path(),
            &acme(),
            self.deps(session),
            &RetryPolicy::default(),
        )
        .unwrap_or_else(|error| panic!("the app opens: {error}"));

        app.with_project_root(self.root.root())
    }

    fn dispatch(&self, session: &SessionSource, action: &Action) -> Value {
        self.app(session)
            .dispatch(&Caller::cli(), action)
            .unwrap_or_else(|error| panic!("the action is accepted: {error}"))
    }

    fn stamps(&self) -> Vec<Stamp> {
        stamps(&self.raw.db_path())
    }

    fn last_stamp(&self) -> Stamp {
        self.stamps().pop().expect("the database has records")
    }

    fn seed_work_item(&self) {
        let no_session = SessionSource::Absent;
        let item = node_item(WORK_ITEM, "work-item", work_item_data("Ship it"));

        self.omhl(&no_session)
            .append(&request(vec![item]))
            .expect("the work item is accepted");
    }

    /// Carrega a diretiva e apaga o arquivo: o node segue vigente, órfão.
    fn seed_orphaned_guideline(&self) {
        let no_session = SessionSource::Absent;
        self.root
            .guideline("be-brief", "kind = \"business\"\ntext = \"Be brief\"\n");
        self.dispatch(&no_session, &Action::Load { dry_run: false });
        fs::remove_file(self.root.root().join(".omhl/guidelines/be-brief.toml"))
            .expect("the guideline file is removed");
    }
}

#[test]
fn it_should_record_the_session_on_an_attestation() {
    let fx = Fixture::new();
    fx.seed_work_item();
    let attest = Action::Attest {
        check: "checks.run-tests".to_owned(),
        work_item: WORK_ITEM.to_owned(),
        outcome: "passed".to_owned(),
        note: None,
    };

    fx.dispatch(&raw_session(SESSION), &attest);

    assert_eq!(fx.last_stamp().session.as_deref(), Some(SESSION));
}

#[test]
fn it_should_record_the_session_on_a_load() {
    let fx = Fixture::new();
    fx.root
        .guideline("be-brief", "kind = \"business\"\ntext = \"Be brief\"\n");

    fx.dispatch(&raw_session(SESSION), &Action::Load { dry_run: false });

    assert_eq!(fx.last_stamp(), Stamp::new(GUIDELINE, false, SESSION));
}

#[test]
fn it_should_record_the_session_on_a_check_retirement() {
    let fx = Fixture::new();
    let data = mechanical_check_data("before-pr", "x", true);
    let no_session = SessionSource::Absent;
    let request = hexlog::AppendRequest {
        author: loader_author(),
        ..request(vec![node_item(CHECK, "check", data)])
    };
    fx.omhl(&no_session)
        .append(&request)
        .expect("the check is accepted");
    let retire = Action::CheckRetire {
        check: CHECK.to_owned(),
    };

    fx.dispatch(&raw_session(SESSION), &retire);

    assert_eq!(fx.last_stamp(), Stamp::new(CHECK, true, SESSION));
}

#[test]
fn it_should_record_the_session_on_a_guideline_retirement() {
    let fx = Fixture::new();
    fx.seed_orphaned_guideline();
    let retire = Action::GuidelineRetire {
        id: GUIDELINE.to_owned(),
        by: None,
    };

    fx.dispatch(&raw_session(SESSION), &retire);

    assert_eq!(fx.last_stamp(), Stamp::new(GUIDELINE, true, SESSION));
}

#[test]
fn it_should_record_the_session_on_a_close() {
    let fx = Fixture::new();
    let no_session = SessionSource::Absent;
    seed_pull_request(&mut fx.omhl(&no_session));
    attest_required(&mut fx.omhl(&no_session), &snap('a'));
    let session = raw_session(SESSION);

    fx.omhl(&session)
        .close_work_item(
            &acme(),
            &sid(PR_7),
            &snapshots(&snap('a'), &tree('c')),
            &all_native_enabled(),
            &omhl::facade::WorkRequirements::none(),
            &omhl::facade::Closing {
                author: &author(),
                note: None,
            },
        )
        .expect("the work item is closed");

    assert_eq!(fx.last_stamp(), Stamp::new(PR_7, false, SESSION));
}

#[test]
fn it_should_record_the_marker_when_the_variable_is_absent_outside_claude_code() {
    let project = Project::new("acme");

    append_items(&project, false, None, &[premise_wire("SQLite is fast")]);

    assert_eq!(
        stamped(&project),
        [Stamp::new(PREMISE, false, OUTSIDE_CLAUDE_CODE)]
    );
}

#[test]
fn it_should_record_the_marker_when_the_variable_is_absent_under_claude_code() {
    let project = Project::new("acme");

    append_items(&project, true, None, &[premise_wire("SQLite is fast")]);

    assert_eq!(
        stamped(&project),
        [Stamp::new(PREMISE, false, OUTSIDE_CLAUDE_CODE)]
    );
}

/// A recusa de `invalid-session-id`: saída 1, o erro sem o valor e nada gravado.
fn assert_refused_without_writing(value: OsString) {
    let project = Project::new("acme");

    let (exit, run) = append_items(
        &project,
        false,
        Some(value),
        &[premise_wire("SQLite is fast")],
    );

    let error = error_of(&run);
    assert_eq!(
        (
            exit == ExitCode::FAILURE,
            error["code"].clone(),
            stamped(&project).len()
        ),
        (true, json!("invalid-session-id"), 0)
    );
}

#[test]
fn it_should_refuse_an_empty_session_variable() {
    assert_refused_without_writing(OsString::new());
}

#[test]
fn it_should_refuse_a_session_variable_that_is_not_utf8() {
    assert_refused_without_writing(OsString::from_vec(vec![0x66, 0x6f, 0x80, 0x6f]));
}

#[test]
fn it_should_refuse_a_session_variable_of_101_utf16_units() {
    assert_refused_without_writing(OsString::from("x".repeat(101)));
}

#[test]
fn it_should_refuse_a_session_variable_equal_to_the_marker() {
    assert_refused_without_writing(OsString::from(OUTSIDE_CLAUDE_CODE));
}

#[test]
fn it_should_accept_a_session_variable_of_100_utf16_units() {
    let project = Project::new("acme");

    append_in_session(
        &project,
        &"x".repeat(100),
        &[premise_wire("SQLite is fast")],
    );

    assert_eq!(
        stamped(&project),
        [Stamp::new(PREMISE, false, &"x".repeat(100))]
    );
}

#[test]
fn it_should_keep_the_chain_head_and_the_record_count_when_the_session_has_101_characters() {
    let project = Project::new("acme");
    append_in_session(&project, SESSION, &[decision_wire()]);
    let before = (chain_head(&project.db_path()), stamped(&project).len());

    append_in_session(
        &project,
        &"q".repeat(101),
        &[premise_wire("SQLite is fast")],
    );

    assert_eq!(
        (chain_head(&project.db_path()), stamped(&project).len()),
        before
    );
}

#[test]
fn it_should_refuse_a_retry_with_the_same_key_and_an_invalid_session_without_replaying() {
    let project = Project::new("acme");
    let request = json!({ "key": "retry-1", "items": [premise_wire("SQLite is fast")] });
    let run = |session: &str| {
        run_omhl_with_session(
            &project,
            &["--format", "json", "append"],
            &request.to_string(),
            false,
            None,
            Some(OsString::from(session)),
        )
        .1
    };
    run(SESSION);

    let retry = run("");

    assert_eq!(
        (error_of(&retry)["code"].clone(), stamped(&project).len()),
        (json!("invalid-session-id"), 1)
    );
}

#[test]
fn it_should_not_carry_the_session_value_in_the_error_or_in_its_path() {
    let project = Project::new("acme");
    let value = "q".repeat(101);

    let run = append_in_session(&project, &value, &[premise_wire("SQLite is fast")]);

    let error = error_of(&run);
    assert_eq!(
        (
            run.stderr.contains(&value) || run.stdout.contains(&value),
            error["details"][0]["path"].clone()
        ),
        (false, json!(""))
    );
}

#[test]
fn it_should_record_the_main_session_id_with_the_role_as_the_author_agent() {
    let project = Project::new("acme");

    let (exit, _) = run_omhl_with_session(
        &project,
        &["--format", "json", "append"],
        &json!({ "items": [decision_wire()] }).to_string(),
        true,
        Some("omhl:critic"),
        Some(OsString::from(SESSION)),
    );

    let (_, recorded) = run_omhl_with_session(
        &project,
        &["--format", "json", "get", DECISION],
        "",
        false,
        None,
        None,
    );
    let node: Value = serde_json::from_str(recorded.stdout.trim_end()).expect("the node is JSON");
    assert_eq!(
        (
            exit == ExitCode::SUCCESS,
            stamped(&project),
            node["node"]["author"]["agent"].clone()
        ),
        (
            true,
            vec![Stamp::new(DECISION, false, SESSION)],
            json!("omhl:critic")
        )
    );
}

const PROJECTION_FILES: &str = "projection files";

fn projected_text(project: &Project) -> String {
    fs::read_dir(project.dir().join(OMHL_DIR).join(PROJECTION_DIR))
        .expect("the projection folder exists")
        .map(|entry| {
            let path = entry.expect("the entry is read").path();

            fs::read_to_string(path).expect("the projected file is read")
        })
        .collect()
}

/// Um work item e uma premissa gravados com `SESSION`; devolve a saída de cada comando de leitura e o texto dos arquivos que o `project` gravou.
fn read_outputs(format: &str, title: &str) -> Vec<(String, String)> {
    let project = Project::new("acme");
    append_in_session(
        &project,
        SESSION,
        &[
            node_wire(WORK_ITEM, "work-item", &work_item_data(title)),
            premise_wire("SQLite is fast"),
        ],
    );
    let commands: [&[&str]; 7] = [
        &["get", WORK_ITEM],
        &["list"],
        &["render"],
        &["project"],
        &["verify"],
        &["report", "answers"],
        &["report", "retro", WORK_ITEM],
    ];

    let mut outputs: Vec<(String, String)> = commands
        .iter()
        .map(|command| {
            let mut args = vec!["--format", format];
            args.extend_from_slice(command);
            let (exit, run) = run_omhl_with_session(&project, &args, "", false, None, None);
            assert_eq!(
                exit,
                ExitCode::SUCCESS,
                "{command:?} failed: {}",
                run.stderr
            );
            assert!(
                !(run.stdout.is_empty() && run.stderr.is_empty()),
                "{command:?} printed nothing"
            );

            (command.join(" "), format!("{}{}", run.stdout, run.stderr))
        })
        .collect();
    assert!(
        outputs[0].1.contains(WORK_ITEM),
        "the work item was not read back: {}",
        outputs[0].1
    );
    let projected = projected_text(&project);
    assert!(
        projected.contains(WORK_ITEM),
        "the work item was not projected: {projected}"
    );
    outputs.push((PROJECTION_FILES.to_owned(), projected));

    outputs
}

fn leaking_commands(format: &str, title: &str) -> Vec<String> {
    read_outputs(format, title)
        .into_iter()
        .filter(|(_, output)| output.contains(SESSION))
        .map(|(command, _)| command)
        .collect()
}

#[test]
fn it_should_not_print_the_session_in_the_agent_format() {
    assert_eq!(leaking_commands("agent", "Ship it"), Vec::<String>::new());
}

#[test]
fn it_should_not_print_the_session_in_the_human_format() {
    assert_eq!(leaking_commands("human", "Ship it"), Vec::<String>::new());
}

#[test]
fn it_should_not_print_the_session_in_the_json_format() {
    assert_eq!(leaking_commands("json", "Ship it"), Vec::<String>::new());
}

/// Controle positivo dos três testes acima: o id posto no título tem de ser achado.
#[test]
fn it_should_find_the_session_id_when_a_read_does_print_it() {
    let leaking = leaking_commands("json", SESSION);

    assert_eq!(
        (
            leaking.contains(&format!("get {WORK_ITEM}")),
            leaking.contains(&PROJECTION_FILES.to_owned())
        ),
        (true, true),
        "{leaking:?}"
    );
}

#[test]
fn it_should_keep_agent_client_and_model_as_the_only_author_fields() {
    let author = Author {
        agent: "claude".to_owned(),
        model: Some("fable".to_owned()),
        client: "cli".to_owned(),
    };

    let value = output::author(&author);

    let mut keys: Vec<&String> = value.as_object().expect("an object").keys().collect();
    keys.sort();
    assert_eq!(keys, ["agent", "client", "model"]);
}

#[test]
fn it_should_give_the_node_revision_and_relation_views_no_session() {
    let fx = Fixture::new();
    let session = raw_session(SESSION);
    let mut omhl = fx.omhl(&session);
    let items = vec![
        node_item(PREMISE, "premise", premise_data("SQLite is fast", None)),
        node_item(DECISION, "decision", decision_data("Use WAL", None)),
        support::pr_gate::relation_item("supports", PREMISE, DECISION),
    ];
    omhl.append(&request(items)).expect("the batch is accepted");

    let node = omhl.get(&acme(), &sid(PREMISE)).expect("the node is read");
    let revisions = omhl
        .lineage_records(&acme(), &hexlog::Lineage::Node(sid(PREMISE)))
        .expect("the revisions are read");
    let relations = omhl
        .relations(
            &acme(),
            &RelationQuery {
                relation_type: Some(rty("supports")),
                from: None,
                to: None,
                include_retired: true,
            },
        )
        .expect("the relations are read");

    let dump = format!("{node:?} {revisions:?} {relations:?}").to_lowercase();
    assert_eq!(
        (dump.contains("session"), dump.contains(SESSION)),
        (false, false)
    );
}

#[test]
fn it_should_give_no_text_profile_a_session_entry() {
    let with_session: Vec<String> = CommandKey::ALL
        .iter()
        .flat_map(|key| {
            let profile = profile_for(*key);
            profile
                .success
                .iter()
                .chain(profile.failure)
                .filter(|entry| entry.key.contains("session"))
                .map(move |entry| format!("{}: {}", profile.label, entry.key))
        })
        .collect();

    assert_eq!(with_session, Vec::<String>::new());
}

fn invalid_session() -> SessionSource {
    raw_session("")
}

#[test]
fn it_should_refuse_the_session_before_the_guard_in_an_append() {
    let fx = Fixture::new();
    let misplaced = node_item("checks.x", "decision", decision_data("Use WAL", None));
    let code_of = |session: &SessionSource| match fx
        .omhl(session)
        .append(&request(vec![misplaced.clone()]))
    {
        Err(OmhlError::Rule { code, .. }) => code,
        other => panic!("the append must be refused, got {other:?}"),
    };

    let codes = (code_of(&raw_session(SESSION)), code_of(&invalid_session()));

    assert_eq!(codes, (RuleCode::IdNamespace, RuleCode::InvalidSessionId));
}

#[test]
fn it_should_refuse_the_session_first_in_a_close_before_the_probe_and_the_gate() {
    let fx = Fixture::new();
    let no_session = SessionSource::Absent;
    seed_pull_request(&mut fx.omhl(&no_session));
    let head_before = chain_head(&fx.raw.db_path());
    let session = invalid_session();
    let failing_probe = FakeProbe::failing();
    let deps = OmhlDeps {
        probe: &failing_probe,
        ..fx.deps(&session)
    };
    let mut omhl = Omhl::open(&fx.raw.config(), deps).expect("omhl opens");

    let refused = omhl.close_work_item(
        &acme(),
        &sid("work-items.missing"),
        &snapshots(&snap('a'), &tree('c')),
        &all_native_enabled(),
        &omhl::facade::WorkRequirements::none(),
        &omhl::facade::Closing {
            author: &author(),
            note: None,
        },
    );

    assert_eq!(
        (refused.err(), chain_head(&fx.raw.db_path()) == head_before),
        (
            Some(omhl::facade::CloseError::Omhl(OmhlError::rule(
                RuleCode::InvalidSessionId,
                ""
            ))),
            true
        )
    );
}
