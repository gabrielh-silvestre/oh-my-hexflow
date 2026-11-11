use std::fmt;

use hexlog::{ErrorCode, HexlogError};
use serde_json::{Value, json};

use crate::error::{OmhlError, RuleCode};
use crate::extensions::model::ExtensionIssue;
use crate::project::ProjectError;
use crate::project::layout::ISSUES_MAX;
use crate::projection::ProjectionError;

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
    context: Option<Value>,
}

impl Detail {
    fn new(path: &str, code: &str, message: &str) -> Detail {
        Detail {
            path: path.to_owned(),
            code: code.to_owned(),
            message: message.to_owned(),
            current: None,
            retired: None,
            context: None,
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
        if let Some(context) = &self.context {
            detail["context"] = context.clone();
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

    pub(crate) fn detail_codes(&self) -> impl Iterator<Item = &str> {
        self.details.iter().map(|detail| detail.code.as_str())
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

    /// Mostra até `ISSUES_MAX` problemas; `omitted` conta os que a coleta já descartou.
    pub(crate) fn extensions_invalid(issues: &[ExtensionIssue], omitted: usize) -> AppError {
        let omitted = omitted + issues.len().saturating_sub(ISSUES_MAX);
        let details: Vec<Detail> = issues
            .iter()
            .take(ISSUES_MAX)
            .map(|issue| Detail::new(&issue.path, issue.code, &issue.message))
            .collect();
        let message = format!(
            "extensions are invalid: {} error(s) shown, {omitted} omitted; nothing was loaded",
            details.len()
        );

        AppError {
            code: "extensions-invalid".to_owned(),
            message,
            details,
        }
    }

    /// Um único detalhe com o caminho, o código e a mensagem do erro de projeção.
    pub(crate) fn projection_failed(error: &ProjectionError) -> AppError {
        AppError {
            code: "projection-failed".to_owned(),
            message: "the projection could not be written".to_owned(),
            details: vec![Detail::new(&error.path, error.code, &error.message)],
        }
    }

    /// Troca o ponteiro `/items/<i>/...` de cada detalhe pelo arquivo `files[i]`.
    pub(crate) fn remap_item_paths(mut self, files: &[String]) -> AppError {
        for detail in &mut self.details {
            if let Some(file) = item_index(&detail.path).and_then(|index| files.get(index)) {
                detail.path.clone_from(file);
            }
        }

        self
    }

    pub(crate) fn with_detail(mut self, path: &str, code: &str, message: &str) -> AppError {
        self.details.push(Detail::new(path, code, message));

        self
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

    pub(crate) fn checks_failed(details: Vec<CheckDetail>) -> AppError {
        AppError::per_check("checks-failed", "a blocking check failed", details)
    }

    pub(crate) fn check_not_run(details: Vec<CheckDetail>) -> AppError {
        AppError::per_check(
            "check-not-run",
            "a check could not run to a verdict",
            details,
        )
    }

    pub(crate) fn attestation_refused(details: Vec<CheckDetail>) -> AppError {
        AppError::per_check(
            "attestation-refused",
            "the guard refused an attestation",
            details,
        )
    }

    pub(crate) fn attestation_not_recorded(details: Vec<CheckDetail>) -> AppError {
        AppError::per_check(
            "attestation-not-recorded",
            "a verdict was computed but its attestation was not recorded",
            details,
        )
    }

    pub(crate) fn unsupported_platform(details: Vec<CheckDetail>) -> AppError {
        AppError::per_check(
            "unsupported-platform",
            "checks cannot run on this platform",
            details,
        )
    }

    fn per_check(code: &str, message: &str, details: Vec<CheckDetail>) -> AppError {
        let details = details
            .into_iter()
            .map(|detail| Detail {
                context: detail.context,
                ..Detail::new(&detail.check, detail.code, &detail.message)
            })
            .collect();

        AppError {
            code: code.to_owned(),
            message: message.to_owned(),
            details,
        }
    }

    pub(crate) fn check_not_mechanical(path: &str) -> AppError {
        AppError::at_pointer("check-not-mechanical", path, "the check has no command")
    }

    pub(crate) fn branch_name_invalid(path: &str) -> AppError {
        AppError::at_pointer(
            "branch-name-invalid",
            path,
            "the branch name does not name a work item",
        )
    }

    pub(crate) fn head_ambiguous(path: &str) -> AppError {
        AppError::at_pointer("head-ambiguous", path, "the branch of HEAD is ambiguous")
    }

    pub(crate) fn branch_protected(path: &str) -> AppError {
        AppError::at_pointer("branch-protected", path, "the branch is protected")
    }

    pub(crate) fn develop_mark_missing(path: &str) -> AppError {
        AppError::at_pointer(
            "develop-mark-missing",
            path,
            "a commit on develop needs the develop mark",
        )
    }

    pub(crate) fn push_ref_not_checked_out(path: &str) -> AppError {
        AppError::at_pointer(
            "push-ref-not-checked-out",
            path,
            "a pushed ref is not the checked-out HEAD",
        )
    }

    pub(crate) fn push_input_invalid(path: &str) -> AppError {
        AppError::at_pointer("push-input-invalid", path, "the pre-push input is invalid")
    }

    pub(crate) fn worktree_dirty(path: &str) -> AppError {
        AppError::at_pointer(
            "worktree-dirty",
            path,
            "the working tree has uncommitted changes",
        )
    }

    pub(crate) fn worktree_differs_from_index(path: &str) -> AppError {
        AppError::at_pointer(
            "worktree-differs-from-index",
            path,
            "the working tree differs from the index",
        )
    }

    pub(crate) fn attestation_not_found(path: &str) -> AppError {
        AppError::at_pointer(
            "attestation-not-found",
            path,
            "no attestation exists for this check and work item",
        )
    }

    pub(crate) fn byproduct_missing(path: &str) -> AppError {
        AppError::at_pointer(
            "byproduct-missing",
            path,
            "the attestation has no stored output",
        )
    }

    pub(crate) fn hooks_existing(path: &str) -> AppError {
        AppError::at_pointer(
            "hooks-existing",
            path,
            "the repository already has hooks that would be disabled",
        )
    }

    pub(crate) fn hooks_path_conflict(path: &str) -> AppError {
        AppError::at_pointer(
            "hooks-path-conflict",
            path,
            "core.hooksPath already points elsewhere",
        )
    }

    pub(crate) fn hooks_path_unreachable(path: &str) -> AppError {
        AppError::at_pointer(
            "hooks-path-unreachable",
            path,
            "the hook does not resolve through core.hooksPath",
        )
    }

    pub(crate) fn hooks_dir_unwritable(path: &str) -> AppError {
        AppError::at_pointer(
            "hooks-dir-unwritable",
            path,
            "the hooks folder cannot be written",
        )
    }

    /// Único caminho, além de `CheckDetail.context`, por onde `context` entra num detalhe; nunca é gravado.
    pub(crate) fn with_context(mut self, context: Value) -> AppError {
        if let Some(detail) = self.details.last_mut() {
            detail.context = Some(context);
        }

        self
    }
}

/// Um detalhe por check; o `path` do detalhe é o id do check.
pub(crate) struct CheckDetail {
    pub check: String,
    pub code: &'static str,
    pub message: String,
    pub context: Option<Value>,
}

fn item_index(path: &str) -> Option<usize> {
    path.strip_prefix("/items/")?
        .split('/')
        .next()?
        .parse()
        .ok()
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
                context: None,
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
