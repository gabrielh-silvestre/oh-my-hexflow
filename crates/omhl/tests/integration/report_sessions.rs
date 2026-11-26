use crate::support;

use std::fs;

use hexlog::{AppendOutcome, AppendRequest, Author, Config, Deps, Hexlog};
use omhl::facade::{Omhl, OmhlDeps, SessionSource};
use omhl::graphs;
use omhl::native::NativeBase;
use serde_json::{Value, json};
use support::gate::relation_item;
use support::omhl::{
    acme, decision_data, node_item, premise_data, pull_request_data, request, retirement_item,
    revision_item, work_item_data,
};
use support::pr_gate::{mechanical_check_data, relation_retirement_item};
use support::project::Project;
use support::run::{Run, run_omhl_with_session};
use support::session::{chain_head, raw_session, stamps};
use support::{FakeProbe, FixedClock, SeqIds, author, loader_author, snap};

const SESSION_A: &str = "sess-zulu";
const SESSION_B: &str = "sess-yankee";
const SESSION_C: &str = "sess-xray";
const SESSION_D: &str = "sess-whiskey";
const SESSION_E: &str = "sess-victor";
const SESSION_F: &str = "sess-uniform";
const SESSION_G: &str = "sess-tango";
// Em ordem de primeiro registro; o alfabeto corre ao contrário de propósito.
const SESSIONS_BY_FIRST_RECORD: [&str; 7] = [
    SESSION_A, SESSION_B, SESSION_C, SESSION_D, SESSION_E, SESSION_F, SESSION_G,
];

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const W3: &str = "work-items.w3";
const PR: &str = "work-items.pr-7";
const GHOST: &str = "work-items.ghost";
const NATIVE_GUIDELINE: &str = "guidelines.one-decision-per-turn";

const FIRST_RECORD_OF_A: &str = "2026-11-10T10:00:00.000Z";
const LAST_RECORD_OF_A: &str = "2026-11-10T17:00:00.000Z";

/// Banco temporário do projeto `acme`: cada gravação abre o `omhl` com a sessão e o relógio dados.
struct Db {
    project: Project,
    probe: FakeProbe,
    ids: SeqIds,
}

impl Db {
    fn new() -> Db {
        let project = Project::new("acme");
        fs::create_dir_all(
            project
                .db_path()
                .parent()
                .expect("the database has a folder"),
        )
        .expect("the data folder is created");

        Db {
            project,
            probe: FakeProbe::clean(snap('a')),
            ids: SeqIds::new(),
        }
    }

    fn write_as(
        &self,
        session: Option<&str>,
        at: &str,
        who: Author,
        items: Vec<hexlog::AppendItem>,
    ) -> AppendOutcome {
        let clock = FixedClock::at(at);
        let source = session.map_or(SessionSource::Absent, raw_session);
        let deps = OmhlDeps {
            clock: &clock,
            ids: &self.ids,
            probe: &self.probe,
            session: &source,
        };
        let mut omhl = Omhl::open(&Config::new(self.project.db_path()), deps).expect("omhl opens");
        omhl.ensure_tenant(&acme()).expect("the tenant is created");
        let request = AppendRequest {
            author: who,
            ..request(items)
        };

        omhl.append(&request).expect("the append is accepted")
    }

    fn write(
        &self,
        session: Option<&str>,
        at: &str,
        items: Vec<hexlog::AppendItem>,
    ) -> AppendOutcome {
        self.write_as(session, at, author(), items)
    }

    /// Como um binário anterior grava: direto no `hexlog`, sem o campo de sessão.
    fn write_unstamped(&self, at: &str, items: Vec<hexlog::AppendItem>) {
        let clock = FixedClock::at(at);
        let deps = Deps {
            clock: &clock,
            ids: &self.ids,
            catalog: graphs::catalog().expect("the embedded catalogs build"),
            native: &NativeBase,
        };
        let mut hexlog =
            Hexlog::open(&Config::new(self.project.db_path()), deps).expect("hexlog opens");
        hexlog
            .ensure_tenant(&acme())
            .expect("the tenant is created");
        hexlog
            .append(&request(items))
            .expect("the unstamped append is accepted");
    }

