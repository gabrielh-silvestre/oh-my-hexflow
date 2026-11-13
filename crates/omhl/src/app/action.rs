use hexlog::Author;
use serde_json::{Map, Value};

use super::error::AppError;

const DEFAULT_TAIL: u32 = 50;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    List {
        type_name: Option<String>,
        id_prefix: Option<String>,
        limit: Option<u32>,
        after_seq: Option<u64>,
        where_eq: Vec<(String, String)>,
    },
    Get {
        id: String,
    },
    Append {
        request: Value,
    },
    Attest {
        check: String,
        work_item: String,
        outcome: String,
        note: Option<String>,
    },
    Render,
    GatePr {
        work_item: String,
    },
    /// Tratada pelo `Runtime`, não pelo `Engine`: o veredito costuma sair sem abrir o banco.
    GateWorktree {
        branch: String,
    },
    Close {
        work_item: String,
    },
    Verify,
    Load {
        dry_run: bool,
    },
    Project,
    /// Sem work item no pedido: ele vem da branch. `pre_push` é a stdin do git com `--pre-push`.
    CheckRun {
        target: CheckTarget,
        pre_push: Option<String>,
    },
    /// Aposenta o node de um check órfão.
    CheckRetire {
        check: String,
    },
    ConsumeDevelopMark,
    Byproduct {
        check: String,
        work_item: String,
        tail: u32,
    },
    /// Tratada pelo `Runtime`, não pelo `Engine`.
    CheckSweep,
    HooksInstall {
        force: bool,
    },
    SpecInit {
        work_item: String,
        slug: String,
    },
    SpecValidate {
        work_item: String,
    },
    ReportAnswers {
        work_item: Option<String>,
    },
    /// Sem `min`, vale `promotion.min_work_items` do `workflow.toml`.
    ReportRecurrences {
        min: Option<u32>,
    },
    ReportRetro {
        work_item: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckTarget {
    Event(String),
    Check(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    pub client: &'static str,
    pub agent: &'static str,
}

impl Caller {
    pub fn cli() -> Caller {
        Caller {
            client: "cli",
            agent: "user",
        }
    }

    /// O Claude Code define `CLAUDECODE=1` nas chamadas de Bash; o falso positivo (humano) é o erro seguro.
    pub fn cli_under_claude_code() -> Caller {
        Caller {
            client: "cli",
            agent: "claude",
        }
    }

    pub fn mcp() -> Caller {
        Caller {
            client: "mcp",
            agent: "claude",
        }
    }

    pub fn hook() -> Caller {
        Caller {
            client: "hook",
            agent: "hook",
        }
    }

    pub fn loader() -> Caller {
        Caller {
            client: "loader",
            agent: "loader",
        }
    }

    /// Só o `check run` grava com ele.
    pub fn executor() -> Caller {
        Caller {
            client: "executor",
            agent: "executor",
        }
    }

    /// Sem `model`: o `omhl` não sabe qual modelo está do outro lado.
    pub(crate) fn author(&self) -> Author {
        Author {
            agent: self.agent.to_owned(),
            model: None,
            client: self.client.to_owned(),
        }
    }
}

/// Nome da ferramenta MCP e objeto `arguments` viram uma `Action`.
pub fn parse_tool_call(name: &str, arguments: Value) -> Result<Action, AppError> {
    let mut args = match arguments {
        Value::Null => Map::new(),
        Value::Object(map) => map,
        _ => return Err(AppError::invalid_arguments("", "wrong type")),
    };

    match name {
        "list" => list(&mut args),
        "get" => only(&args, &["id"])
            .and_then(|()| required_text(&mut args, "id"))
            .map(|id| Action::Get { id }),
        "append" => Ok(Action::Append {
            request: Value::Object(args),
        }),
        "attest" => attest(&mut args),
        "render" => only(&args, &[]).map(|()| Action::Render),
        "gate_pr" => work_item(&mut args).map(|work_item| Action::GatePr { work_item }),
        "gate_worktree" => only(&args, &["branch"])
            .and_then(|()| required_text(&mut args, "branch"))
            .map(|branch| Action::GateWorktree { branch }),
        "close" => work_item(&mut args).map(|work_item| Action::Close { work_item }),
        "verify" => only(&args, &[]).map(|()| Action::Verify),
        "byproduct" => byproduct(&mut args),
        "project" => only(&args, &[]).map(|()| Action::Project),
        "report_answers" => only(&args, &["work_item"]).and_then(|()| {
            Ok(Action::ReportAnswers {
                work_item: optional_text(&mut args, "work_item")?,
            })
        }),
        "report_recurrences" => only(&args, &["min"]).and_then(|()| {
            Ok(Action::ReportRecurrences {
                min: optional_u32(&args, "min")?,
            })
        }),
        "report_retro" => work_item(&mut args).map(|work_item| Action::ReportRetro { work_item }),
        _ => Err(AppError::unknown_tool()),
    }
}

fn list(args: &mut Map<String, Value>) -> Result<Action, AppError> {
    only(args, &["type", "id_prefix", "limit", "after_seq", "where"])?;
    let limit = optional_u32(args, "limit")?;

    Ok(Action::List {
        type_name: optional_text(args, "type")?,
        id_prefix: optional_text(args, "id_prefix")?,
        limit,
        after_seq: optional_uint(args, "after_seq")?,
        where_eq: where_pairs(args)?,
    })
}

fn where_pairs(args: &mut Map<String, Value>) -> Result<Vec<(String, String)>, AppError> {
    match args.remove("where") {
        None => Ok(Vec::new()),
        Some(Value::Object(fields)) => fields
            .into_iter()
            .map(|(field, value)| match value {
                Value::String(text) => Ok((field, text)),
                _ => Err(AppError::invalid_arguments(
                    &format!("/where/{field}"),
                    "wrong type",
                )),
            })
            .collect(),
        Some(_) => Err(AppError::invalid_arguments("/where", "wrong type")),
    }
}

fn attest(args: &mut Map<String, Value>) -> Result<Action, AppError> {
    only(args, &["check", "work_item", "outcome", "note"])?;

    Ok(Action::Attest {
        check: required_text(args, "check")?,
        work_item: required_text(args, "work_item")?,
        outcome: required_text(args, "outcome")?,
        note: optional_text(args, "note")?,
    })
}

fn byproduct(args: &mut Map<String, Value>) -> Result<Action, AppError> {
    only(args, &["check", "work_item", "tail"])?;
    let tail = optional_u32(args, "tail")?.unwrap_or(DEFAULT_TAIL);

    Ok(Action::Byproduct {
        check: required_text(args, "check")?,
        work_item: required_text(args, "work_item")?,
        tail,
    })
}

fn work_item(args: &mut Map<String, Value>) -> Result<String, AppError> {
    only(args, &["work_item"])?;

    required_text(args, "work_item")
}

/// Chave fora do esquema da ferramenta é recusada.
fn only(args: &Map<String, Value>, allowed: &[&str]) -> Result<(), AppError> {
    match args.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(AppError::invalid_arguments(
            &format!("/{}", hexlog::pointer_segment(key)),
            "unknown argument",
        )),
        None => Ok(()),
    }
}

fn pointer(key: &str) -> String {
    format!("/{key}")
}

fn optional_text(args: &mut Map<String, Value>, key: &str) -> Result<Option<String>, AppError> {
    match args.remove(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(AppError::invalid_arguments(&pointer(key), "wrong type")),
    }
}

fn required_text(args: &mut Map<String, Value>, key: &str) -> Result<String, AppError> {
    optional_text(args, key)?
        .ok_or_else(|| AppError::invalid_arguments(&pointer(key), "missing argument"))
}

fn optional_uint(args: &Map<String, Value>, key: &str) -> Result<Option<u64>, AppError> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };

    match value.as_u64() {
        Some(number) => Ok(Some(number)),
        None => Err(AppError::invalid_arguments(&pointer(key), "wrong type")),
    }
}

fn optional_u32(args: &Map<String, Value>, key: &str) -> Result<Option<u32>, AppError> {
    optional_uint(args, key)?
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| AppError::invalid_arguments(&pointer(key), "unknown value"))
        })
        .transpose()
}
