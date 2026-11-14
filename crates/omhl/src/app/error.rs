use std::fmt;

use hexlog::{ErrorCode, HexlogError};
use serde_json::{Value, json};

use crate::error::{OmhlError, RuleCode};
use crate::extensions::model::ExtensionIssue;
use crate::guidance::Guidance;
use crate::project::ProjectError;
use crate::project::layout::ISSUES_MAX;
use crate::projection::ProjectionError;
use crate::workflow::spec::Violation;

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
    next_step: Option<String>,
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
            next_step: None,
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
        if let Some(next_step) = &self.next_step {
            detail["next_step"] = json!(next_step);
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

    /// Erro sem detalhes. Privado: todo código estável tem construtor nomeado; só o código dinâmico de `ProjectError` passa por aqui direto.
    fn plain(code: &str, message: &str) -> AppError {
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

    pub(crate) fn input_too_large(limit: usize) -> AppError {
        AppError::plain(
            "input-too-large",
            &format!("input is larger than {limit} bytes"),
        )
    }

    pub(crate) fn input_not_utf8() -> AppError {
        AppError::plain("input-not-utf8", "input is not valid UTF-8")
    }

    pub(crate) fn input_too_deep() -> AppError {
        AppError::plain("input-too-deep", "input is nested too deeply")
    }

    pub(crate) fn input_invalid_json(line: usize, column: usize) -> AppError {
        AppError::plain(
            "input-invalid-json",
            &format!("input is not valid JSON at line {line} column {column}"),
        )
    }

    pub(crate) fn input_unreadable() -> AppError {
        AppError::plain("input-unreadable", "input could not be read")
    }

    pub(crate) fn invalid_hook_input() -> AppError {
        AppError::plain(
            "invalid-hook-input",
            "hook input must be a JSON object with an absolute string cwd",
        )
    }

    pub(crate) fn invalid_project_dir() -> AppError {
        AppError::plain(
            "invalid-project-dir",
            "the project directory cannot be resolved",
        )
    }

    pub(crate) fn no_marker() -> AppError {
        AppError::plain(
            "no-marker",
            "no .omhl.toml found in the project directory or any parent; a branch older than the marker needs a rebase onto main",
        )
    }

    pub(crate) fn db_inside_project() -> AppError {
        AppError::plain(
            "db-inside-project",
            "the database folder is inside the project; its blobs/ folder would dirty the attested tree",
        )
    }

    pub(crate) fn no_project_root() -> AppError {
        AppError::plain("no-project-root", "no project root is available")
    }

    pub(crate) fn render_failed() -> AppError {
        AppError::plain("render-failed", "the render result has no text")
    }

    pub(crate) fn unknown_tool() -> AppError {
        AppError::plain("unknown-tool", "unknown tool")
    }

    pub(crate) fn runner_unavailable() -> AppError {
        AppError::plain("runner-unavailable", "no command runner is configured")
    }

    pub(crate) fn byproducts_unavailable() -> AppError {
        AppError::plain("byproducts-unavailable", "no byproduct store is configured")
    }

    pub(crate) fn develop_mark_unavailable() -> AppError {
        AppError::plain(
            "develop-mark-unavailable",
            "the develop mark is not configured",
        )
    }

    pub(crate) fn binary_path_unreadable() -> AppError {
        AppError::plain(
            "unsupported-platform",
            "the path of the omhl binary cannot be read",
        )
    }

    pub(crate) fn process_table_unreadable() -> AppError {
        AppError::plain(
            "unsupported-platform",
            "the process table cannot be read on this platform",
        )
    }

    /// Mostra até `ISSUES_MAX` problemas; `omitted` conta os que a coleta já descartou.
    pub(crate) fn extensions_invalid(issues: &[ExtensionIssue], omitted: usize) -> AppError {
        let omitted = omitted + issues.len().saturating_sub(ISSUES_MAX);
        let details: Vec<Detail> = issues
            .iter()
            .take(ISSUES_MAX)
            .map(|issue| Detail {
                next_step: issue
                    .next_step
                    .map(|guidance| guidance.next_step.to_owned()),
                ..Detail::new(&issue.path, issue.code, &issue.message)
            })
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

    pub(crate) fn check_not_orphaned(path: &str) -> AppError {
        AppError::at_pointer(
            "check-not-orphaned",
            path,
            "the check file still exists in the working tree",
        )
    }

    /// Recusa da camada `app` com causa e próximo passo; o ponteiro é o único dado de entrada que aparece.
    pub(crate) fn refusal(code: &str, pointer: &str, guidance: Guidance) -> AppError {
        AppError {
            code: code.to_owned(),
            message: guidance.cause.to_owned(),
            details: vec![Detail {
                next_step: Some(guidance.next_step.to_owned()),
                ..Detail::new(pointer, code, guidance.cause)
            }],
        }
    }

    fn unguided_rule(code: &str, pointer: &str) -> AppError {
        let (message, details) = match pointer.is_empty() {
            true => (code.to_owned(), Vec::new()),
            false => (
                format!("{code} at {pointer}"),
                vec![Detail::new(pointer, code, code)],
            ),
        };

        AppError {
            code: code.to_owned(),
            message,
            details,
        }
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

    pub(crate) fn byproduct_corrupt(path: &str) -> AppError {
        AppError::at_pointer(
            "byproduct-corrupt",
            path,
            "the stored output could not be read back intact",
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

    pub(crate) fn spec_exists(path: &str) -> AppError {
        AppError::at_pointer(
            "spec-exists",
            path,
            "a spec folder of this work item already exists",
        )
    }

    pub(crate) fn spec_store_unusable(reason: &str) -> AppError {
        AppError::at_pointer("spec-store-unusable", "/work_item", reason)
    }

    pub(crate) fn spec_slug_invalid(path: &str) -> AppError {
        AppError::at_pointer(
            "spec-slug-invalid",
            path,
            "the slug must have 1 to 64 characters of a-z, 0-9 and -, and not start with -",
        )
    }

    pub(crate) fn spec_not_found(path: &str) -> AppError {
        AppError::at_pointer(
            "spec-not-found",
            path,
            "the work item has no spec folder with a part",
        )
    }

    pub(crate) fn prefix_not_allowed(path: &str) -> AppError {
        AppError::at_pointer(
            "prefix-not-allowed",
            path,
            "the branch name must be <type>/wi-<id>__<slug> with a type listed in the workflow prefixes",
        )
    }

    pub(crate) fn main_dir_unknown() -> AppError {
        AppError::plain(
            "main-dir-unknown",
            "the main directory of the repository cannot be resolved",
        )
    }

    /// `context` leva a branch, o prefixo, o work item, os átomos que faltam e o diretório principal.
    pub(crate) fn worktree_gate_refused(path: &str, context: Value) -> AppError {
        AppError::at_pointer(
            "worktree-gate-refused",
            path,
            "the premises the branch prefix requires are missing",
        )
        .with_context(context)
    }

    /// Mostra até `ISSUES_MAX` violações; o conteúdo da spec nunca entra.
    pub(crate) fn spec_invalid(violations: &[Violation]) -> AppError {
        AppError::violations_of("spec-invalid", "spec", violations)
    }

    fn violations_of(code: &str, subject: &str, violations: &[Violation]) -> AppError {
        let omitted = violations.len().saturating_sub(ISSUES_MAX);
        let details: Vec<Detail> = violations
            .iter()
            .take(ISSUES_MAX)
            .map(|violation| {
                let message = format!("the {subject} breaks the rule {}", violation.code);

                Detail::new(&violation.path, violation.code, &message)
            })
            .collect();
        let message = format!(
            "the {subject} is invalid: {} violation(s) shown, {omitted} omitted",
            details.len()
        );

        AppError {
            code: code.to_owned(),
            message,
            details,
        }
    }

    pub(crate) fn amendment_divergence_not_open(path: &str) -> AppError {
        AppError::at_pointer(
            "amendment-divergence-not-open",
            path,
            "only an open divergence can be approved with an amendment",
        )
    }

    pub(crate) fn amendment_not_found(path: &str) -> AppError {
        let message = "the amendment file does not exist in the spec folder of the work item";

        AppError {
            code: "amendment-invalid".to_owned(),
            message: message.to_owned(),
            details: vec![Detail::new(path, "amendment-not-found", message)],
        }
    }

    /// Mostra até `ISSUES_MAX` violações; o conteúdo da emenda nunca entra.
    pub(crate) fn amendment_invalid(violations: &[Violation]) -> AppError {
        AppError::violations_of("amendment-invalid", "amendment", violations)
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
                next_step: None,
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
            OmhlError::Rule { code, pointer } => match code.guidance() {
                Some(guidance) => AppError::refusal(code.as_str(), &pointer, guidance),
                None => AppError::unguided_rule(code.as_str(), &pointer),
            },
        }
    }
}

impl From<ProjectError> for AppError {
    fn from(error: ProjectError) -> AppError {
        AppError::plain(error.code, &error.message)
    }
}