    fn run(&self, args: &[&str]) -> (std::process::ExitCode, Run) {
        run_omhl_with_session(&self.project, args, "", false, None, None)
    }

    fn sessions(&self, filter: &[&str]) -> Vec<Value> {
        let mut args = vec!["--format", "json", "report", "sessions"];
        for work_item in filter {
            args.extend(["--work-item", work_item]);
        }
        let (_, run) = self.run(&args);
        let value: Value = serde_json::from_str(run.stdout.trim_end())
            .unwrap_or_else(|_| panic!("the output is JSON: {}", run.stderr));

        value["sessions"]
            .as_array()
            .expect("the output has the list")
            .clone()
    }

    fn session_ids(&self, filter: &[&str]) -> Vec<String> {
        self.sessions(filter)
            .iter()
            .map(|session| session["id"].as_str().expect("the id is text").to_owned())
            .collect()
    }

    fn session(&self, id: &str) -> Value {
        self.sessions(&[])
            .into_iter()
            .find(|session| session["id"] == id)
            .unwrap_or_else(|| panic!("session {id} is listed"))
    }
}

fn on_day(at: &str) -> String {
    format!("2026-11-10T{at}:00.000Z")
}

fn seeded() -> Db {
    let db = Db::new();
    db.write_unstamped(
        "2026-11-09T08:00:00.000Z",
        vec![node_item(
            "premises.old",
            "premise",
            premise_data("Written before sessions", None),
        )],
    );
    db.write(
        None,
        "2026-11-09T09:00:00.000Z",
        vec![
            node_item(W1, "work-item", work_item_data("First")),
            node_item(W2, "work-item", work_item_data("Second")),
        ],
    );
    let seed = db.write(
        None,
        "2026-11-09T09:30:00.000Z",
        vec![
            node_item("premises.p1", "premise", premise_data("P1", Some(W1))),
            node_item("decisions.d1", "decision", decision_data("D1", Some(W2))),
            node_item("premises.pn", "premise", premise_data("No work item", None)),
            node_item(
                "decisions.dn",
                "decision",
                decision_data("No work item", None),
            ),
            node_item("premises.px", "premise", premise_data("Mover", Some(W1))),
            node_item(
                "decisions.dx",
                "decision",
                decision_data("Anchor", Some(W1)),
            ),
            node_item(
                "premises.pr1",
                "premise",
                premise_data("To retire", Some(W2)),
            ),
        ],
    );
    let px_base = seed.records[4].id.clone();
    let pr1_base = seed.records[6].id.clone();

    let pa = db.write(
        Some(SESSION_A),
        &on_day("10:00"),
        vec![node_item(
            "premises.pa",
            "premise",
            premise_data("Pa", Some(W1)),
        )],
    );
    db.write(
        Some(SESSION_A),
        &on_day("10:05"),
        vec![
            node_item(W3, "work-item", work_item_data("Third")),
            node_item(PR, "pull-request", pull_request_data("Gate")),
        ],
    );

    let check = mechanical_check_data("before-pr", "x", true);
    db.write_as(
        Some(SESSION_B),
        &on_day("11:00"),
        loader_author(),
        vec![
            node_item(
                "guidelines.g1",
                "guideline",
                json!({ "text": "Be brief", "kind": "business" }),
            ),
            node_item("checks.c1", "check", check),
        ],
    );
    db.write(
        Some(SESSION_B),
        &on_day("11:30"),
        vec![
            relation_item("supports", NATIVE_GUIDELINE, "premises.pn"),
            relation_item("supports", "premises.pn", "decisions.dn"),
        ],
    );

    db.write(
        Some(SESSION_C),
        &on_day("12:00"),
        vec![relation_item("supports", "premises.p1", "decisions.dn")],
    );
    db.write(
        Some(SESSION_D),
        &on_day("13:00"),
        vec![relation_item("supports", "premises.p1", "decisions.d1")],
    );
    db.write(
        Some(SESSION_E),
        &on_day("14:00"),
        vec![retirement_item("premises.pr1", &pr1_base)],
    );
    db.write(
        Some(SESSION_F),
        &on_day("15:00"),
        vec![relation_item("supports", "premises.px", "decisions.dx")],
    );
    db.write(
        Some(SESSION_G),
        &on_day("16:00"),
        vec![revision_item(
            "premises.px",
            "premise",
            premise_data("Mover", Some(W2)),
            &px_base,
        )],
    );
    db.write(
        Some(SESSION_A),
        &on_day("17:00"),
        vec![revision_item(
            "premises.pa",
            "premise",
            premise_data("Pa revised", Some(W1)),
            &pa.records[0].id,
        )],
    );

    db
}

