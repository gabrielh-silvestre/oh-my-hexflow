use std::fmt;

use hexlog::{AppendItem, ErrorCode, HexlogError, ItemBody};
use serde_json::{Value, json};

use crate::delivery_gate::CloseRefusal;
use crate::error::{OmhlError, RuleCode};
use crate::extensions::model::ExtensionIssue;
use crate::facade::CloseError;
use crate::gate::QuestionClass;
use crate::guidance::Guidance;
use crate::project::ProjectError;
use crate::project::layout::ISSUES_MAX;
use crate::projection::ProjectionError;
use crate::workflow::branch_policy::BranchFormat;
use crate::workflow::spec::Violation;

const BUSY_MESSAGE: &str =
    "database is busy: another omhl process is writing; run the command again";
const STORE_ERROR_MESSAGE: &str = "storage error; run the command again";

const CYCLE_GUIDANCE: Guidance = Guidance {
    cause: "the relation would close a cycle: its target already leads back to its source",
    next_step: "drop this relation, or retire the existing link that closes the loop",
};
const ENDPOINT_GUIDANCE: Guidance = Guidance {
    cause: "a relation endpoint is not a current node",
    next_step: "append the missing node before the relation, in the same batch or an earlier one, or fix the id",
};
const GUIDED_RELATION_TYPES: [&str; 2] = ["rests-on", "supersedes"];

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

const CLOSE_REFUSED_MESSAGE: &str = "a refuse question failed, and --force does not override it: fix its cause before the merge, use --abandon for work that will not be delivered, or --failed-delivery --note when the evaluated commit is already on the integration branch";
const CLOSE_WARNINGS_MESSAGE: &str = "only warn questions failed: --force --note closes the work item, with the owner's authorization";
const QUESTION_FAILED_MESSAGE: &str = "the gate question did not pass";

pub(super) const INVALID_WORKFLOW_NEXT_STEP: &str =
    "fix the workflow file named in the message; `omhl load --dry-run` details the problem";

