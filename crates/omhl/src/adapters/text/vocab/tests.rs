use super::*;

#[test]
fn it_should_collapse_repeated_spaces_and_trim_the_value_ends_in_the_agent_vocabulary() {
    let cleaned = AgentVocabulary.clean("  a   b  c  ");

    assert_eq!(cleaned, "a b c");
}

#[test]
fn it_should_keep_the_value_untouched_in_the_human_vocabulary() {
    let cleaned = HumanVocabulary.clean("  a   b  ");

    assert_eq!(cleaned, "  a   b  ");
}

#[test]
fn it_should_keep_the_interior_of_an_output_line_and_trim_only_its_end() {
    let cleaned = AgentVocabulary.clean_output("  a   b   ");

    assert_eq!(cleaned, "  a   b");
}

#[test]
fn it_should_turn_the_gap_into_one_space_only_in_the_agent_vocabulary() {
    assert_eq!(AgentVocabulary.separator(Join::Gap), " ");
    assert_eq!(HumanVocabulary.separator(Join::Gap), "  ");
    assert_eq!(
        AgentVocabulary.separator(Join::Dot),
        HumanVocabulary.separator(Join::Dot)
    );
}

#[test]
fn it_should_count_zero_as_none_and_pluralize_from_two() {
    assert_eq!(counted(0, "node"), "none");
    assert_eq!(counted(1, "node"), "1 node");
    assert_eq!(counted(2, "node"), "2 nodes");
    assert_eq!(plural(0, "node"), "0 nodes");
}

#[test]
fn it_should_indent_two_spaces_per_level_in_both_vocabularies() {
    assert_eq!(HumanVocabulary.indent(3), "      ");
    assert_eq!(AgentVocabulary.indent(3), "      ");
}

#[test]
fn it_should_give_each_state_the_same_symbol_in_both_vocabularies() {
    for (state, symbol) in [
        (State::Ok, SYMBOL_OK),
        (State::Warning, SYMBOL_WARNING),
        (State::Error, SYMBOL_ERROR),
    ] {
        assert_eq!(HumanVocabulary.symbol(state), symbol);
        assert_eq!(AgentVocabulary.symbol(state), symbol);
    }
}

#[test]
fn it_should_close_with_the_next_label_and_a_final_period_in_both_vocabularies() {
    let expected = format!("{NEXT_LABEL} go on.");

    assert_eq!(HumanVocabulary.next("go on"), expected);
    assert_eq!(AgentVocabulary.next("go on"), expected);
}

#[test]
fn it_should_prefix_the_closing_step_with_the_nothing_blocked_sentence() {
    let closing = AgentVocabulary.nothing_blocked("go on");

    assert_eq!(closing, format!("{NOTHING_BLOCKED} {NEXT_LABEL} go on."));
}
