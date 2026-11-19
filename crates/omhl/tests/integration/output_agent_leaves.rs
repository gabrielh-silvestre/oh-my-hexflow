use crate::support;
use omhl::adapters::text::{Entry, NodeContainer, Outcome, Profile, Region, Shape};
use serde_json::json;
use support::output_agent_leaves::{
    fixture_problems, mask_durations, presence_problems, required_problems, table_problems,
};

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

#[test]
fn it_should_mask_every_duration_and_keep_the_other_numbers() {
    let masked = mask_durations("✔ checks.x2  passed  12ms · timeout 60s · took 2.6s · exit 7");

    assert_eq!(
        masked,
        "✔ checks.x2  passed  <dur> · timeout <dur> · took <dur> · exit 7"
    );
}