fn first_line(run: &Run) -> &str {
    run.stdout.lines().next().expect("the output has a line")
}

#[test]
fn it_should_list_one_row_per_real_session_in_the_order_of_their_first_record() {
    let db = seeded();

    let ids = db.session_ids(&[]);

    assert_eq!(ids, SESSIONS_BY_FIRST_RECORD);
}

#[test]
fn it_should_list_a_session_that_only_retired_a_record() {
    let db = seeded();

    let session = db.session(SESSION_E);

    assert_eq!(
        (session["nodes"].clone(), session["relations"].clone()),
        (json!(1), json!(0))
    );
}

#[test]
fn it_should_list_a_session_that_only_revised_a_record() {
    let db = seeded();

    let session = db.session(SESSION_G);

    assert_eq!(
        (session["nodes"].clone(), session["relations"].clone()),
        (json!(1), json!(0))
    );
}

#[test]
fn it_should_count_every_node_record_of_a_session_including_revisions() {
    let db = seeded();

    let session = db.session(SESSION_A);

    assert_eq!(session["nodes"], json!(4));
}

#[test]
fn it_should_total_the_records_of_each_session() {
    let db = seeded();

    let totals: Vec<Value> = db
        .sessions(&[])
        .iter()
        .map(|session| json!([session["id"], session["nodes"], session["relations"]]))
        .collect();

    assert_eq!(
        totals,
        [
            json!([SESSION_A, 4, 0]),
            json!([SESSION_B, 2, 2]),
            json!([SESSION_C, 0, 1]),
            json!([SESSION_D, 0, 1]),
            json!([SESSION_E, 1, 0]),
            json!([SESSION_F, 0, 1]),
            json!([SESSION_G, 1, 0]),
        ]
    );
}

#[test]
fn it_should_date_each_session_from_its_first_to_its_last_record() {
    let db = seeded();

    let dates: Vec<Value> = db
        .sessions(&[])
        .iter()
        .map(|session| json!([session["id"], session["first"], session["last"]]))
        .collect();

    assert_eq!(
        dates,
        [
            json!([SESSION_A, FIRST_RECORD_OF_A, LAST_RECORD_OF_A]),
            json!([SESSION_B, on_day("11:00"), on_day("11:30")]),
            json!([SESSION_C, on_day("12:00"), on_day("12:00")]),
            json!([SESSION_D, on_day("13:00"), on_day("13:00")]),
            json!([SESSION_E, on_day("14:00"), on_day("14:00")]),
            json!([SESSION_F, on_day("15:00"), on_day("15:00")]),
            json!([SESSION_G, on_day("16:00"), on_day("16:00")]),
        ]
    );
}

#[test]
fn it_should_order_sessions_by_their_first_record_even_when_its_date_is_later() {
    let db = Db::new();
    db.write(
        Some(SESSION_A),
        &on_day("11:00"),
        vec![node_item("premises.a", "premise", premise_data("A", None))],
    );
    db.write(
        Some(SESSION_B),
        &on_day("10:00"),
        vec![node_item("premises.b", "premise", premise_data("B", None))],
    );

    let ids = db.session_ids(&[]);

    assert_eq!(ids, [SESSION_A, SESSION_B]);
}

