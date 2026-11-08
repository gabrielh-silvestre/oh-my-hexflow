use std::fmt;

use hexlog::{ErrorCode, HexlogError};
use serde_json::{Value, json};

use crate::error::{OmhlError, RuleCode};
use crate::project::ProjectError;

const BUSY_MESSAGE: &str =
    "database is busy: another omhl process is writing; run the command again";
const STORE_ERROR_MESSAGE: &str = "storage error; run the command again";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Detail {
    path: String,
    code: String,
    message: String,
    current: Option<String>,
    retired: Option<bool>,
}

impl Detail {
    fn new(path: &str, code: &str, message: &str) -> Detail {
        Detail {
            path: path.to_owned(),
            code: code.to_owned(),
            message: message.to_owned(),
            current: None,
            retired: None,
        }
    }

    fn to_json(&self) -> Value {
        let mut detail = json!({ "path": self.path, "code": self.code, "message": self.message });
        if let Some(current) = &self.current {
            detail["current"] = json!(current);
        }
        if let Some(retired) = self.retired {
            detail["retired"] = json!(retired);
        }
        detail
    }
}

/// O texto de erro nunca repete valor de entrada: só códigos, ponteiros e regras.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    code: String,
    message: String,
    details: Vec<Detail>,
}

impl AppError {
    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn to_json(&self) -> Value {
        let details: Vec<Value> = self.details.iter().map(Detail::to_json).collect();

        json!({ "error": { "code": self.code, "message": self.message, "details": details } })
    }

    /// Erro sem detalhes: entrada, projeto e ferramenta desconhecida.
    pub(crate) fn plain(code: &str, message: &str) -> AppError {
        AppError {
            code: code.to_owned(),
            message: message.to_owned(),
            details: Vec::new(),
        }
    }

    /// Erro de entrada com um único detalhe de mesmo código e mesma mensagem.
    pub(crate) fn at_pointer(code: &str, pointer: &str, message: &str) -> AppError {
        AppError {
            code: code.to_owned(),
            message: message.to_owned(),
            details: vec![Detail::new(pointer, code, message)],
        }
    }

    pub(crate) fn append_invalid(pointer: &str, message: &str) -> AppError {
        AppError::at_pointer("append-invalid", pointer, message)
    }

    pub(crate) fn invalid_arguments(pointer: &str, message: &str) -> AppError {
        AppError::at_pointer("invalid-arguments", pointer, message)
    }

    /// A mensagem leva o motivo da sonda; o código e o ponteiro são os da regra.
    pub(crate) fn probe_failed(reason: &str) -> AppError {
        let code = RuleCode::ProbeFailed.as_str();

        AppError {
            code: code.to_owned(),
            message: format!("{code} at /snapshot: {reason}"),
            details: vec![Detail::new("/snapshot", code, code)],
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<HexlogError> for AppError {
    fn from(error: HexlogError) -> AppError {
        let message = match error.code {
            ErrorCode::Busy => BUSY_MESSAGE.to_owned(),
            ErrorCode::StoreError => STORE_ERROR_MESSAGE.to_owned(),
            _ => error.message,
        };
        let details = error
            .details
            .into_iter()
            .map(|detail| Detail {
                path: detail.path,
                code: detail.code,
                message: detail.message,
                current: detail.current.map(|id| id.as_str().to_owned()),
                retired: detail.retired,
            })
            .collect();

        AppError {
            code: error.code.as_str().to_owned(),
            message,
            details,
        }
    }
}

impl From<OmhlError> for AppError {
    fn from(error: OmhlError) -> AppError {
        match error {
            OmhlError::Hexlog(error) => AppError::from(error),
            OmhlError::Rule { code, pointer } => {
                let code = code.as_str();
                let (message, details) = match pointer.is_empty() {
                    true => (code.to_owned(), Vec::new()),
                    false => (
                        format!("{code} at {pointer}"),
                        vec![Detail::new(&pointer, code, code)],
                    ),
                };

                AppError {
                    code: code.to_owned(),
                    message,
                    details,
                }
            }
        }
    }
}

impl From<ProjectError> for AppError {
    fn from(error: ProjectError) -> AppError {
        AppError::plain(error.code, &error.message)
    }
}
