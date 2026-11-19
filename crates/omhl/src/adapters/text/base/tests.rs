use super::*;

fn profile(success: &'static [Entry], failure: &'static [Entry]) -> Profile {
    Profile {
        label: "test",
        nodes: None,
        success,
        failure,
    }
}

fn shape_of(
    profile: &Profile,
    node_type: Option<&str>,
    parent: Option<&str>,
    key: &str,
) -> Option<Shape> {
    profile
        .find(Outcome::Success, Region::Data, node_type, parent, key)
        .map(|entry| entry.shape)
}

#[test]
fn it_should_prefer_the_explicit_entry_with_the_same_parent_over_one_without_parent() {
    const ENTRIES: &[Entry] = &[
        Entry::new(Region::Data, "id", Shape::Literal),
        Entry::new(Region::Data, "id", Shape::Hash(8)).under("snapshot"),
    ];
    let table = profile(ENTRIES, &[]);

    let shape = shape_of(&table, None, Some("snapshot"), "id");

    assert_eq!(shape, Some(Shape::Hash(8)));
}

#[test]
fn it_should_prefer_an_explicit_entry_without_parent_over_a_node_type_wildcard() {
    const ENTRIES: &[Entry] = &[
        Entry::new(Region::Data, WILDCARD, Shape::Noise).scoped("check"),
        Entry::new(Region::Data, "how", Shape::Literal),
    ];
    let table = profile(ENTRIES, &[]);

    let shape = shape_of(&table, Some("check"), None, "how");

    assert_eq!(shape, Some(Shape::Literal));
}

#[test]
fn it_should_prefer_the_node_type_wildcard_over_the_any_wildcard() {
    const ENTRIES: &[Entry] = &[
        Entry::new(Region::Data, WILDCARD, Shape::Noise),
        Entry::new(Region::Data, WILDCARD, Shape::Closed).scoped("check"),
    ];
    let table = profile(ENTRIES, &[]);

    let shape = shape_of(&table, Some("check"), None, "anything");

    assert_eq!(shape, Some(Shape::Closed));
}

#[test]
fn it_should_fall_back_to_the_any_wildcard_when_no_other_entry_matches() {
    const ENTRIES: &[Entry] = &[
        Entry::new(Region::Data, WILDCARD, Shape::Noise),
        Entry::new(Region::Data, WILDCARD, Shape::Closed).scoped("check"),
    ];
    let table = profile(ENTRIES, &[]);

    let shape = shape_of(&table, Some("premise"), None, "anything");

    assert_eq!(shape, Some(Shape::Noise));
}

#[test]
fn it_should_prefer_the_node_type_entry_over_the_any_entry_for_the_same_key() {
    const ENTRIES: &[Entry] = &[
        Entry::new(Region::Root, "id", Shape::Literal),
        Entry::new(Region::Root, "id", Shape::Closed).scoped("attestation"),
    ];
    let table = profile(ENTRIES, &[]);

    let shape = table
        .find(
            Outcome::Success,
            Region::Root,
            Some("attestation"),
            None,
            "id",
        )
        .map(|entry| entry.shape);

    assert_eq!(shape, Some(Shape::Closed));
}

#[test]
fn it_should_use_the_any_entry_for_a_node_type_without_its_own_entry() {
    const ENTRIES: &[Entry] = &[
        Entry::new(Region::Root, "id", Shape::Literal),
        Entry::new(Region::Root, "id", Shape::Closed).scoped("attestation"),
    ];
    let table = profile(ENTRIES, &[]);

    let shape = table
        .find(Outcome::Success, Region::Root, Some("decision"), None, "id")
        .map(|entry| entry.shape);

    assert_eq!(shape, Some(Shape::Literal));
}

#[test]
fn it_should_skip_an_explicit_entry_whose_parent_differs_from_the_asked_one() {
    const ENTRIES: &[Entry] = &[Entry::new(Region::Data, "id", Shape::Hash(8)).under("snapshot")];
    let table = profile(ENTRIES, &[]);

    let shape = shape_of(&table, None, Some("gate"), "id");

    assert_eq!(shape, None);
}

#[test]
fn it_should_not_let_the_wildcard_match_outside_the_data_region() {
    const ENTRIES: &[Entry] = &[Entry::new(Region::Root, WILDCARD, Shape::Noise)];
    let table = profile(ENTRIES, &[]);

    let found = table.find(Outcome::Success, Region::Root, None, None, "anything");

    assert_eq!(found, None);
}

#[test]
fn it_should_return_none_for_a_key_outside_the_table() {
    const ENTRIES: &[Entry] = &[Entry::new(Region::Root, "created", Shape::Literal)];
    let table = profile(ENTRIES, &[]);

    let found = table.find(Outcome::Success, Region::Root, None, None, "made");

    assert_eq!(found, None);
}

#[test]
fn it_should_declare_only_the_leaves_of_an_object_never_the_object_key() {
    const ENTRIES: &[Entry] = &[Entry::new(Region::Data, "id", Shape::Hash(8)).under("snapshot")];
    let table = profile(ENTRIES, &[]);

    let object = table.find(Outcome::Success, Region::Data, None, None, "snapshot");

    assert_eq!(object, None);
}

#[test]
fn it_should_search_only_the_table_of_the_asked_outcome() {
    const SUCCESS: &[Entry] = &[Entry::new(Region::Root, "ok", Shape::Derived)];
    const FAILURE: &[Entry] = &[Entry::new(Region::Root, "code", Shape::Closed)];
    let table = profile(SUCCESS, FAILURE);

    let found = table.find(Outcome::Failure, Region::Root, None, None, "ok");

    assert_eq!(found, None);
}
