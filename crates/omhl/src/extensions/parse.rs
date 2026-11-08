use hexlog::SemanticId;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use super::model::{Extension, ExtensionIssue, ExtensionKind, RawExtension};
use crate::project::text::{
    is_invisible_format, normalize_line_breaks, normalize_line_breaks_owned, toml_error_position,
};

const HOW_MAX_CHARS: usize = 8_000;
const TEXT_MAX_CHARS: usize = 2_000;
const EVENT_MAX_CHARS: usize = 100;
const MATCHER_MAX_CHARS: usize = 200;
const AFTER_MAX_ITEMS: usize = 16;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckToml {
    blocking: bool,
    how: Option<String>,
    #[serde(default)]
    after: Vec<String>,
    when: WhenToml,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WhenToml {
    event: String,
    matcher: Option<String>,
    vocabulary: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuidelineToml {
    text: Option<String>,
}

pub fn parse(raw: &RawExtension) -> Result<Extension, Vec<ExtensionIssue>> {
    match raw.kind {
        ExtensionKind::Check => parse_check(raw),
        ExtensionKind::Guideline => parse_guideline(raw),
    }
}

fn parse_check(raw: &RawExtension) -> Result<Extension, Vec<ExtensionIssue>> {
    let parsed: CheckToml = parse_toml(raw)?;
    let id = format!("{}{}", raw.kind.id_prefix(), raw.slug);
    let mut issues = Vec::new();

    let how = keep(
        &mut issues,
        text_field(raw, parsed.how, "how", HOW_MAX_CHARS),
    );
    let when = when_data(&raw.toml_path, parsed.when, &mut issues);
    check_after(&raw.toml_path, &id, &parsed.after, &mut issues);

    let (Some(how), Some(when), true) = (how, when, issues.is_empty()) else {
        return Err(issues);
    };

    Ok(Extension {
        kind: raw.kind,
        id,
        path: raw.toml_path.clone(),
        data: Value::Object(Map::from_iter([
            ("blocking".to_owned(), Value::Bool(parsed.blocking)),
            ("how".to_owned(), Value::String(how)),
            ("when".to_owned(), when),
        ])),
        after: parsed.after,
    })
}

/// `None` quando algum campo de `[when]` foi recusado (o motivo vai para `issues`).
fn when_data(path: &str, when: WhenToml, issues: &mut Vec<ExtensionIssue>) -> Option<Value> {
    let event = keep(
        issues,
        short_field(path, "event", when.event, EVENT_MAX_CHARS),
    );
    let matcher = when
        .matcher
        .map(|matcher| short_field(path, "matcher", matcher, MATCHER_MAX_CHARS))
        .transpose();
    let matcher = keep(issues, matcher);
    if when.vocabulary.as_deref().is_some_and(|v| v != "free") {
        issues.push(issue(
            path,
            "value-not-allowed",
            "when.vocabulary must be \"free\"".to_owned(),
        ));
    }

    let mut data = Map::from_iter([
        ("vocabulary".to_owned(), Value::String("free".to_owned())),
        ("event".to_owned(), Value::String(event?)),
    ]);
    if let Some(matcher) = matcher? {
        data.insert("matcher".to_owned(), Value::String(matcher));
    }

    Some(Value::Object(data))
}

fn parse_guideline(raw: &RawExtension) -> Result<Extension, Vec<ExtensionIssue>> {
    let parsed: GuidelineToml = parse_toml(raw)?;
    let text = text_field(raw, parsed.text, "text", TEXT_MAX_CHARS).map_err(|found| vec![found])?;

    Ok(Extension {
        kind: raw.kind,
        id: format!("{}{}", raw.kind.id_prefix(), raw.slug),
        path: raw.toml_path.clone(),
        data: Value::Object(Map::from_iter([("text".to_owned(), Value::String(text))])),
        after: Vec::new(),
    })
}

fn issue(path: &str, code: &'static str, message: String) -> ExtensionIssue {
    ExtensionIssue {
        path: path.to_owned(),
        code,
        message,
    }
}

fn keep<T>(issues: &mut Vec<ExtensionIssue>, result: Result<T, ExtensionIssue>) -> Option<T> {
    result.map_err(|found| issues.push(found)).ok()
}

fn parse_toml<T: DeserializeOwned>(raw: &RawExtension) -> Result<T, Vec<ExtensionIssue>> {
    toml::from_str(&raw.toml).map_err(|error| {
        let (line, column) = toml_error_position(&raw.toml, &error);
        vec![issue(
            &raw.toml_path,
            "invalid-toml",
            format!("not valid TOML at line {line}, column {column}"),
        )]
    })
}

/// Texto longo: vem da chave do TOML ou do `.md` irmão, nunca dos dois.
fn text_field(
    raw: &RawExtension,
    inline: Option<String>,
    name: &str,
    max_chars: usize,
) -> Result<String, ExtensionIssue> {
    let (path, text) = match (inline, raw.markdown.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(issue(
                &raw.toml_path,
                "text-conflict",
                format!("`{name}` is set in both the TOML and the markdown file"),
            ));
        }
        (Some(inline), None) => (&raw.toml_path, normalize_line_breaks_owned(inline)),
        (None, Some(markdown)) => {
            let path = raw.markdown_path.as_ref().unwrap_or(&raw.toml_path);
            let mut text = normalize_line_breaks(markdown).into_owned();
            text.truncate(text.trim_end_matches('\n').len());
            (path, text)
        }
        (None, None) => (&raw.toml_path, String::new()),
    };
    if text.is_empty() {
        return Err(issue(path, "text-missing", format!("`{name}` is empty")));
    }

    checked(path, name, text, max_chars)
}

/// Campo curto (`event`, `matcher`): vazio não vale.
fn short_field(
    path: &str,
    name: &str,
    value: String,
    max_chars: usize,
) -> Result<String, ExtensionIssue> {
    if value.is_empty() {
        return Err(issue(
            path,
            "value-not-allowed",
            format!("`{name}` is empty"),
        ));
    }

    checked(path, name, normalize_line_breaks_owned(value), max_chars)
}

fn checked(
    path: &str,
    name: &str,
    text: String,
    max_chars: usize,
) -> Result<String, ExtensionIssue> {
    if text
        .chars()
        .any(|c| (c.is_control() && c != '\n' && c != '\t') || is_invisible_format(c))
    {
        return Err(issue(
            path,
            "value-not-allowed",
            format!("`{name}` has a control or invisible character"),
        ));
    }
    if text.chars().count() > max_chars {
        return Err(issue(
            path,
            "value-too-long",
            format!("`{name}` is longer than {max_chars} characters"),
        ));
    }

    Ok(text)
}

fn check_after(path: &str, own_id: &str, after: &[String], issues: &mut Vec<ExtensionIssue>) {
    if after.len() > AFTER_MAX_ITEMS {
        return issues.push(issue(
            path,
            "value-too-long",
            format!("`after` has more than {AFTER_MAX_ITEMS} items"),
        ));
    }
    for (index, target) in after.iter().enumerate() {
        let is_check_id = SemanticId::parse(target).is_ok()
            && target
                .strip_prefix(ExtensionKind::Check.id_prefix())
                .is_some_and(|slug| !slug.is_empty());
        let reason = if !is_check_id {
            "is not a check id"
        } else if target == own_id {
            "points at the check itself"
        } else if after[..index].contains(target) {
            "is repeated"
        } else {
            continue;
        };
        issues.push(issue(
            path,
            "value-not-allowed",
            format!("`after[{index}]` {reason}"),
        ));
    }
}
