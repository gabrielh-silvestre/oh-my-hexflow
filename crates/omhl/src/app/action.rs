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
        "close" => work_item(&mut args).map(|work_item| Action::Close { work_item }),
        "verify" => only(&args, &[]).map(|()| Action::Verify),
        "byproduct" => byproduct(&mut args),
        "project" => only(&args, &[]).map(|()| Action::Project),
        _ => Err(AppError::unknown_tool()),
    }
}

fn list(args: &mut Map<String, Value>) -> Result<Action, AppError> {
    only(args, &["type", "id_prefix", "limit", "after_seq"])?;
    let limit = optional_uint(args, "limit")?;

    Ok(Action::List {
        type_name: optional_text(args, "type")?,
        id_prefix: optional_text(args, "id_prefix")?,
        limit: limit
            .map(|value| {
                u32::try_from(value)
                    .map_err(|_| AppError::invalid_arguments("/limit", "unknown value"))
            })
            .transpose()?,
        after_seq: optional_uint(args, "after_seq")?,
    })
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
    let tail = match optional_uint(args, "tail")? {
        None => DEFAULT_TAIL,
        Some(value) => u32::try_from(value)
            .map_err(|_| AppError::invalid_arguments("/tail", "unknown value"))?,
    };

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
