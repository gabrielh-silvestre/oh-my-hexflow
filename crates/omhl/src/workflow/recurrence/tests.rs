use crate::workflow::recurrence::{Premise, Recurrence, normalize, recurrences};

const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";
const W3: &str = "work-items.w3";
const TLS: &str = "Use TLS for every external call.";

// Camada pura: `crate::workflow::recurrence::{normalize, recurrences}`

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn pure(id: &str, kind: &str, text: &str, work_item: &str) -> Premise {
    Premise {
        id: id.to_owned(),
        kind: kind.to_owned(),
        text: text.to_owned(),
        work_item: work_item.to_owned(),
    }
}

fn tls(id: &str, work_item: &str) -> Premise {
    pure(id, "technical", TLS, work_item)
}

fn found(premises: &[Premise], directives: &[&str], min: u32) -> Vec<Recurrence> {
    recurrences(premises, &strings(directives), min)
}

#[test]
fn it_should_trim_the_edges_of_a_text() {
    assert_eq!(normalize("  use tls \n"), "use tls");
}

#[test]
fn it_should_lowercase_an_ascii_text() {
    assert_eq!(normalize("Use TLS"), "use tls");
}

#[test]
fn it_should_lowercase_a_non_ascii_text() {
    assert_eq!(normalize("AÇÃO Única"), "ação única");
}

#[test]
fn it_should_collapse_runs_of_whitespace_into_one_space() {
    assert_eq!(normalize("use  \t tls\n\nalways"), "use tls always");
}

#[test]
fn it_should_remove_trailing_sentence_punctuation() {
    for mark in [".", ",", ";", ":", "!", "?"] {
        let text = format!("use tls{mark}");

        assert_eq!(normalize(&text), "use tls", "mark {mark:?}");
    }
}

#[test]
fn it_should_remove_a_run_of_trailing_punctuation() {
    assert_eq!(normalize("use tls?!..."), "use tls");
}

#[test]
fn it_should_remove_the_space_left_before_the_removed_punctuation() {
    assert_eq!(normalize("use tls . "), "use tls");
}

#[test]
fn it_should_keep_interior_punctuation() {
    assert_eq!(
        normalize("Use TLS, always: no exceptions"),
        "use tls, always: no exceptions"
    );
}

#[test]
fn it_should_keep_a_trailing_closing_parenthesis() {
    assert_eq!(normalize("call connect()"), "call connect()");
}

#[test]
fn it_should_give_one_key_to_texts_that_differ_only_in_case_spacing_and_final_punctuation() {
    assert_eq!(normalize("  Use   TLS!! "), normalize("use tls."));
}

#[test]
fn it_should_list_a_text_seen_in_as_many_distinct_work_items_as_the_minimum() {
    let premises = [
        tls("premises.p1", W1),
        tls("premises.p2", W2),
        tls("premises.p3", W3),
    ];

    let groups = found(&premises, &[], 3);

    assert_eq!(groups.len(), 1);
}

#[test]
fn it_should_not_list_a_text_seen_in_fewer_distinct_work_items_than_the_minimum() {
    let premises = [tls("premises.p1", W1), tls("premises.p2", W2)];

    let groups = found(&premises, &[], 3);

    assert_eq!(groups, []);
}

#[test]
fn it_should_count_a_work_item_once_however_many_premises_it_has() {
    let premises = [
        tls("premises.p1", W1),
        tls("premises.p2", W1),
        tls("premises.p3", W1),
    ];

    let groups = found(&premises, &[], 2);

    assert_eq!(groups, []);
}

#[test]
fn it_should_group_texts_that_differ_in_case_spacing_and_final_punctuation() {
    let premises = [
        pure("premises.p1", "technical", "Use TLS.", W1),
        pure("premises.p2", "technical", "use   tls", W2),
        pure("premises.p3", "technical", "USE TLS!", W3),
    ];

    let groups = found(&premises, &[], 3);

    assert_eq!(groups.len(), 1);
}

#[test]
fn it_should_keep_premises_of_different_kinds_apart() {
    let premises = [
        pure("premises.p1", "technical", TLS, W1),
        pure("premises.p2", "business", TLS, W2),
        pure("premises.p3", "technical", TLS, W3),
    ];

    let groups = found(&premises, &[], 3);

    assert_eq!(groups, []);
}

#[test]
fn it_should_describe_a_group_with_its_kind_normalized_text_work_items_and_premises() {
    let premises = [
        pure("premises.p1", "technical", "Use TLS.", W1),
        pure("premises.p2", "technical", "use tls", W2),
    ];

    let groups = found(&premises, &[], 2);

    assert_eq!(
        groups,
        [Recurrence {
            kind: "technical".to_owned(),
            text: "use tls".to_owned(),
            work_items: strings(&[W1, W2]),
            premises: strings(&["premises.p1", "premises.p2"]),
        }]
    );
}

#[test]
fn it_should_list_each_work_item_once_in_the_order_it_first_appears() {
    let premises = [
        tls("premises.p1", W2),
        tls("premises.p2", W1),
        tls("premises.p3", W2),
    ];

    let groups = found(&premises, &[], 2);

    assert_eq!(groups[0].work_items, strings(&[W2, W1]));
}

#[test]
fn it_should_list_every_premise_of_the_group_in_input_order() {
    let premises = [
        tls("premises.p1", W1),
        tls("premises.p2", W1),
        tls("premises.p3", W2),
    ];

    let groups = found(&premises, &[], 2);

    assert_eq!(
        groups[0].premises,
        strings(&["premises.p1", "premises.p2", "premises.p3"])
    );
}

#[test]
fn it_should_order_the_groups_by_their_first_premise() {
    let premises = [
        pure("premises.p1", "technical", "second text", W1),
        pure("premises.p2", "technical", "first text", W1),
        pure("premises.p3", "technical", "second text", W2),
        pure("premises.p4", "technical", "first text", W2),
    ];

    let groups = found(&premises, &[], 2);

    let texts: Vec<&str> = groups.iter().map(|group| group.text.as_str()).collect();
    assert_eq!(texts, ["second text", "first text"]);
}

#[test]
fn it_should_hide_a_group_whose_text_matches_a_directive_after_normalization() {
    let premises = [tls("premises.p1", W1), tls("premises.p2", W2)];

    let groups = found(&premises, &["  USE tls for   every external call "], 2);

    assert_eq!(groups, []);
}

#[test]
fn it_should_keep_a_group_when_the_directive_text_differs() {
    let premises = [tls("premises.p1", W1), tls("premises.p2", W2)];

    let groups = found(&premises, &["Use TLS for internal calls only"], 2);

    assert_eq!(groups.len(), 1);
}

#[test]
fn it_should_find_nothing_without_premises() {
    let groups = found(&[], &[], 1);

    assert_eq!(groups, []);
}
