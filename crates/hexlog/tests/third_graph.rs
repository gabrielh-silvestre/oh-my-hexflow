mod support;

use hexlog::{Catalog, Hexlog, RelationTypeDef, TypeDef};
use serde::Deserialize;
use serde_json::json;
use support::{
    Harness, append_one, commit, node, relation, relation_query, retire, retire_relation, revise,
    rty, sid, tenant, with_snapshot,
};

#[derive(Deserialize)]
struct ThirdGraph {
    types: Vec<TypeDef>,
    relations: Vec<RelationTypeDef>,
}

fn third_graph_catalog() -> Catalog {
    let definitions: ThirdGraph = serde_json::from_str(include_str!("fixtures/third_graph.json"))
        .expect("the third graph definitions are valid JSON");

    Catalog::new(definitions.types, definitions.relations)
        .expect("the third graph definitions are accepted by the catalog")
}

/// Dois nodes ligados, consultados, revisados e retirados: o caminho comum aos três grafos.
fn walk_pair(
    hexlog: &mut Hexlog<'_>,
    parent: (&str, &str, serde_json::Value),
    child: (&str, &str, serde_json::Value),
    relation_type: &str,
    revised_child_data: &serde_json::Value,
) {
    let acme = tenant("acme");
    let parent_record = append_one(hexlog, node(parent.0, parent.1, parent.2));
    let child_record = append_one(hexlog, node(child.0, child.1, child.2));
    let link = append_one(hexlog, relation(relation_type, parent.0, child.0));

    let linked = hexlog.relations(&acme, &relation_query()).unwrap();
    let linked: Vec<_> = linked
        .iter()
        .filter(|view| view.relation_type == rty(relation_type))
        .map(|view| (view.from.as_str(), view.to.as_str()))
        .collect();
    assert_eq!(linked, vec![(parent.0, child.0)]);

    let revision = append_one(
        hexlog,
        revise(
            node(child.0, child.1, revised_child_data.clone()),
            &child_record,
        ),
    );
    let current = hexlog.get(&acme, &sid(child.0)).unwrap().unwrap();
    assert_eq!(
        (&current.record_id, &current.data),
        (&revision, revised_child_data)
    );

    append_one(
        hexlog,
        retire_relation(relation_type, parent.0, child.0, &link),
    );
    append_one(hexlog, retire(child.0, &revision));
    append_one(hexlog, retire(parent.0, &parent_record));

    assert_eq!(hexlog.get(&acme, &sid(child.0)).unwrap(), None);
    assert!(
        hexlog
            .relations(&acme, &relation_query())
            .unwrap()
            .iter()
            .all(|view| view.relation_type != rty(relation_type)),
        "the retired relation is hidden by default"
    );
}

/// Grafo `tickets`: nodes com snapshot, duas relation types, revisão e retirada.
fn walk_tickets(hexlog: &mut Hexlog<'_>) {
    let acme = tenant("acme");
    let login = append_one(
        hexlog,
        with_snapshot(
            node("bug.login", "ticket", json!({"status": "open"})),
            commit('c'),
        ),
    );
    let logout = append_one(
        hexlog,
        with_snapshot(
            node("bug.logout", "ticket", json!({"status": "open"})),
            commit('c'),
        ),
    );
    let release = append_one(hexlog, node("release.v1", "milestone", json!({})));
    let blocks = append_one(hexlog, relation("blocks", "bug.login", "bug.logout"));
    let targets = append_one(hexlog, relation("targets", "bug.login", "release.v1"));

    let pairs: Vec<_> = hexlog
        .relations(&acme, &relation_query())
        .unwrap()
        .iter()
        .map(|view| {
            (
                view.relation_type.as_str().to_owned(),
                view.to.as_str().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("blocks".to_owned(), "bug.logout".to_owned()),
            ("targets".to_owned(), "release.v1".to_owned()),
        ]
    );

    let closed = append_one(
        hexlog,
        with_snapshot(
            revise(
                node("bug.login", "ticket", json!({"status": "closed"})),
                &login,
            ),
            commit('d'),
        ),
    );
    let current = hexlog.get(&acme, &sid("bug.login")).unwrap().unwrap();
    assert_eq!(
        (&current.record_id, &current.data, &current.snapshot),
        (&closed, &json!({"status": "closed"}), &Some(commit('d')))
    );

    append_one(
        hexlog,
        retire_relation("blocks", "bug.login", "bug.logout", &blocks),
    );
    append_one(
        hexlog,
        retire_relation("targets", "bug.login", "release.v1", &targets),
    );
    append_one(hexlog, retire("bug.login", &closed));
    append_one(hexlog, retire("bug.logout", &logout));
    append_one(hexlog, retire("release.v1", &release));
    assert_eq!(hexlog.get(&acme, &sid("bug.login")).unwrap(), None);
}

#[test]
fn it_should_accept_a_third_graph_defined_only_by_data() {
    let harness = Harness::new();
    let mut hexlog = harness.open_acme_with(&harness.config(), third_graph_catalog());

    walk_pair(
        &mut hexlog,
        ("doc.intro", "section", json!({"title": "Intro"})),
        ("doc.p1", "paragraph", json!({"text": "first"})),
        "contains",
        &json!({"text": "first, revised"}),
    );
    walk_pair(
        &mut hexlog,
        ("people.ana", "person", json!({"name": "Ana"})),
        ("teams.core", "team", json!({"name": "Core"})),
        "member-of",
        &json!({"name": "Core platform"}),
    );
    walk_tickets(&mut hexlog);
    let report = hexlog.verify_chain(&tenant("acme"), None).unwrap();

    assert!(report.ok, "breaks: {:?}", report.breaks);
}

#[test]
fn it_should_let_the_adapter_enforce_a_cross_record_invariant_the_engine_does_not_know() {
    // O motor não conhece a regra de transição; quem a conhece valida antes de chamar a Facade.
    fn transition_allowed(from: &str, to: &str) -> bool {
        from == "open" && to == "closed"
    }
    let harness = Harness::new();
    let mut hexlog = harness.open_acme_with(&harness.config(), third_graph_catalog());
    let opened = append_one(
        &mut hexlog,
        with_snapshot(
            node("bug.login", "ticket", json!({"status": "open"})),
            commit('c'),
        ),
    );
    let closed = append_one(
        &mut hexlog,
        with_snapshot(
            revise(
                node("bug.login", "ticket", json!({"status": "closed"})),
                &opened,
            ),
            commit('d'),
        ),
    );

    let reopened = hexlog.append(&support::request(
        "acme",
        None,
        vec![with_snapshot(
            revise(
                node("bug.login", "ticket", json!({"status": "open"})),
                &closed,
            ),
            commit('e'),
        )],
    ));

    assert!(!transition_allowed("closed", "open"));
    assert!(reopened.is_ok(), "the engine accepts closed to open");
}
