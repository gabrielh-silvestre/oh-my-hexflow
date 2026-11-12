use std::ops::RangeInclusive;

use hexlog::chain::{canonical, sha256hex};
use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value, json};

/// Só `Failed` não é `ok`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    PassedWithWarning,
    Failed,
}

/// No máximo um padrão por regra: os dois fluxos no TOML são erro no load.
#[derive(Debug, Clone)]
pub enum StreamPattern {
    Stdout(Regex),
    Stderr(Regex),
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub exit: i32,
    pub pattern: Option<StreamPattern>,
    pub outcome: Outcome,
}

/// Sem campo de snapshot: ele sai de `snapshot_for`.
#[derive(Debug, Clone)]
pub struct CheckSpec {
    pub run: Vec<String>,
    pub timeout_secs: u32,
    pub expect: Vec<Rule>,
}

/// Nunca ecoa o conteúdo recusado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError {
    pub path: String,
    pub code: &'static str,
}

/// Só mede check com `run`.
pub const CHECK_DATA_MAX_UNITS: usize = 15_000;
pub const RUN_MAX_UNITS: usize = 6_000;
pub const RUNNABLE_EVENTS: [&str; 3] = ["before-commit", "before-pr", "before-merge"];

/// Teto de 1 MiB, menor que os 10 MiB padrão do crate.
const REGEX_SIZE_LIMIT: usize = 1 << 20;
const RUN_MAX_ITEMS: usize = 64;
const RUN_ITEM_MAX_CHARS: usize = 1_000;
const TIMEOUT_SECS_RANGE: RangeInclusive<u32> = 1..=7_200;
const EXPECT_MAX_RULES: usize = 16;

impl CheckSpec {
    /// `None` é check só de texto. Devolve o primeiro erro, na ordem dos campos do check.
    pub fn from_data(data: &Value) -> Result<Option<CheckSpec>, SpecError> {
        if !is_mechanical(data) {
            return Ok(None);
        }
        let run = &data["run"];
        check_event(data)?;
        check_size(data, "", CHECK_DATA_MAX_UNITS, "data-too-large")?;
        check_size(run, "/run", RUN_MAX_UNITS, "run-too-large")?;

        Ok(Some(CheckSpec {
            run: decode_run(run)?,
            timeout_secs: decode_timeout(data.get("timeout_secs"))?,
            expect: decode_expect(data.get("expect"))?,
        }))
    }
}

fn spec_error(path: impl Into<String>, code: &'static str) -> SpecError {
    SpecError {
        path: path.into(),
        code,
    }
}

fn check_event(data: &Value) -> Result<(), SpecError> {
    let event = data.pointer("/when/event").and_then(Value::as_str);
    if event.is_some_and(|event| RUNNABLE_EVENTS.contains(&event)) {
        Ok(())
    } else {
        Err(spec_error("/when/event", "event-unsupported"))
    }
}

/// Mede em unidades UTF-16 do JSON canônico, a mesma régua do hexlog.
fn check_size(
    value: &Value,
    path: &str,
    max_units: usize,
    code: &'static str,
) -> Result<(), SpecError> {
    let canonical = canonical(value).map_err(|_| spec_error(path, "shape-invalid"))?;
    if canonical.encode_utf16().count() > max_units {
        Err(spec_error(path, code))
    } else {
        Ok(())
    }
}

