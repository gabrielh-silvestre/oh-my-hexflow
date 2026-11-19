use serde_json::json;

use crate::adapters::text::{CommandKey, Layout, render_success};

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

#[test]
fn it_should_print_the_sweep_counts_alone_when_nothing_was_swept() {
    let value = json!({ "scan_timed_out": false, "swept": [], "unreadable": 3 });

    let text = render_success(CommandKey::CheckSweep, &value, Layout::Agent);

    assert_eq!(text, "✔ check sweep · 0 swept · 3 unreadable");
}

#[test]
fn it_should_warn_and_ask_for_another_sweep_when_the_scan_timed_out() {
    let value = json!({ "scan_timed_out": true, "swept": [], "unreadable": 0 });

    let text = render_success(CommandKey::CheckSweep, &value, Layout::Agent);

    assert_eq!(
        lines_of(&text),
        [
            "⚠ check sweep · 0 swept · 0 unreadable · scan timed out",
            "Next: run omhl check sweep again, an empty swept proves nothing after a timeout.",
        ]
    );
}

#[test]
fn it_should_print_a_swept_process_as_a_dash_item_with_pid_and_owner() {
    let value = json!({
        "scan_timed_out": false,
        "swept": [{ "comm": "sleep", "owner_pid": 4100, "pid": 4242 }],
        "unreadable": 0,
    });

    let text = render_success(CommandKey::CheckSweep, &value, Layout::Agent);

    assert_eq!(
        lines_of(&text),
        [
            "✔ check sweep · 1 swept · 0 unreadable",
            "- sleep · pid 4242 · owner 4100",
        ]
    );
}