#[test]
fn it_should_date_a_session_in_write_order_when_the_clock_goes_back() {
    let db = Db::new();
    db.write(
        Some(SESSION_A),
        &on_day("11:00"),
        vec![node_item(
            "premises.a1",
            "premise",
            premise_data("A1", None),
        )],
    );
    db.write(
        Some(SESSION_A),
        &on_day("09:00"),
        vec![node_item(
            "premises.a2",
            "premise",
            premise_data("A2", None),
        )],
    );

    let session = db.session(SESSION_A);

    assert_eq!(
        (session["first"].clone(), session["last"].clone()),
        (json!(on_day("11:00")), json!(on_day("09:00")))
    );
}

#[test]
fn it_should_take_the_work_item_of_a_node_from_its_data_and_the_own_id_of_work_item_and_pull_request_nodes()
 {
    let db = seeded();

    let session = db.session(SESSION_A);

    assert_eq!(session["work_items"], json!([PR, W1, W3]));
}

#[test]
fn it_should_give_no_work_item_to_directive_check_native_and_unlinked_relations() {
    let db = seeded();

    let session = db.session(SESSION_B);

    assert_eq!(session["work_items"], json!([]));
}

#[test]
fn it_should_give_a_retirement_the_work_item_of_the_previous_record_of_its_lineage() {
    let db = seeded();

    let session = db.session(SESSION_E);

    assert_eq!(session["work_items"], json!([W2]));
}

#[test]
fn it_should_give_a_relation_the_work_item_of_the_endpoint_that_has_one() {
    let db = seeded();

    let session = db.session(SESSION_C);

    assert_eq!(session["work_items"], json!([W1]));
}

#[test]
fn it_should_give_a_relation_between_two_work_items_both_of_them() {
    let db = seeded();

    let session = db.session(SESSION_D);

    assert_eq!(session["work_items"], json!([W1, W2]));
}

#[test]
fn it_should_keep_the_work_item_an_endpoint_had_when_the_relation_was_written() {
    let db = seeded();

    let session = db.session(SESSION_F);

    assert_eq!(session["work_items"], json!([W1]));
}

/// Os work items entram num lote próprio: a guarda só aceita referência a um já gravado.
fn with_work_items() -> Db {
    let db = Db::new();
    db.write(
        None,
        &on_day("09:00"),
        vec![
            node_item(W1, "work-item", work_item_data("First")),
            node_item(W2, "work-item", work_item_data("Second")),
        ],
    );

    db
}

/// Relation `premises.p` -> `decisions.d` gravada com as duas pontas em W1; as pontas listadas vão para W2 e só então SESSION_C a retira.
fn retired_relation_after_moving_to_w2(moved: &[&str]) -> Db {
    let db = with_work_items();
    let seed = db.write(
        None,
        &on_day("09:30"),
        vec![
            node_item("premises.p", "premise", premise_data("P", Some(W1))),
            node_item("decisions.d", "decision", decision_data("D", Some(W1))),
        ],
    );
    let relation = db.write(
        Some(SESSION_A),
        &on_day("10:00"),
        vec![relation_item("supports", "premises.p", "decisions.d")],
    );
    for end in moved {
        let revision = match *end {
            "premises.p" => revision_item(
                end,
                "premise",
                premise_data("P", Some(W2)),
                &seed.records[0].id,
            ),
            _ => revision_item(
                end,
                "decision",
                decision_data("D", Some(W2)),
                &seed.records[1].id,
            ),
        };
        db.write(Some(SESSION_B), &on_day("11:00"), vec![revision]);
    }
    db.write(
        Some(SESSION_C),
        &on_day("12:00"),
        vec![relation_retirement_item(
            "supports",
            "premises.p",
            "decisions.d",
            &relation.records[0].id,
        )],
    );

    db
}

