use serde_json::{Value, json};

/// Work item de apoio dos testes da camada de invariantes (aberto, ver `seed_pull_request`).
pub const PR_7: &str = "work-items.pr-7";
pub const RUN_TESTS: &str = "checks.run-tests";

pub fn check_data(event: &str, how: &str, blocking: bool) -> Value {
    json!({ "when": { "vocabulary": "closed", "event": event }, "how": how, "blocking": blocking })
}

pub fn decision_data(text: &str, work_item: Option<&str>) -> Value {
    let mut data = json!({
        "text": text,
        "evidence": [{ "source": "docs", "summary": "the docs say so" }],
    });
    if let Some(id) = work_item {
        data["work_item"] = json!(id);
    }

    data
}
