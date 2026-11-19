use crate::support;

use hexlog::{
    Catalog, EndpointStatus, ErrorCode, ExternalId, Hexlog, Origin, RecordId, RelationQuery,
    RelationTypeDef, RelationView, TypeDef,
};
use serde_json::{Value, json};
use support::catalog::{catalog_of, reference_catalog};
use support::tamper::Tamper;
use support::{
    FakeNative, Harness, append_one, assert_fork_rejected, assert_refusal, commit, node, relation,
    relation_lineage, relation_query, request, retire, retire_relation, revise, revise_relation,
    rty, sid, tenant, ty, unused_record_id, with_external, with_snapshot,
};

/// Grava um node `note` por id, em `append`s separados, e devolve os ids técnicos na mesma ordem.
fn write_notes(hexlog: &mut Hexlog<'_>, ids: &[&str]) -> Vec<RecordId> {
    ids.iter()
        .map(|id| append_one(hexlog, node(id, "note", json!({}))))
        .collect()
}

fn relations_of(hexlog: &mut Hexlog<'_>) -> Vec<RelationView> {
    hexlog
        .relations(&tenant("acme"), &relation_query())
        .expect("relations are listed")
}

fn single_relation(hexlog: &mut Hexlog<'_>) -> RelationView {
    let mut views = relations_of(hexlog);
    assert_eq!(views.len(), 1, "exactly one relation is listed");

    views.remove(0)
}

fn endpoints(views: &[RelationView]) -> Vec<(&str, &str)> {
    views
        .iter()
        .map(|view| (view.from.as_str(), view.to.as_str()))
        .collect()
}

/// Catálogo local com um relation type sem `acyclic` e sem restrição de extremo.
fn loose_catalog() -> Catalog {
    let note = TypeDef {
        name: ty("note"),
        schema: json!({"type": "object"}),
        requires_snapshot: false,
        equivalent: None,
    };
    let loose = RelationTypeDef {
        name: rty("loose"),
        from: None,
        to: None,
        acyclic: false,
        equivalent: None,
    };

    Catalog::new(vec![note], vec![loose]).expect("the local definitions are accepted")
}

#[test]
fn it_should_accept_a_relation_to_a_native_node_and_record_its_origin() {
    let harness = Harness::with_native_other();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one"]);

    append_one(&mut hexlog, relation("after", "a.one", "n.other"));

    let view = single_relation(&mut hexlog);
    assert_eq!(
        (
            view.from_origin,
            view.to_origin,
            view.native_catalog.as_deref(),
            view.to_status
        ),
        (
            Origin::Tenant,
            Origin::Native,
            Some("v1"),
            EndpointStatus::Current
        )
    );
}

#[test]
fn it_should_accept_a_relation_whose_source_is_native_and_target_is_tenant() {
    let harness = Harness::with_native_other();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one"]);

    append_one(&mut hexlog, relation("after", "n.other", "a.one"));

    let view = single_relation(&mut hexlog);
    assert_eq!(
        (view.from_origin, view.to_origin),
        (Origin::Native, Origin::Tenant)
    );
}

#[test]
fn it_should_reject_a_relation_to_a_missing_node() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one"]);

    let missing_target = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("after", "a.one", "a.ghost")],
        ))
        .unwrap_err();
    let missing_source = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("after", "a.ghost", "a.one")],
        ))
        .unwrap_err();

    assert_refusal(
        &missing_target,
        ErrorCode::InvalidRecord,
        "endpoint-not-found",
        "/items/0/body/to",
    );
    assert_refusal(
        &missing_source,
        ErrorCode::InvalidRecord,
        "endpoint-not-found",
        "/items/0/body/from",
    );
}

#[test]
fn it_should_reject_a_relation_with_an_unknown_relation_type() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("nope", "a.one", "a.two")],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::RelationTypeNotFound,
        "relation-type-not-found",
        "/items/0/body/relation_type",
    );
}

#[test]
fn it_should_reject_a_relation_between_two_native_nodes() {
    let mut harness = Harness::with_native_other();
    harness.native.reserve(sid("n.second"), ty("note"));
    let mut hexlog = harness.open_acme();

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("after", "n.other", "n.second")],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "native-only-relation",
        "/items/0/body",
    );
}