#[test]
fn it_should_give_a_relation_retirement_the_work_items_of_the_previous_record_of_its_lineage() {
    let db = retired_relation_after_moving_to_w2(&["premises.p"]);

    let session = db.session(SESSION_C);

    assert_eq!(session["work_items"], json!([W1]));
}

#[test]
fn it_should_not_list_a_relation_retirement_under_the_work_item_an_endpoint_moved_to() {
    let db = retired_relation_after_moving_to_w2(&["premises.p"]);

    let ids = db.session_ids(&[W2]);

    assert_eq!(ids, [SESSION_B]);
}

#[test]
fn it_should_list_a_relation_retirement_under_the_old_work_item_after_both_endpoints_left_it() {
    let db = retired_relation_after_moving_to_w2(&["premises.p", "decisions.d"]);

    let ids = db.session_ids(&[W1]);

    assert_eq!(ids, [SESSION_A, SESSION_C]);
}

#[test]
fn it_should_count_a_relation_retirement_in_the_relation_total() {
    let db = retired_relation_after_moving_to_w2(&["premises.p"]);

    let session = db.session(SESSION_C);

    assert_eq!(
        (session["nodes"].clone(), session["relations"].clone()),
        (json!(0), json!(1))
    );
}

#[test]
fn it_should_give_no_work_item_to_a_relation_whose_endpoint_lost_it_in_a_revision() {
    let db = with_work_items();
    let seed = db.write(
        None,
        &on_day("09:30"),
        vec![
            node_item("premises.p", "premise", premise_data("P", Some(W1))),
            node_item("decisions.d", "decision", decision_data("D", None)),
        ],
    );
    db.write(
        None,
        &on_day("10:00"),
        vec![revision_item(
            "premises.p",
            "premise",
            premise_data("P", None),
            &seed.records[0].id,
        )],
    );
    db.write(
        Some(SESSION_A),
        &on_day("11:00"),
        vec![relation_item("supports", "premises.p", "decisions.d")],
    );

    let session = db.session(SESSION_A);

    assert_eq!(session["work_items"], json!([]));
}

#[test]
fn it_should_give_a_revision_the_work_item_it_moved_to() {
    let db = seeded();

    let session = db.session(SESSION_G);

    assert_eq!(session["work_items"], json!([W2]));
}

#[test]
fn it_should_print_agent_text_with_a_check_mark_when_no_format_is_given() {
    let db = seeded();

    let (_, run) = db.run(&["report", "sessions"]);

    assert_eq!(first_line(&run), "✔ report sessions · 7 sessions");
}

#[test]
fn it_should_print_agent_text_with_one_line_per_session() {
    let db = seeded();

    let (_, run) = db.run(&["report", "sessions"]);

    assert!(
        run.stdout.contains(&format!(
            "session {SESSION_A} · first {FIRST_RECORD_OF_A} · last {LAST_RECORD_OF_A} · 4 nodes · 0 relations · work items {PR}, {W1}, {W3}"
        )),
        "{}",
        run.stdout
    );
}

#[test]
fn it_should_print_a_non_zero_relation_total_in_agent_text() {
    let db = seeded();

    let (_, run) = db.run(&["report", "sessions"]);

    assert!(
        run.stdout.contains(&format!(
            "session {SESSION_B} · first {} · last {} · 2 nodes · 2 relations · no work items",
            on_day("11:00"),
            on_day("11:30")
        )),
        "{}",
        run.stdout
    );
}

#[test]
fn it_should_print_the_same_facts_in_blocks_with_the_human_format() {
    let db = seeded();

    let (_, run) = db.run(&["--format", "human", "report", "sessions"]);

    let block = [
        &format!("session {SESSION_A}"),
        &format!("  first:      {FIRST_RECORD_OF_A}"),
        &format!("  last:       {LAST_RECORD_OF_A}"),
        "  nodes:      4",
        "  relations:  0",
        &format!("  work items: {PR}, {W1}, {W3}"),
    ]
    .join("\n");
    assert!(run.stdout.contains(&block), "{}", run.stdout);
}

