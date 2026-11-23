use serde_json::{Value, json};

use super::latest_per_pair;
use crate::test_support::views::{NodeViewExt, node_view};

fn attestation(id: &str, seq: u64, data: Value) -> hexlog::NodeView {
    node_view(id, "attestation", data).seq(seq)
}

fn byproducts(nodes: Vec<hexlog::NodeView>) -> Vec<String> {
    latest_per_pair(nodes)
        .iter()
        .map(|node| {
            node.data["byproduct"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

#[test]
fn it_should_keep_only_the_highest_seq_of_each_check_and_work_item_pair() {
    let nodes = vec![
        attestation(
            "a.1",
            5,
            json!({ "check": "c1", "work_item": "w1", "byproduct": "new" }),
        ),
        attestation(
            "a.2",
            2,
            json!({ "check": "c1", "work_item": "w1", "byproduct": "old" }),
        ),
        attestation(
            "a.3",
            1,
            json!({ "check": "c1", "work_item": "w2", "byproduct": "other" }),
        ),
    ];

    let latest = byproducts(nodes);

    assert_eq!(latest, ["new", "other"]);
}

#[test]
fn it_should_treat_an_attestation_without_work_item_as_its_own_pair() {
    let nodes = vec![
        attestation(
            "a.1",
            3,
            json!({ "check": "c1", "byproduct": "latest-bare" }),
        ),
        attestation("a.2", 1, json!({ "check": "c1", "byproduct": "old-bare" })),
        attestation(
            "a.3",
            2,
            json!({ "check": "c1", "work_item": "w1", "byproduct": "paired" }),
        ),
    ];

    let latest = byproducts(nodes);

    assert_eq!(latest, ["latest-bare", "paired"]);
}