fn decode_run(run: &Value) -> Result<Vec<String>, SpecError> {
    let items = run
        .as_array()
        .ok_or_else(|| spec_error("/run", "shape-invalid"))?;
    let argv = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| spec_error(format!("/run/{index}"), "shape-invalid"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if argv.is_empty() {
        return Err(spec_error("/run", "run-empty"));
    }
    if argv.len() > RUN_MAX_ITEMS {
        return Err(spec_error("/run", "run-too-many-items"));
    }
    if let Some(index) = argv
        .iter()
        .position(|item| item.chars().count() > RUN_ITEM_MAX_CHARS)
    {
        return Err(spec_error(format!("/run/{index}"), "run-item-too-long"));
    }

    Ok(argv)
}

fn decode_timeout(timeout: Option<&Value>) -> Result<u32, SpecError> {
    const PATH: &str = "/timeout_secs";
    let timeout = timeout.ok_or_else(|| spec_error(PATH, "timeout-missing"))?;
    if !timeout.is_i64() && !timeout.is_u64() {
        return Err(spec_error(PATH, "shape-invalid"));
    }

    timeout
        .as_u64()
        .and_then(|secs| u32::try_from(secs).ok())
        .filter(|secs| TIMEOUT_SECS_RANGE.contains(secs))
        .ok_or_else(|| spec_error(PATH, "timeout-out-of-range"))
}

fn decode_expect(expect: Option<&Value>) -> Result<Vec<Rule>, SpecError> {
    const PATH: &str = "/expect";
    let rules = expect
        .ok_or_else(|| spec_error(PATH, "expect-missing"))?
        .as_array()
        .ok_or_else(|| spec_error(PATH, "shape-invalid"))?;
    if rules.is_empty() {
        return Err(spec_error(PATH, "expect-empty"));
    }
    if rules.len() > EXPECT_MAX_RULES {
        return Err(spec_error(PATH, "expect-too-many-rules"));
    }

    rules
        .iter()
        .enumerate()
        .map(|(index, rule)| decode_rule(rule, &format!("{PATH}/{index}")))
        .collect()
}

fn decode_rule(rule: &Value, rule_path: &str) -> Result<Rule, SpecError> {
    let fields = rule
        .as_object()
        .ok_or_else(|| spec_error(rule_path, "shape-invalid"))?;
    let exit = fields
        .get("exit")
        .ok_or_else(|| spec_error(format!("{rule_path}/exit"), "exit-missing"))?
        .as_i64()
        .and_then(|code| i32::try_from(code).ok())
        .ok_or_else(|| spec_error(format!("{rule_path}/exit"), "shape-invalid"))?;
    let outcome = fields
        .get("outcome")
        .and_then(Value::as_str)
        .and_then(parse_outcome)
        .ok_or_else(|| spec_error(format!("{rule_path}/outcome"), "shape-invalid"))?;

    Ok(Rule {
        exit,
        pattern: decode_pattern(fields, rule_path)?,
        outcome,
    })
}

fn parse_outcome(text: &str) -> Option<Outcome> {
    match text {
        "passed" => Some(Outcome::Passed),
        "passed_with_warning" => Some(Outcome::PassedWithWarning),
        "failed" => Some(Outcome::Failed),
        _ => None,
    }
}

fn decode_pattern(
    fields: &Map<String, Value>,
    rule_path: &str,
) -> Result<Option<StreamPattern>, SpecError> {
    let stdout = pattern_text(fields, "stdout", rule_path)?;
    let stderr = pattern_text(fields, "stderr", rule_path)?;
    match (stdout, stderr) {
        (Some(_), Some(_)) => Err(spec_error(rule_path, "rule-two-streams")),
        (Some(pattern), None) => {
            compile(pattern, rule_path, "stdout").map(|regex| Some(StreamPattern::Stdout(regex)))
        }
        (None, Some(pattern)) => {
            compile(pattern, rule_path, "stderr").map(|regex| Some(StreamPattern::Stderr(regex)))
        }
        (None, None) => Ok(None),
    }
}

fn pattern_text<'a>(
    fields: &'a Map<String, Value>,
    stream: &str,
    rule_path: &str,
) -> Result<Option<&'a str>, SpecError> {
    fields
        .get(stream)
        .map(|pattern| {
            pattern
                .as_str()
                .ok_or_else(|| spec_error(format!("{rule_path}/{stream}"), "shape-invalid"))
        })
        .transpose()
}

/// A mensagem do `regex::Error` ecoa o padrão, por isso só o código sai daqui.
fn compile(pattern: &str, rule_path: &str, stream: &str) -> Result<Regex, SpecError> {
    RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|error| {
            let code = match error {
                regex::Error::CompiledTooBig(_) => "regex-too-large",
                _ => "regex-invalid",
            };
            spec_error(format!("{rule_path}/{stream}"), code)
        })
}

/// Único critério de "check mecânico": a chave `run` presente, qualquer que seja o valor.
/// Guarda, gate e `from_data` o usam para que a abertura de um deles não divirja dos outros.
pub fn is_mechanical(data: &Value) -> bool {
    data.get("run").is_some()
}

/// Único decodificador do tipo de snapshot de um check. Não passa pelo
/// `from_data`: um spec inválido também tem kind (AF-MINOR 22).
pub fn snapshot_for(data: &Value) -> hexlog::SnapshotKind {
    let before_commit =
        data.pointer("/when/event").and_then(Value::as_str) == Some("before-commit");
    if before_commit && is_mechanical(data) {
        hexlog::SnapshotKind::GitTree
    } else {
        hexlog::SnapshotKind::GitCommit
    }
}

/// Impressão estável da definição mecânica de um check: só `run`, `timeout_secs` e `expect`.
/// Fonte única do `check run` (grava e compara) e do gate (confere).
pub fn spec_digest(data: &Value) -> String {
    let definition = json!({
        "run": data.get("run"),
        "timeout_secs": data.get("timeout_secs"),
        "expect": data.get("expect"),
    });
    // o `data` do banco já passou pela régua do hexlog, então `canonical` não falha aqui
    sha256hex(&canonical(&definition).unwrap_or_default())
        .as_str()
        .to_owned()
}