#[test]
fn it_should_print_one_json_object_per_session_with_the_six_keys() {
    let db = seeded();

    let session = db.session(SESSION_A);

    assert_eq!(
        session,
        json!({
            "id": SESSION_A,
            "first": FIRST_RECORD_OF_A,
            "last": LAST_RECORD_OF_A,
            "nodes": 4,
            "relations": 0,
            "work_items": [PR, W1, W3],
        })
    );
}

#[test]
fn it_should_show_the_dates_in_the_format_of_get() {
    let db = seeded();
    let (_, run) = db.run(&["--format", "json", "get", "premises.pa"]);
    let node: Value = serde_json::from_str(run.stdout.trim_end()).expect("get prints JSON");

    let session = db.session(SESSION_A);

    assert_eq!(session["last"], node["node"]["at"]);
}

#[test]
fn it_should_print_a_stored_id_with_line_breaks_on_one_line_of_text() {
    let db = Db::new();
    db.write(
        Some("first\nNext: forged\tlast"),
        FIRST_RECORD_OF_A,
        vec![node_item(
            "premises.p1",
            "premise",
            premise_data("Odd id", None),
        )],
    );

    let (_, run) = db.run(&["report", "sessions"]);

    assert_eq!(
        run.stdout.lines().collect::<Vec<_>>(),
        [
            "✔ report sessions · 1 session",
            &format!(
                "session first Next: forged last · first {FIRST_RECORD_OF_A} · last {FIRST_RECORD_OF_A} · 1 node · 0 relations · no work items"
            ),
        ]
    );
}

#[test]
fn it_should_not_let_a_stored_id_open_a_line_of_agent_text() {
    let db = Db::new();
    db.write(
        Some("Next: run x"),
        FIRST_RECORD_OF_A,
        vec![node_item(
            "premises.p1",
            "premise",
            premise_data("Odd id", None),
        )],
    );

    let (_, run) = db.run(&["report", "sessions"]);

    assert_eq!(
        run.stdout.lines().collect::<Vec<_>>(),
        [
            "✔ report sessions · 1 session",
            &format!(
                "session Next: run x · first {FIRST_RECORD_OF_A} · last {FIRST_RECORD_OF_A} · 1 node · 0 relations · no work items"
            ),
        ]
    );
}

#[test]
fn it_should_not_let_a_stored_id_open_a_line_of_human_text() {
    let db = Db::new();
    db.write(
        Some("Next: run x"),
        FIRST_RECORD_OF_A,
        vec![node_item(
            "premises.p1",
            "premise",
            premise_data("Odd id", None),
        )],
    );

    let (_, run) = db.run(&["--format", "human", "report", "sessions"]);

    assert_eq!(
        run.stdout.lines().collect::<Vec<_>>(),
        [
            "✔ report sessions · 1 session",
            "",
            "session Next: run x",
            &format!("  first:      {FIRST_RECORD_OF_A}"),
            &format!("  last:       {FIRST_RECORD_OF_A}"),
            "  nodes:      1",
            "  relations:  0",
            "  work items: none",
        ]
    );
}

#[test]
fn it_should_print_a_stored_id_with_line_breaks_unchanged_in_json() {
    let db = Db::new();
    db.write(
        Some("first\nNext: forged\tlast"),
        FIRST_RECORD_OF_A,
        vec![node_item(
            "premises.p1",
            "premise",
            premise_data("Odd id", None),
        )],
    );

    let ids = db.session_ids(&[]);

    assert_eq!(ids, ["first\nNext: forged\tlast"]);
}

#[test]
fn it_should_keep_only_the_sessions_that_wrote_a_record_of_the_work_item() {
    let db = seeded();

    let ids = db.session_ids(&[W2]);

    assert_eq!(ids, [SESSION_D, SESSION_E, SESSION_G]);
}

