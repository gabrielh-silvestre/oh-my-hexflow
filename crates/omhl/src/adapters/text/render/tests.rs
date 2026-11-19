use serde_json::json;

use super::*;

#[test]
fn it_should_drop_controls_but_keep_line_breaks_and_tabs_in_the_context_text() {
    let value = json!({ "text": "a\n\tb\u{1b}]0;x\u{7}c\r\u{85}d" });

    let doc = Render.success(&value);

    assert_eq!(
        doc.lines,
        vec![Line::Raw {
            depth: 0,
            text: "a\n\tb]0;xcd".to_owned()
        }]
    );
}
