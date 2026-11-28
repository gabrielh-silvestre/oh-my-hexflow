use serde_json::Value;

use crate::delivery_gate::REQUIRED_EVENTS;

pub fn check_loosens(previous: &Value, revised: &Value) -> bool {
    let blocking = |data: &Value| data["blocking"] == true;
    let required_event = |data: &Value| {
        data["when"]["event"]
            .as_str()
            .is_some_and(|event| REQUIRED_EVENTS.contains(&event))
    };

    (blocking(previous) && !blocking(revised))
        || (!previous["run"].is_null() && revised["run"].is_null())
        || (blocking(previous)
            && blocking(revised)
            && required_event(previous)
            && !required_event(revised))
}
