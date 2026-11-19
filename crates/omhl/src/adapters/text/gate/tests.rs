use serde_json::{Value, json};

use crate::adapters::text::{CommandKey, Layout, render_success};

/// O app só devolve retro com ao menos uma divergência ou um gap sem critério, então o retro sem itens vem à mão.
fn close_value_with_retro(retro: &Value) -> Value {
    json!({
        "outcome": {
            "chain_head": { "hash": "h-0", "seq": 4 },
            "records": [{ "id": "acme:0192a000-0000-7000-8000-000000000004", "seq": 4 }],
            "replayed": false,
        },
        "report": {
            "chain_head": { "hash": "h-1", "seq": 3 },
            "gate": "pull-request",
            "passed": true,
            "questions": [{
                "failing": [], "kind": "occurred", "label": "checks.run-tests",
                "matched": 1, "omitted": 0, "passed": true,
            }],
        },
        "retro": retro,
    })
}

fn empty_retro() -> Value {
    json!({
        "divergences": [],
        "amendments": { "approved": [], "pending": [] },
        "no_criterion": [],
        "open_gaps": [],
    })
}

#[test]
fn it_should_summarize_an_empty_retro_as_none_in_the_agent_layout() {
    let value = close_value_with_retro(&empty_retro());

    let text = render_success(CommandKey::Close, &value, Layout::Agent);

    assert!(text.contains("✔ retro · none"), "{text}");
}

#[test]
fn it_should_summarize_an_empty_retro_as_none_in_the_human_layout() {
    let value = close_value_with_retro(&empty_retro());

    let text = render_success(CommandKey::Close, &value, Layout::Human);

    assert!(text.contains("✔ retro · none"), "{text}");
}

#[test]
fn it_should_not_propose_an_improvement_when_the_retro_has_no_items() {
    let value = close_value_with_retro(&empty_retro());

    let text = render_success(CommandKey::Close, &value, Layout::Agent);

    assert!(!text.contains("Next:"), "{text}");
}

#[test]
fn it_should_omit_the_retro_block_when_the_retro_is_null() {
    let value = close_value_with_retro(&Value::Null);

    let text = render_success(CommandKey::Close, &value, Layout::Agent);

    assert!(!text.contains("retro"), "{text}");
}