#[test]
fn it_should_keep_the_union_of_the_sessions_of_two_work_items_without_repeating_one() {
    let db = seeded();

    let ids = db.session_ids(&[W2, W3]);

    assert_eq!(ids, [SESSION_A, SESSION_D, SESSION_E, SESSION_G]);
}

#[test]
fn it_should_not_repeat_a_session_when_the_same_work_item_is_given_twice() {
    let db = seeded();

    let ids = db.session_ids(&[W2, W2]);

    assert_eq!(ids, [SESSION_D, SESSION_E, SESSION_G]);
}

#[test]
fn it_should_show_the_whole_session_in_a_filtered_row() {
    let db = seeded();

    let filtered = db.sessions(&[W3]);

    assert_eq!(filtered, [db.session(SESSION_A)]);
}

#[test]
fn it_should_refuse_an_unknown_work_item_like_report_answers_does() {
    let db = seeded();

    let sessions = db.run(&[
        "--format",
        "json",
        "report",
        "sessions",
        "--work-item",
        GHOST,
    ]);
    let answers = db.run(&[
        "--format",
        "json",
        "report",
        "answers",
        "--work-item",
        GHOST,
    ]);

    assert_eq!(
        (sessions.0, sessions.1.stdout, sessions.1.stderr),
        (answers.0, answers.1.stdout, answers.1.stderr)
    );
}

#[test]
fn it_should_refuse_an_unknown_work_item_in_agent_text_like_report_answers_does() {
    let db = seeded();

    let sessions = db.run(&["report", "sessions", "--work-item", GHOST]);
    let answers = db.run(&["report", "answers", "--work-item", GHOST]);

    assert_eq!(
        (sessions.1.stdout, sessions.1.stderr),
        (answers.1.stdout, answers.1.stderr)
    );
}

#[test]
fn it_should_refuse_an_unknown_work_item_among_known_ones_with_the_not_found_code() {
    let db = seeded();

    let (_, run) = db.run(&[
        "--format",
        "json",
        "report",
        "sessions",
        "--work-item",
        W1,
        "--work-item",
        GHOST,
    ]);

    let error: Value = serde_json::from_str(run.stderr.trim_end()).expect("the error is JSON");
    assert_eq!(error["error"]["code"], "work-item-not-found");
}

#[test]
fn it_should_print_the_same_output_on_two_runs() {
    let db = seeded();

    let first = db.run(&["report", "sessions"]).1.stdout;
    let second = db.run(&["report", "sessions"]).1.stdout;

    assert_eq!(first, second);
}

#[test]
fn it_should_print_a_check_mark_and_an_empty_list_for_a_database_without_real_sessions() {
    let db = Db::new();
    db.write(
        None,
        FIRST_RECORD_OF_A,
        vec![node_item(
            "premises.p1",
            "premise",
            premise_data("Marked", None),
        )],
    );
    db.write_unstamped(
        FIRST_RECORD_OF_A,
        vec![node_item(
            "premises.p2",
            "premise",
            premise_data("Old", None),
        )],
    );

    let (_, text) = db.run(&["report", "sessions"]);
    let listed = db.sessions(&[]);

    assert_eq!(
        (text.stdout, listed),
        ("✔ report sessions · none\n".to_owned(), Vec::<Value>::new())
    );
}

#[test]
fn it_should_leave_the_chain_head_the_sequence_and_the_record_count_untouched() {
    let db = seeded();
    let before = (
        chain_head(&db.project.db_path()),
        stamps(&db.project.db_path()).len(),
    );

    db.run(&["report", "sessions"]);
    db.run(&["--format", "json", "report", "sessions", "--work-item", W1]);
    db.run(&["--format", "human", "report", "sessions"]);

    let after = (
        chain_head(&db.project.db_path()),
        stamps(&db.project.db_path()).len(),
    );
    assert_eq!(after, before);
}
