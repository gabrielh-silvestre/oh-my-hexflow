//! O vocabulário central da saída vista pela API pública: símbolo, recuo e fecho iguais nos dois layouts (a limpeza
//! de espaço tem unitários em `vocab.rs` e testes de ponta a ponta em `check_run_text.rs`).

use omhl::adapters::text::{
    AgentVocabulary, HumanVocabulary, NEXT_LABEL, NOTHING_BLOCKED, SYMBOL_ERROR, SYMBOL_OK,
    SYMBOL_WARNING, State, Vocabulary,
};

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
