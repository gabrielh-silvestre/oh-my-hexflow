use super::*;

fn agent(doc: &Doc) -> String {
    super::agent(doc, &AgentVocabulary)
}

fn human(doc: &Doc) -> String {
    super::human(doc, &HumanVocabulary)
}

fn header(text: &str) -> Line {
    Line::Header {
        state: State::Ok,
        parts: vec![text.to_owned()],
    }
}

fn item(depth: usize, text: &str) -> Line {
    Line::Item {
        depth,
        marker: Marker::Dash,
        parts: vec![text.to_owned()],
        join: Join::Dot,
    }
}

fn field(depth: usize, label: &str, value: &str) -> Line {
    Line::Field {
        depth,
        label: label.to_owned(),
        value: value.to_owned(),
    }
}

fn note(text: &str) -> Line {
    Line::Note {
        state: State::Warning,
        text: text.to_owned(),
    }
}

fn doc(lines: Vec<Line>, closing: Closing) -> Doc {
    Doc { lines, closing }
}

fn joined_item(parts: &[&str], join: Join) -> Line {
    Line::Item {
        depth: 0,
        marker: Marker::None,
        parts: parts.iter().map(|part| (*part).to_owned()).collect(),
        join,
    }
}

#[test]
fn it_should_indent_agent_lines_by_two_spaces_per_depth() {
    let doc = doc(
        vec![item(0, "a"), item(1, "b"), item(2, "c")],
        Closing::None,
    );

    let text = agent(&doc);

    assert_eq!(text, "- a\n  - b\n    - c");
}

#[test]
fn it_should_not_insert_blank_lines_in_the_agent_layout() {
    let doc = doc(
        vec![
            header("run"),
            note("careful"),
            item(0, "a"),
            field(1, "k", "v"),
            item(0, "b"),
        ],
        Closing::Next("go".to_owned()),
    );

    let text = agent(&doc);

    assert_eq!(
        text,
        format!("{SYMBOL_OK} run\n{SYMBOL_WARNING} careful\n- a\n  k: v\n- b\n{NEXT_LABEL} go.")
    );
}

#[test]
fn it_should_not_align_field_labels_in_the_agent_layout() {
    let doc = doc(
        vec![header("run"), field(1, "a", "1"), field(1, "longer", "2")],
        Closing::None,
    );

    let text = agent(&doc);

    assert_eq!(text, format!("{SYMBOL_OK} run\n  a: 1\n  longer: 2"));
}

#[test]
fn it_should_align_field_labels_within_a_human_block() {
    let doc = doc(
        vec![header("run"), field(0, "a", "1"), field(0, "longer", "2")],
        Closing::None,
    );

    let text = human(&doc);

    assert_eq!(text, format!("{SYMBOL_OK} run\na:      1\nlonger: 2"));
}

#[test]
fn it_should_align_field_labels_independently_per_human_block() {
    let doc = doc(
        vec![
            header("one"),
            field(0, "a", "1"),
            header("two"),
            field(0, "longer", "2"),
        ],
        Closing::None,
    );

    let text = human(&doc);

    assert_eq!(
        text,
        format!("{SYMBOL_OK} one\na: 1\n\n{SYMBOL_OK} two\nlonger: 2")
    );
}

#[test]
fn it_should_separate_human_blocks_with_exactly_one_blank_line() {
    let doc = doc(vec![header("run"), note("careful")], Closing::None);

    let text = human(&doc);

    assert_eq!(text, format!("{SYMBOL_OK} run\n\n{SYMBOL_WARNING} careful"));
}

#[test]
fn it_should_keep_consecutive_childless_items_in_one_human_block() {
    let doc = doc(
        vec![header("run"), item(0, "a"), item(0, "b")],
        Closing::None,
    );

    let text = human(&doc);

    assert_eq!(text, format!("{SYMBOL_OK} run\n\n- a\n- b"));
}

#[test]
fn it_should_open_a_human_block_for_an_item_that_has_children() {
    let doc = doc(
        vec![item(0, "a"), item(0, "b"), field(1, "k", "v")],
        Closing::None,
    );

    let text = human(&doc);

    assert_eq!(text, "- a\n\n- b\n  k: v");
}

#[test]
fn it_should_put_the_next_step_in_its_own_human_block() {
    let doc = doc(
        vec![header("run"), field(0, "k", "v")],
        Closing::Next("go".to_owned()),
    );

    let text = human(&doc);

    assert_eq!(text, format!("{SYMBOL_OK} run\nk: v\n\n{NEXT_LABEL} go."));
}

#[test]
fn it_should_put_the_nothing_blocked_step_in_its_own_human_block() {
    let doc = doc(
        vec![header("run"), field(0, "k", "v")],
        Closing::NothingBlocked("go".to_owned()),
    );

    let text = human(&doc);

    assert_eq!(
        text,
        format!("{SYMBOL_OK} run\nk: v\n\n{NOTHING_BLOCKED} {NEXT_LABEL} go.")
    );
}

#[test]
fn it_should_end_the_agent_text_at_the_last_line_when_there_is_no_closing() {
    let doc = doc(vec![header("run"), item(0, "a")], Closing::None);

    let text = agent(&doc);

    assert_eq!(text, format!("{SYMBOL_OK} run\n- a"));
}

#[test]
fn it_should_end_the_human_text_at_the_last_block_when_there_is_no_closing() {
    let doc = doc(vec![header("run"), item(0, "a")], Closing::None);

    let text = human(&doc);

    assert_eq!(text, format!("{SYMBOL_OK} run\n\n- a"));
}

#[test]
fn it_should_join_item_parts_with_a_dot_separator() {
    let doc = doc(vec![joined_item(&["a", "b"], Join::Dot)], Closing::None);

    let text = agent(&doc);

    assert_eq!(text, "a · b");
}

#[test]
fn it_should_join_item_parts_with_a_two_space_gap_in_the_human_layout() {
    let doc = doc(vec![joined_item(&["a", "b"], Join::Gap)], Closing::None);

    let text = human(&doc);

    assert_eq!(text, "a  b");
}

#[test]
fn it_should_join_item_parts_with_one_space_in_the_agent_layout_where_the_gap_is() {
    let doc = doc(vec![joined_item(&["a", "b"], Join::Gap)], Closing::None);

    let text = agent(&doc);

    assert_eq!(text, "a b");
}

#[test]
fn it_should_collapse_repeated_spaces_in_agent_values_only() {
    let doc = doc(vec![field(1, "k", "  a   b ")], Closing::None);

    assert_eq!(agent(&doc), "  k: a b");
    assert_eq!(human(&doc), "  k:   a   b ");
}

#[test]
fn it_should_not_end_the_agent_text_with_a_line_feed() {
    let doc = doc(vec![header("run")], Closing::Next("go".to_owned()));

    let text = agent(&doc);

    assert!(!text.ends_with('\n'));
}

#[test]
fn it_should_not_end_the_human_text_with_a_line_feed() {
    let doc = doc(vec![header("run")], Closing::Next("go".to_owned()));

    let text = human(&doc);

    assert!(!text.ends_with('\n'));
}