#[test]
fn it_should_reject_a_self_relation() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one"]);

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("after", "a.one", "a.one")],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "self-relation",
        "/items/0/body/to",
    );
}

#[test]
fn it_should_reject_a_relation_whose_endpoint_type_the_relation_forbids() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.note", "a.note2"]);
    append_one(&mut hexlog, node("a.task", "task", json!({})));
    append_one(&mut hexlog, node("a.task2", "task", json!({})));

    let wrong_source = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("owned-by", "a.note", "a.note2")],
        ))
        .unwrap_err();
    let wrong_target = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("owned-by", "a.task", "a.task2")],
        ))
        .unwrap_err();

    assert_refusal(
        &wrong_source,
        ErrorCode::InvalidRecord,
        "endpoint-type",
        "/items/0/body/from",
    );
    assert_refusal(
        &wrong_target,
        ErrorCode::InvalidRecord,
        "endpoint-type",
        "/items/0/body/to",
    );
}

#[test]
fn it_should_see_nodes_of_previous_items_when_validating_a_relation_in_the_same_batch() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let items = vec![
        node("a.one", "note", json!({})),
        node("a.two", "note", json!({})),
        relation("after", "a.one", "a.two"),
    ];

    hexlog.append(&request("acme", None, items)).unwrap();

    let view = single_relation(&mut hexlog);
    assert_eq!(
        (view.from_status, view.to_status),
        (EndpointStatus::Current, EndpointStatus::Current)
    );
}

#[test]
fn it_should_reject_a_cycle_on_an_acyclic_relation_including_native_relations() {
    let mut harness = Harness::new();
    harness.native.reserve(sid("n.x"), ty("note"));
    harness
        .native
        .reserve_relation(rty("after"), sid("n.x"), sid("a.one"));
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one"]);
    let head_before = hexlog.chain_head(&tenant("acme")).unwrap();

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("after", "a.one", "n.x")],
        ))
        .unwrap_err();

    assert_refusal(&error, ErrorCode::CycleRejected, "cycle", "/items/0/body");
    assert_eq!(hexlog.chain_head(&tenant("acme")).unwrap(), head_before);
}

#[test]
fn it_should_ignore_relations_with_a_retired_endpoint_when_detecting_cycles() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let ids = write_notes(&mut hexlog, &["a.one", "a.two", "a.three"]);
    append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    append_one(&mut hexlog, relation("after", "a.two", "a.three"));
    append_one(&mut hexlog, retire("a.two", &ids[1]));

    hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("after", "a.three", "a.one")],
        ))
        .unwrap();

    let views = relations_of(&mut hexlog);
    assert!(endpoints(&views).contains(&("a.three", "a.one")));
}

#[test]
fn it_should_not_revalidate_cycles_when_a_retired_node_is_reactivated() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let ids = write_notes(&mut hexlog, &["a.one", "a.two", "a.three"]);
    append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    append_one(&mut hexlog, relation("after", "a.two", "a.three"));
    let retirement = append_one(&mut hexlog, retire("a.two", &ids[1]));
    append_one(&mut hexlog, relation("after", "a.three", "a.one"));

    hexlog
        .append(&request(
            "acme",
            None,
            vec![revise(node("a.two", "note", json!({})), &retirement)],
        ))
        .unwrap();

    let views = relations_of(&mut hexlog);
    assert_eq!(
        endpoints(&views),
        [
            ("a.one", "a.two"),
            ("a.two", "a.three"),
            ("a.three", "a.one")
        ]
    );
    for view in &views {
        assert_eq!(view.from_status, EndpointStatus::Current);
        assert_eq!(view.to_status, EndpointStatus::Current);
    }
}

#[test]
fn it_should_report_retired_endpoint_status_after_the_node_is_retired() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let ids = write_notes(&mut hexlog, &["a.one", "a.two"]);
    append_one(&mut hexlog, relation("after", "a.one", "a.two"));

    append_one(&mut hexlog, retire("a.two", &ids[1]));

    let view = single_relation(&mut hexlog);
    assert_eq!(
        (view.from_status, view.to_status),
        (EndpointStatus::Current, EndpointStatus::Retired)
    );
}