/// O texto de erro nunca repete valor de entrada: só códigos, ponteiros e regras.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    code: String,
    message: String,
    details: Vec<Detail>,
    /// Fatos que valem para o erro todo e que repetir em cada detalhe só duplicaria.
    context: Option<Value>,
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

        let mut error = json!({ "code": self.code, "message": self.message, "details": details });
        if let Some(context) = &self.context {
            error["context"] = context.clone();
        }

        json!({ "error": error })
    }

    /// Todo código estável tem construtor nomeado; só o código dinâmico de `ProjectError` passa por aqui direto.
    fn plain(code: &str, message: &str) -> AppError {
        AppError {
            code: code.to_owned(),
            context: None,
            message: message.to_owned(),
            details: Vec::new(),
        }
    }

    pub(crate) fn at_pointer(code: &str, pointer: &str, message: &str) -> AppError {
        AppError {
            code: code.to_owned(),
            context: None,
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

    pub(crate) fn freeze_mark_unwritable(work_item: &str) -> AppError {
        AppError::plain(
            "freeze-mark-unwritable",
            &format!(
                "the freeze mark of work item {work_item} could not be written; the lock decided without it"
            ),
        )
    }

    pub(crate) fn worktree_exclude_unreadable() -> AppError {
        AppError::plain(
            "worktree-exclude-unreadable",
            "git check-ignore could not tell whether the worktree folder is ignored; nothing was written to info/exclude",
        )
    }

    pub(crate) fn notes_folder_unwritable() -> AppError {
        AppError::plain(
            "notes-folder-unwritable",
            "the .omhl/notes folder could not be created; the lock allowed the write anyway",
        )
    }

    pub(crate) fn path_unresolvable() -> AppError {
        AppError::plain(
            "path-unresolvable",
            "the physical path of the edited file cannot be resolved; the lock decided by the lexical path only",
        )
    }

    pub(crate) fn invalid_project_dir() -> AppError {
        AppError::plain(
            "invalid-project-dir",
            "the project directory cannot be resolved",
        )
    }

    pub(crate) fn project_dir_missing() -> AppError {
        AppError::plain(
            "invalid-project-dir",
            "the project directory does not exist",
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

    pub(crate) fn runner_unavailable() -> AppError {
        AppError::plain("runner-unavailable", "no command runner is configured")
    }

    pub(crate) fn byproducts_unavailable() -> AppError {
        AppError::plain("byproducts-unavailable", "no byproduct store is configured")
    }

    pub(crate) fn retention_unavailable() -> AppError {
        AppError::plain(
            "retention-unavailable",
            "no byproduct retention is configured",
        )
    }

    pub(crate) fn commit_mark_unavailable() -> AppError {
        AppError::plain(
            "commit-mark-unavailable",
            "the commit mark is not configured or cannot be read",
        )
    }

    pub(crate) fn binary_path_unreadable() -> AppError {
        AppError::plain(
            "unsupported-platform",
            "the path of the omhl binary cannot be read",
        )
    }

    pub(crate) fn action_outside_engine() -> AppError {
        AppError::plain(
            "action-outside-engine",
            "this action runs in the runtime, not in the engine",
        )
    }

    pub(crate) fn process_table_unreadable() -> AppError {
        AppError::plain(
            "unsupported-platform",
            "the process table cannot be read on this platform",
        )
    }

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
            context: None,
            message,
            details,
        }
    }

    pub(crate) fn projection_failed(error: &ProjectionError) -> AppError {
        AppError {
            code: "projection-failed".to_owned(),
            context: None,
            message: "the projection could not be written".to_owned(),
            details: vec![Detail::new(&error.path, error.code, &error.message)],
        }
    }

    pub(crate) fn guide_relation_refusal(mut self, items: &[AppendItem]) -> AppError {
        let Some(detail) = self.details.first_mut() else {
            return self;
        };
        let guidance = match detail.code.as_str() {
            "cycle" => CYCLE_GUIDANCE,
            "endpoint-not-found" => ENDPOINT_GUIDANCE,
            _ => return self,
        };
        let guided = item_index(&detail.path)
            .and_then(|index| items.get(index))
            .is_some_and(|item| match &item.body {
                ItemBody::Relation { relation_type, .. } => {
                    GUIDED_RELATION_TYPES.contains(&relation_type.as_str())
                }
                _ => false,
            });
        if !guided {
            return self;
        }
        detail.message = guidance.cause.to_owned();
        detail.next_step = Some(guidance.next_step.to_owned());
        self.message = guidance.cause.to_owned();

        self
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

    pub(crate) fn probe_failed(reason: &str) -> AppError {
        let code = RuleCode::ProbeFailed.as_str();

        AppError {
            code: code.to_owned(),
            context: None,
            message: format!("{code} at /snapshot: {reason}"),
            details: vec![Detail::new("/snapshot", code, code)],
        }
    }

    pub(crate) fn checks_failed(details: Vec<CheckDetail>, context: Value) -> AppError {
        AppError {
            context: Some(context),
            ..AppError::per_check("checks-failed", "a blocking check failed", details)
        }
    }

    pub(crate) fn check_not_run(details: Vec<CheckDetail>) -> AppError {
        AppError::per_check(
            "check-not-run",
            "a check could not run to a verdict",
            details,
        )
    }

    pub(crate) fn attestation_refused(details: Vec<CheckDetail>) -> AppError {
        AppError::per_check("attestation-refused", "an attestation was refused", details)
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
            context: None,
            message: message.to_owned(),
            details,
        }
    }

    pub(crate) fn check_not_mechanical(path: &str) -> AppError {
        AppError::at_pointer("check-not-mechanical", path, "the check has no command")
    }

    pub(crate) fn native_check_disabled(path: &str) -> AppError {
        let mut error = AppError::at_pointer("check-not-found", path, "check-not-found");
        error.details[0].next_step =
            Some("enable it in [native] checks of .omhl/workflow.toml".to_owned());
        error
    }

    pub(crate) fn check_not_orphaned(path: &str) -> AppError {
        AppError::at_pointer(
            "check-not-orphaned",
            path,
            "the check file still exists in the working tree",
        )
    }

    pub(crate) fn check_retire_refused(pointer: &str) -> AppError {
        AppError::refusal(
            "check-loosening-refused",
            pointer,
            Guidance {
                cause: "retiring a check is the owner's task, not the agent's",
                next_step: "ask the owner to retire it with --owner-override, run outside Claude Code",
            },
        )
    }

    pub(crate) fn guideline_not_found(pointer: &str) -> AppError {
        AppError::refusal(
            "guideline-not-found",
            pointer,
            Guidance {
                cause: "the id is not a current guideline stored in the repository",
                next_step: "pass the id of a current guideline that is not native",
            },
        )
    }

    pub(crate) fn guideline_not_orphaned(pointer: &str) -> AppError {
        AppError::refusal(
            "guideline-not-orphaned",
            pointer,
            Guidance {
                cause: "the guideline file still exists in the working tree",
                next_step: "delete the guideline file, then run omhl guideline retire again",
            },
        )
    }

    pub(crate) fn successor_not_current(pointer: &str) -> AppError {
        AppError::refusal(
            "successor-not-current",
            pointer,
            Guidance {
                cause: "the successor is not a current guideline",
                next_step: "pass the id of a current guideline in --by, or retire without --by",
            },
        )
    }

    pub(crate) fn successor_is_self(pointer: &str) -> AppError {
        AppError::refusal(
            "successor-is-self",
            pointer,
            Guidance {
                cause: "a guideline cannot be its own successor",
                next_step: "pass a different guideline in --by, or retire without --by",
            },
        )
    }

    pub(crate) fn into_supersedes_cycle(self, pointer: &str) -> AppError {
        match self.details.first() {
            Some(detail) if detail.code == "cycle" => AppError::refusal(
                "supersedes-cycle",
                pointer,
                Guidance {
                    cause: "the successor is already superseded by the retired guideline, directly or through others",
                    next_step: "pass a different guideline in --by, or retire without --by",
                },
            ),
            _ => self,
        }
    }

    pub(crate) fn refusal(code: &str, pointer: &str, guidance: Guidance) -> AppError {
        AppError {
            code: code.to_owned(),
            context: None,
            message: guidance.cause.to_owned(),
            details: vec![Detail {
                next_step: Some(guidance.next_step.to_owned()),
                ..Detail::new(pointer, code, guidance.cause)
            }],
        }
    }

    pub(crate) fn failed_delivery_unneeded(only_warn: bool) -> AppError {
        let next_step = match only_warn {
            true => {
                "only warn questions failed: close with --force --note, with the owner's authorization"
            }
            false => "every question passed: close without --failed-delivery",
        };

        AppError::refusal(
            "failed-delivery-unneeded",
            "/failed_delivery",
            Guidance {
                cause: "no refuse question failed, so the failed delivery has nothing to record",
                next_step,
            },
        )
    }

    pub(crate) fn delivery_not_on_work_item_branch() -> AppError {
        AppError::refusal(
            "delivery-not-on-work-item-branch",
            "/work_item",
            Guidance {
                cause: "the current branch is not the branch of the work item",
                next_step: "check out the branch of the work item and run the close again",
            },
        )
    }

    pub(crate) fn delivery_not_integrated() -> AppError {
        AppError::refusal(
            "delivery-not-integrated",
            "/work_item",
            Guidance {
                cause: "the evaluated commit is not in the local integration branch",
                next_step: "update the local integration branch from the remote without a new commit, or ask the owner for --abandon",
            },
        )
    }

    pub(crate) fn delivery_integration_undeclared() -> AppError {
        AppError::refusal(
            "delivery-not-integrated",
            "/work_item",
            Guidance {
                cause: "the workflow declares no integration branch, so the delivery cannot be checked",
                next_step: "declare the integration branch in branch.integration at the owner's request; without it, the only close left is --abandon",
            },
        )
    }

    pub(crate) fn review_integration_missing() -> AppError {
        AppError::refusal(
            "review-base-invalid",
            "/base",
            Guidance {
                cause: "the integration branch declared in branch.integration has no local ref",
                next_step: "create or fetch the local branch named in branch.integration, or fix branch.integration",
            },
        )
    }

    pub(crate) fn review_base_unresolved() -> AppError {
        AppError::refusal(
            "review-base-invalid",
            "/base",
            Guidance {
                cause: "--base does not name a commit, or it is the same commit as HEAD, so there is no range to review",
                next_step: "pass --base with the merge-base of the pull request with its integration branch",
            },
        )
    }

    pub(crate) fn review_base_other_merge_base() -> AppError {
        AppError::refusal(
            "review-base-invalid",
            "/base",
            Guidance {
                cause: "--base gives another merge-base with HEAD than the declared integration branch",
                next_step: "drop --base, or pass a --base whose merge-base with HEAD is the one of the integration branch",
            },
        )
    }

    pub(crate) fn review_base_not_in_integration() -> AppError {
        AppError::refusal(
            "review-base-invalid",
            "/base",
            Guidance {
                cause: "--base is not an ancestor of the declared integration branch",
                next_step: "pass --base with the merge-base that gate-delivery printed before the merge",
            },
        )
    }

    pub(crate) fn close_refused(refusal: CloseRefusal) -> AppError {
        let (code, message, questions) = match refusal {
            CloseRefusal::Refused(questions) => ("close-refused", CLOSE_REFUSED_MESSAGE, questions),
            CloseRefusal::Warnings(questions) => {
                ("close-warnings", CLOSE_WARNINGS_MESSAGE, questions)
            }
            CloseRefusal::DeliveryUnneeded { only_warn } => {
                return AppError::failed_delivery_unneeded(only_warn);
            }
        };
        let details = questions
            .into_iter()
            .filter(|question| !question.passed)
            .enumerate()
            .map(|(index, question)| Detail {
                context: Some(json!({ "label": question.label })),
                next_step: question.next_step,
                ..Detail::new(
                    &format!("/questions/{index}"),
                    question.class.unwrap_or(QuestionClass::Refuse).as_str(),
                    QUESTION_FAILED_MESSAGE,
                )
            })
            .collect();

        AppError {
            code: code.to_owned(),
            context: None,
            message: message.to_owned(),
            details,
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
            context: None,
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

    pub(crate) fn commit_mark_missing(path: &str) -> AppError {
        AppError::at_pointer(
            "commit-mark-missing",
            path,
            "a commit on this exception branch needs the commit mark",
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

    pub(crate) fn byproduct_prune_failed() -> AppError {
        AppError::plain(
            "byproduct-prune-failed",
            "the stored outputs could not be pruned",
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

    pub(crate) fn prefix_not_allowed(path: &str, format: &BranchFormat) -> AppError {
        let message = if format.has_type() {
            format!(
                "the branch name must be {} with a type listed in the workflow prefixes",
                format.template()
            )
        } else {
            format!(
                "the branch format {} has no <type>, so the type is not in the branch name",
                format.template()
            )
        };
        AppError::at_pointer("prefix-not-allowed", path, &message)
    }

    pub(crate) fn main_dir_unknown() -> AppError {
        AppError::plain(
            "main-dir-unknown",
            "the main directory of the repository cannot be resolved",
        )
    }

    pub(crate) fn invalid_workflow(error: ProjectError) -> AppError {
        let mut detail = Detail::new("/workflow", error.code, &error.message);
        detail.next_step = Some(INVALID_WORKFLOW_NEXT_STEP.to_owned());

        AppError {
            code: error.code.to_owned(),
            context: None,
            message: error.message,
            details: vec![detail],
        }
    }

    pub(crate) fn worktree_gate_refused(path: &str, context: Value) -> AppError {
        AppError::at_pointer(
            "worktree-gate-refused",
            path,
            "the premises the branch prefix requires are missing",
        )
        .with_context(context)
    }

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
            context: None,
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
            context: None,
            message: message.to_owned(),
            details: vec![Detail::new(path, "amendment-not-found", message)],
        }
    }

    pub(crate) fn amendment_invalid(violations: &[Violation]) -> AppError {
        AppError::violations_of("amendment-invalid", "amendment", violations)
    }

    /// A base conferida segue na recusa do fechamento, que não devolve o relatório.
    pub(crate) fn with_review_base(mut self, review_base: &Value) -> AppError {
        if self.context.is_none() && !review_base.is_null() {
            self.context = Some(json!({ "review_base": review_base }));
        }

        self
    }

    /// Único caminho, além de `CheckDetail.context`, por onde `context` entra num detalhe; nunca é gravado.
    pub(crate) fn with_context(mut self, context: Value) -> AppError {
        if let Some(detail) = self.details.last_mut() {
            detail.context = Some(context);
        }

        self
    }
}

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
            context: None,
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

impl From<CloseError> for AppError {
    fn from(error: CloseError) -> AppError {
        match error {
            CloseError::Omhl(error) => AppError::from(error),
            CloseError::Refused(refusal) => AppError::close_refused(refusal),
        }
    }
}

impl From<ProjectError> for AppError {
    fn from(error: ProjectError) -> AppError {
        AppError::plain(error.code, &error.message)
    }
}

#[cfg(test)]
mod tests;
