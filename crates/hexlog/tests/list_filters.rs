mod support;

use hexlog::{ErrorCode, FieldName, Hexlog, ListFilter, NodeView, Page, PageRequest, Scalar};
use serde_json::json;
use support::{
    Harness, all, append_one, assert_refusal, commit, field, node, page, request, tenant,
    with_snapshot,
};

fn listed_ids(hexlog: &mut Hexlog<'_>, filter: &ListFilter) -> Vec<String> {
    hexlog
        .list(&tenant("acme"), filter, &page(50))
        .unwrap()
        .items
        .into_iter()
        .map(|view| view.id.as_str().to_owned())
        .collect()
}

fn where_eq(name: &str, value: Scalar) -> ListFilter {
    ListFilter {
        where_eq: vec![(field(name), value)],
        ..all()
    }
}

fn summary(page: &Page<NodeView>) -> (Vec<u64>, Option<u64>) {
    let seqs = page.items.iter().map(|view| view.seq).collect();

    (seqs, page.next_after_seq)
}

#[test]
fn it_should_match_prefixes_only_at_dot_boundaries() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let items = vec![
        node("a.b", "note", json!({})),
        node("a.b.c", "note", json!({})),
        node("a.bc", "note", json!({})),
    ];
    hexlog.append(&request("acme", None, items)).unwrap();
    let filter = ListFilter {
        id_prefix: Some("a.b".to_owned()),
        ..all()
    };

    let ids = listed_ids(&mut hexlog, &filter);

    assert_eq!(ids, ["a.b", "a.b.c"]);
}

#[test]
fn it_should_filter_nodes_by_snapshot() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    append_one(
        &mut hexlog,
        with_snapshot(node("a.one", "strict-note", json!({})), commit('a')),
    );
    append_one(
        &mut hexlog,
        with_snapshot(node("a.two", "strict-note", json!({})), commit('b')),
    );
    let filter = ListFilter {
        snapshot: Some(commit('a')),
        ..all()
    };

    let ids = listed_ids(&mut hexlog, &filter);

    assert_eq!(ids, ["a.one"]);
}

#[test]
fn it_should_filter_by_a_top_level_scalar_field() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let items = vec![
        node("a.bool", "note", json!({"x": true})),
        node("a.int", "note", json!({"x": 1})),
        node("a.str", "note", json!({"x": "1"})),
    ];
    hexlog.append(&request("acme", None, items)).unwrap();

    let by_bool = listed_ids(&mut hexlog, &where_eq("x", Scalar::Bool(true)));
    let by_int = listed_ids(&mut hexlog, &where_eq("x", Scalar::Int(1)));
    let by_str = listed_ids(&mut hexlog, &where_eq("x", Scalar::Str("1".to_owned())));

    assert_eq!(by_bool, ["a.bool"]);
    assert_eq!(by_int, ["a.int"]);
    assert_eq!(by_str, ["a.str"]);
}

#[test]
fn it_should_match_a_where_field_only_inside_data_even_when_its_name_equals_an_envelope_field() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    let items = vec![
        node("a.one", "note", json!({"type": "x", "snapshot": "y"})),
        node("b.one", "note", json!({"type": "z"})),
    ];
    hexlog.append(&request("acme", None, items)).unwrap();

    let by_data_value = listed_ids(&mut hexlog, &where_eq("type", Scalar::Str("x".to_owned())));
    let by_envelope_value = listed_ids(
        &mut hexlog,
        &where_eq("type", Scalar::Str("note".to_owned())),
    );

    assert_eq!(by_data_value, ["a.one"]);
    assert!(by_envelope_value.is_empty());
}

#[test]
fn it_should_reject_a_where_field_name_with_json_path_or_sql_metacharacters() {
    let cases = [
        "a.b".to_owned(),
        "a[0]".to_owned(),
        "a\"b".to_owned(),
        "a'b".to_owned(),
        String::new(),
        "a".repeat(65),
    ];

    for name in &cases {
        let error = FieldName::parse(name).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidInput, "code for {name:?}");
    }
}

#[test]
fn it_should_treat_a_hostile_scalar_value_as_data_and_leave_the_tables_intact() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    append_one(&mut hexlog, node("a.one", "note", json!({"x": "ok"})));
    let hostile = where_eq("x", Scalar::Str("'; DROP TABLE records; --".to_owned()));

    let matched = listed_ids(&mut hexlog, &hostile);

    let intact = listed_ids(&mut hexlog, &where_eq("x", Scalar::Str("ok".to_owned())));
    assert!(matched.is_empty());
    assert_eq!(intact, ["a.one"]);
}

#[test]
fn it_should_page_by_seq_with_a_keyset_cursor() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();
    for index in 1..=5 {
        append_one(&mut hexlog, node(&format!("a.n{index}"), "note", json!({})));
    }
    let acme = tenant("acme");
    let by = |after_seq| PageRequest {
        limit: 2,
        after_seq,
    };

    let first = hexlog.list(&acme, &all(), &by(None)).unwrap();
    let second = hexlog
        .list(&acme, &all(), &by(first.next_after_seq))
        .unwrap();
    let third = hexlog
        .list(&acme, &all(), &by(second.next_after_seq))
        .unwrap();
    let exact_fit = hexlog.list(&acme, &all(), &by(Some(3))).unwrap();

    assert_eq!(summary(&first), (vec![1, 2], Some(2)));
    assert_eq!(summary(&second), (vec![3, 4], Some(4)));
    assert_eq!(summary(&third), (vec![5], None));
    assert_eq!(summary(&exact_fit), (vec![4, 5], None));
}

#[test]
fn it_should_reject_a_page_limit_outside_1_to_200() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme();

    for limit in [0, 201] {
        let error = hexlog
            .list(&tenant("acme"), &all(), &page(limit))
            .unwrap_err();

        assert_refusal(&error, ErrorCode::InvalidInput, "page-limit", "/page/limit");
    }
}