#[test]
fn it_should_report_missing_native_endpoint_status_when_the_native_disappears() {
    let mut harness = Harness::with_native_other();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one"]);
    append_one(&mut hexlog, relation("after", "a.one", "n.other"));
    drop(hexlog);
    harness.native.forget(&sid("n.other"));

    let mut hexlog = harness.open();

    let view = single_relation(&mut hexlog);
    assert_eq!(view.to_status, EndpointStatus::MissingNative);
}

#[test]
fn it_should_retire_a_relation_using_the_relation_latest_record_id_as_base() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    let record_id = single_relation(&mut hexlog).record_id;

    append_one(
        &mut hexlog,
        retire_relation("after", "a.one", "a.two", &record_id),
    );

    let visible = relations_of(&mut hexlog);
    let history = hexlog
        .relations(
            &tenant("acme"),
            &RelationQuery {
                include_retired: true,
                ..relation_query()
            },
        )
        .unwrap();
    assert!(visible.is_empty());
    assert_eq!(
        history.iter().map(|view| view.retired).collect::<Vec<_>>(),
        [true]
    );
}

#[test]
fn it_should_copy_origins_and_native_catalog_from_the_latest_record_when_retiring_a_relation() {
    let mut harness = Harness::with_native_other();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one"]);
    let created = append_one(&mut hexlog, relation("after", "a.one", "n.other"));
    drop(hexlog);
    harness.native = FakeNative::new("v2");
    harness.native.reserve(sid("n.other"), ty("note"));
    let mut hexlog = harness.open();

    append_one(
        &mut hexlog,
        retire_relation("after", "a.one", "n.other", &created),
    );

    drop(hexlog);
    let raw = Tamper::open(&harness.db_path()).read_body("acme", 3);
    let body: Value = serde_json::from_str(&raw).unwrap();
    let copied = json!({
        "from_origin": body["lineage"]["from_origin"],
        "to_origin": body["lineage"]["to_origin"],
        "native_catalog": body["native_catalog"],
    });
    assert_eq!(
        copied,
        json!({"from_origin": "tenant", "to_origin": "native", "native_catalog": "v1"})
    );
}

#[test]
fn it_should_return_inbound_relations_when_querying_by_target() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two", "a.three"]);
    append_one(&mut hexlog, relation("after", "a.one", "a.three"));
    append_one(&mut hexlog, relation("after", "a.two", "a.three"));
    append_one(&mut hexlog, relation("after", "a.one", "a.two"));

    let inbound = hexlog
        .relations(
            &tenant("acme"),
            &RelationQuery {
                to: Some(sid("a.three")),
                ..relation_query()
            },
        )
        .unwrap();

    assert_eq!(
        endpoints(&inbound),
        [("a.one", "a.three"), ("a.two", "a.three")]
    );
}

#[test]
fn it_should_reject_snapshot_on_a_relation() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    let item = with_snapshot(relation("after", "a.one", "a.two"), commit('a'));

    let error = hexlog
        .append(&request("acme", None, vec![item]))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "snapshot-not-allowed",
        "/items/0/snapshot",
    );
}

#[test]
fn it_should_accept_external_id_on_a_relation_and_not_inherit_it_on_revision() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    append_one(
        &mut hexlog,
        with_external(relation("after", "a.one", "a.two"), "x"),
    );
    let first = single_relation(&mut hexlog);

    append_one(
        &mut hexlog,
        revise_relation(relation("after", "a.one", "a.two"), &first.record_id),
    );

    let revised = single_relation(&mut hexlog);
    assert_eq!(first.external_id, Some(ExternalId::parse("x").unwrap()));
    assert_eq!(revised.external_id, None);
}

#[test]
fn it_should_reject_a_second_item_for_the_same_relation_lineage_in_the_same_batch() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    let head_before = hexlog.chain_head(&tenant("acme")).unwrap();
    let items = vec![
        relation("after", "a.one", "a.two"),
        retire_relation("after", "a.one", "a.two", &unused_record_id()),
    ];

    let error = hexlog.append(&request("acme", None, items)).unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::ForkRejected,
        "lineage-in-batch",
        "/items/1/body",
    );
    assert_eq!(error.details[0].current, None);
    assert_eq!(hexlog.chain_head(&tenant("acme")).unwrap(), head_before);
}

