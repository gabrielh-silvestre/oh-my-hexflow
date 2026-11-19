use crate::support;

use omhl::extensions::model::{ExtensionKind, RawExtension};
use omhl::extensions::parse::parse;
use serde_json::Value;
use support::appfx::AppFixture;

#[test]
fn it_should_give_every_new_app_refusal_and_extension_issue_a_next_step() {
    // Lista fechada: cada fatia que cria uma recusa do app ou um issue de extension a acrescenta aqui.
    let tomls = [
        ("kind-missing", "text = \"x\"\n"),
        ("kind-invalid", "kind = \"x\"\ntext = \"x\"\n"),
    ];

    let incomplete: Vec<&str> = tomls
        .iter()
        .filter(|(code, toml)| {
            let raw = RawExtension {
                kind: ExtensionKind::Guideline,
                slug: "tone".to_owned(),
                toml_path: ".omhl/guidelines/tone.toml".to_owned(),
                toml: (*toml).to_owned(),
                markdown_path: None,
                markdown: None,
            };
            let issues = parse(&raw).expect_err("the extension is refused");

            !issues.iter().any(|issue| {
                issue.code == *code
                    && !issue.message.is_empty()
                    && issue
                        .next_step
                        .is_some_and(|guidance| !guidance.next_step.is_empty())
            })
        })
        .map(|(code, _)| *code)
        .collect();
    let retire_codes = [
        "guideline-not-found",
        "guideline-not-orphaned",
        "successor-not-current",
        "successor-is-self",
        "supersedes-cycle",
    ];
    let refusals = AppFixture::new().guideline_retire_refusals();
    let filled = |value: &Value| value.as_str().is_some_and(|text| !text.is_empty());
    let retire_incomplete: Vec<&str> = retire_codes
        .iter()
        .zip(&refusals)
        .filter(|(code, refusal)| {
            let error = &refusal["error"];

            !(error["code"] == **code
                && filled(&error["message"])
                && filled(&error["details"][0]["next_step"]))
        })
        .map(|(code, _)| *code)
        .collect();

    assert_eq!(
        (incomplete, retire_incomplete),
        (Vec::<&str>::new(), Vec::<&str>::new())
    );
}