#[test]
fn it_should_reject_a_second_root_for_an_existing_relation_lineage() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    let first = append_one(&mut hexlog, relation("after", "a.one", "a.two"));

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("after", "a.one", "a.two")],
        ))
        .unwrap_err();

    assert_fork_rejected(&error, "/items/0/base", &first, false);
}

#[test]
fn it_should_reject_a_relation_to_a_retired_node() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let ids = write_notes(&mut hexlog, &["a.one", "a.two"]);
    append_one(&mut hexlog, retire("a.two", &ids[1]));

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("after", "a.one", "a.two")],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "endpoint-not-found",
        "/items/0/body/to",
    );
}

#[test]
fn it_should_reject_a_relation_to_a_node_retired_earlier_in_the_same_batch() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let ids = write_notes(&mut hexlog, &["a.one", "a.two"]);
    let head_before = hexlog.chain_head(&tenant("acme")).unwrap();
    let items = vec![
        retire("a.two", &ids[1]),
        relation("after", "a.one", "a.two"),
    ];

    let error = hexlog.append(&request("acme", None, items)).unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "endpoint-not-found",
        "/items/1/body/to",
    );
    assert_eq!(hexlog.chain_head(&tenant("acme")).unwrap(), head_before);
}

#[test]
fn it_should_accept_a_relation_followed_by_the_retirement_of_its_target_in_one_batch() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme_with(&harness.config(), loose_catalog());
    let ids = write_notes(&mut hexlog, &["a.one", "a.two"]);
    let items = vec![
        relation("loose", "a.one", "a.two"),
        retire("a.two", &ids[1]),
    ];

    let outcome = hexlog.append(&request("acme", None, items));

    assert!(outcome.is_ok(), "{outcome:?}");
}

#[test]
fn it_should_prefer_the_native_origin_when_a_tenant_node_shadows_a_native_id() {
    let mut harness = Harness::with_native_other();
    let mut hexlog = harness.open_acme();
    let shadowed = append_one(&mut hexlog, node("n.x", "note", json!({})));
    write_notes(&mut hexlog, &["a.one"]);
    drop(hexlog);
    harness.native.reserve(sid("n.x"), ty("note"));
    let mut hexlog = harness.open();
    append_one(&mut hexlog, relation("after", "a.one", "n.x"));

    append_one(&mut hexlog, retire("n.x", &shadowed));

    let view = single_relation(&mut hexlog);
    assert_eq!(
        (
            view.to_origin,
            view.native_catalog.as_deref(),
            view.to_status
        ),
        (Origin::Native, Some("v1"), EndpointStatus::Current)
    );
}

#[test]
fn it_should_check_the_type_of_a_native_endpoint_against_the_relation_type() {
    let harness = Harness::with_native_other();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one"]);

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![relation("owned-by", "n.other", "a.one")],
        ))
        .unwrap_err();

    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "endpoint-type",
        "/items/0/body/from",
    );
}

#[test]
fn it_should_retire_a_relation_even_when_an_endpoint_is_retired() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let ids = write_notes(&mut hexlog, &["a.one", "a.two"]);
    let created = append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    append_one(&mut hexlog, retire("a.two", &ids[1]));

    append_one(
        &mut hexlog,
        retire_relation("after", "a.one", "a.two", &created),
    );

    assert!(relations_of(&mut hexlog).is_empty());
}

#[test]
fn it_should_retire_a_relation_whose_relation_type_left_the_catalog() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    let created = append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    drop(hexlog);
    let without_relation_types = catalog_of(vec![("note", json!({"type": "object"}))]);
    let mut hexlog = harness.open_with(&harness.config(), without_relation_types);

    append_one(
        &mut hexlog,
        retire_relation("after", "a.one", "a.two", &created),
    );

    drop(hexlog);
    let stored = Tamper::open(&harness.db_path()).read_text(
        "SELECT def_hash FROM records WHERE tenant = ? AND seq = ?",
        &["acme", "4"],
    );
    let expected = reference_catalog()
        .relation_type_hash(&rty("after"))
        .unwrap()
        .as_str()
        .to_owned();
    assert_eq!(stored, [expected]);
}

#[test]
fn it_should_accept_a_cycle_on_a_relation_type_that_is_not_acyclic() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme_with(&harness.config(), loose_catalog());
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    append_one(&mut hexlog, relation("loose", "a.one", "a.two"));

    append_one(&mut hexlog, relation("loose", "a.two", "a.one"));

    let views = relations_of(&mut hexlog);
    assert_eq!(endpoints(&views), [("a.one", "a.two"), ("a.two", "a.one")]);
}

#[test]
fn it_should_count_the_relations_of_earlier_items_of_the_batch_when_detecting_cycles() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two", "a.three"]);
    let head_before = hexlog.chain_head(&tenant("acme")).unwrap();
    let items = vec![
        relation("after", "a.one", "a.two"),
        relation("after", "a.two", "a.three"),
        relation("after", "a.three", "a.one"),
    ];

    let error = hexlog.append(&request("acme", None, items)).unwrap_err();

    assert_refusal(&error, ErrorCode::CycleRejected, "cycle", "/items/2/body");
    assert_eq!(hexlog.chain_head(&tenant("acme")).unwrap(), head_before);
}

#[test]
fn it_should_reinstate_a_retired_relation_with_a_revision() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let ids = write_notes(&mut hexlog, &["a.one", "a.two"]);
    let created = append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    let retirement = append_one(
        &mut hexlog,
        retire_relation("after", "a.one", "a.two", &created),
    );
    append_one(
        &mut hexlog,
        revise_relation(relation("after", "a.one", "a.two"), &retirement),
    );
    let reinstated = single_relation(&mut hexlog);
    let second_retirement = append_one(
        &mut hexlog,
        retire_relation("after", "a.one", "a.two", &reinstated.record_id),
    );
    append_one(&mut hexlog, retire("a.two", &ids[1]));

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![revise_relation(
                relation("after", "a.one", "a.two"),
                &second_retirement,
            )],
        ))
        .unwrap_err();

    assert!(!reinstated.retired);
    assert_refusal(
        &error,
        ErrorCode::InvalidRecord,
        "endpoint-not-found",
        "/items/0/body/to",
    );
}

#[test]
fn it_should_list_the_revisions_of_a_relation_lineage() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    let first = append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    let second = append_one(
        &mut hexlog,
        revise_relation(relation("after", "a.one", "a.two"), &first),
    );
    append_one(
        &mut hexlog,
        retire_relation("after", "a.one", "a.two", &second),
    );

    let revisions = hexlog
        .lineage_records(
            &tenant("acme"),
            &relation_lineage("after", "a.one", "a.two"),
        )
        .unwrap();

    let summary: Vec<_> = revisions
        .into_iter()
        .map(|revision| (revision.revises, revision.retired, revision.data))
        .collect();
    assert_eq!(
        summary,
        [
            (None, false, None),
            (Some(first), false, None),
            (Some(second), true, None),
        ]
    );
}

#[test]
fn it_should_filter_relations_by_relation_type_and_source() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    write_notes(&mut hexlog, &["a.one", "a.two"]);
    append_one(&mut hexlog, node("a.task", "task", json!({})));
    append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    append_one(&mut hexlog, relation("owned-by", "a.task", "a.one"));

    let by_type = hexlog
        .relations(
            &tenant("acme"),
            &RelationQuery {
                relation_type: Some(rty("after")),
                ..relation_query()
            },
        )
        .unwrap();
    let by_source = hexlog
        .relations(
            &tenant("acme"),
            &RelationQuery {
                from: Some(sid("a.task")),
                ..relation_query()
            },
        )
        .unwrap();

    assert_eq!(endpoints(&by_type), [("a.one", "a.two")]);
    assert_eq!(endpoints(&by_source), [("a.task", "a.one")]);
}

#[test]
fn it_should_report_a_stale_base_before_a_missing_endpoint() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let ids = write_notes(&mut hexlog, &["a.one", "a.two"]);
    let first = append_one(&mut hexlog, relation("after", "a.one", "a.two"));
    let second = append_one(
        &mut hexlog,
        revise_relation(relation("after", "a.one", "a.two"), &first),
    );
    append_one(&mut hexlog, retire("a.two", &ids[1]));

    let error = hexlog
        .append(&request(
            "acme",
            None,
            vec![revise_relation(relation("after", "a.one", "a.two"), &first)],
        ))
        .unwrap_err();

    assert_fork_rejected(&error, "/items/0/base", &second, false);
}
