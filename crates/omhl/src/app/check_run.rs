use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::slice;
use std::time::Duration;

use hexlog::{
    AppendItem, AppendRequest, ErrorCode, ItemBody, SemanticId, Snapshot, SnapshotKind, TypeName,
};
use serde_json::{Value, json};

use super::action::{Caller, CheckTarget};
use super::attest::{attestation_data, derived_id};
use super::engine::{Engine, require_project_root};
use super::error::{AppError, CheckDetail};
use super::retry::with_retry;
use super::work_item::{WorkItemRoute, head_branch, resolve_work_item};
use crate::byproduct::{self, ByproductHash};
use crate::error::{OmhlError, RuleCode};
use crate::extensions::collect::{collect, extension_file_exists};
use crate::extensions::model::ExtensionKind;
use crate::mechanical::branch::{HeadFacts, PushError, PushPlan, current_branch, plan_push};
use crate::mechanical::evaluate::{Observation, evaluate_with_reason};
use crate::mechanical::plan::{CheckResult, PlannedCheck, plan_run, summarize};
use crate::mechanical::spec::{CheckSpec, Outcome, spec_digest};
use crate::probe::{ProbeError, ProbeOutcome, TreeSource};
use crate::runner::{CommandRunner, RunError, RunErrorKind, RunReport, RunSpec};

const ATTESTATION: &str = "attestation";
/// 4 MiB por fluxo.
const MAX_STREAM_BYTES: u64 = 4_194_304;
const CUT_OUTPUT_NOTE: &str = "output was cut at 4 MiB; the rule could not be checked; ";
const TAIL_LINES: usize = 20;
const TAIL_LINE_CHARS: usize = 500;
const NON_BLOCKING_PREFIX: &str = "non-blocking: ";
const BUSY_HINT: &str = "the database stayed locked for about 74 seconds; the verdict above was computed but not recorded; run the same command again when the other process ends";
const OUTCOME_IMMUTABLE_HINT: &str = "this snapshot already has the stored outcome for this check; change the tree to get a new verdict";
const SNAPSHOT_REFUSED_HINT: &str =
    "the index changed while the command ran; do not stage during a hook, then run again";
const PROBE_FAILED_HINT: &str =
    "git could not read the tree; run git status and fix the repository state";
const TREE_CHANGED_HINT: &str = "the command changed the working tree; keep checks read-only, add generated files to .gitignore, or restore tracked files with git checkout -- <path>";
const BLOB_FAILED_HINT: &str =
    "the output store is not writable; fix its permissions or free disk space";
const PARENT_GONE_HINT: &str = "the process that started omhl exited before the checks ran; nothing ran; run the same command again";
const SPEC_STALE_HINT: &str = "the check file on disk is invalid or differs from the stored definition; fix the file and run omhl load, then run the same command again";
const ORPHANED_HINT: &str = "the check file is missing in this working tree; if this branch or worktree predates the check, rebase it onto main (or restore the file); omhl check retire <check-id> is permanent for every clone and branch (the id never comes back), so run it only after the file removal is merged";
const PUSH_TO_MAIN_HINT: &str = "main receives no direct pushes; open a pull request";
const PUSH_INPUT_HINT: &str = "the pre-push input needs four fields per line; run git push again";
/// Variáveis que o git exporta para um hook e que apontariam o comando para o índice do hook.
const HOOK_GIT_ENV: [&str; 5] = [
    "GIT_INDEX_FILE",
    "GIT_DIR",
    "GIT_PREFIX",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
];

/// Variáveis do toolchain Rust que mudam o veredito sem aparecer no comando (`--cap-lints allow` esconde aviso de `clippy -D warnings`); configuração própria vai explícita no `run` do check.
const TOOLCHAIN_ENV: [&str; 5] = [
    "RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
];

/// Construtor do erro sem veredito de um check.
type MissError = fn(Vec<CheckDetail>) -> AppError;

/// O que vale para todos os checks de uma execução; `work_item` é `None` na `develop`.
struct Scope<'a> {
    event: &'a str,
    work_item: Option<&'a SemanticId>,
    branch: &'a str,
    cwd: &'a Path,
    project_root: &'a Path,
    /// Digest da definição de cada check lida do TOML da working tree (id -> digest).
    tree_digests: HashMap<String, String>,
}

/// Check sem veredito gravado; `stops` para a fila (tabela 2.3).
struct Miss {
    error: MissError,
    detail: CheckDetail,
    stops: bool,
}

enum Step {
    Verdict {
        result: CheckResult,
        output: Value,
        failed: Option<CheckDetail>,
    },
    Miss(Miss),
}

enum Recording {
    Recorded,
    Refused {
        code: RuleCode,
        stored: Option<String>,
    },
    Busy,
    Failed(AppError),
}

/// `error` é o do primeiro check sem veredito; `details` seguem a ordem de execução.
#[derive(Default)]
struct Tally {
    results: Vec<CheckResult>,
    outputs: Vec<Value>,
    details: Vec<CheckDetail>,
    error: Option<MissError>,
    snapshot: Option<Snapshot>,
}

impl Engine<'_> {
    /// O autor da attestation é sempre o `executor`; o `check run` cuida das próprias tentativas.
    pub(super) fn check_run(
        &mut self,
        target: &CheckTarget,
        pre_push: Option<&str>,
    ) -> Result<Value, AppError> {
        let root = require_project_root(self.project_root.as_deref())?.to_path_buf();
        let runner = self.runner.ok_or_else(AppError::runner_unavailable)?;
        let (event, planned) = self.plan_target(target)?;
        let checks_planned = planned.len();
        let (planned, orphans): (Vec<_>, Vec<_>) = planned.into_iter().partition(|check| {
            extension_file_exists(ExtensionKind::Check, &root, check.id.as_str())
        });
        let facts = self
            .probe
            .head_facts()
            .map_err(|error| AppError::probe_failed(&error.message))?;
        if let Some(stdin) = pre_push
            && check_push(stdin, &facts, &event)? == PushPlan::NothingToAttest
        {
            return Ok(json!({ "event": event, "push": "nothing-to-attest", "results": [] }));
        }
        let branch = head_branch(&facts, &event)?;
        let route = resolve_work_item(self, &event, &branch)?;
        let scope = Scope {
            event: &event,
            work_item: match &route {
                WorkItemRoute::Recorded(id) => Some(id),
                WorkItemRoute::DryRun => None,
            },
            branch: &branch,
            cwd: &root,
            project_root: &root,
            tree_digests: tree_digests(&root),
        };
        let tally = self.run_planned(runner, &scope, &planned)?;

        finish(&scope, tally, checks_planned, &orphans)
    }

    /// Lê checks e relações `after` em valores próprios: nenhuma transação fica aberta durante a execução.
    fn plan_target(
        &mut self,
        target: &CheckTarget,
    ) -> Result<(String, Vec<PlannedCheck>), AppError> {
        let (nodes, after) = with_retry(&self.retry, || {
            Ok((
                self.omhl.check_nodes(&self.tenant)?,
                self.omhl.check_after_relations(&self.tenant)?,
            ))
        })?;

        match target {
            CheckTarget::Event(event) => Ok((event.clone(), plan_run(event, &nodes, &after))),
            CheckTarget::Check(id) => {
                let node = nodes
                    .iter()
                    .find(|node| node.id.as_str() == id)
                    .ok_or_else(|| OmhlError::rule(RuleCode::CheckNotFound, "/check"))?;
                let event = node.data["when"]["event"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let planned = plan_run(&event, slice::from_ref(node), &[]);
                if planned.is_empty() {
                    return Err(AppError::check_not_mechanical("/check"));
                }
                Ok((event, planned))
            }
        }
    }

    fn run_planned(
        &mut self,
        runner: &dyn CommandRunner,
        scope: &Scope<'_>,
        planned: &[PlannedCheck],
    ) -> Result<Tally, AppError> {
        let mut tally = Tally::default();
        for (index, check) in planned.iter().enumerate() {
            let step = match ready_spec(scope, check) {
                Err(miss) => Step::Miss(miss),
                Ok(_) if scope.stale(check) => Step::Miss(spec_stale_miss(scope, check)),
                Ok(spec) => {
                    match self.snapshot_before(scope, check, spec, tally.snapshot.as_ref())? {
                        Err(miss) => Step::Miss(miss),
                        Ok(snapshot) => {
                            let step = self.execute(runner, scope, check, spec, &snapshot)?;
                            tally.snapshot = Some(snapshot);
                            step
                        }
                    }
                }
            };
            match step {
                Step::Verdict {
                    result,
                    output,
                    failed,
                } => {
                    tally.results.push(result);
                    tally.outputs.push(output);
                    tally.details.extend(failed);
                }
                Step::Miss(miss) => {
                    tally.error.get_or_insert(miss.error);
                    tally.details.push(miss.detail);
                    if miss.stops {
                        let rest = &planned[index + 1..];
                        tally
                            .details
                            .extend(rest.iter().map(|check| not_run(scope, check)));
                        break;
                    }
                }
            }
        }

        Ok(tally)
    }

    /// `GitCommit` é sondado só antes do primeiro check que vai executar; `GitTree`, antes de cada um.
    fn snapshot_before(
        &self,
        scope: &Scope<'_>,
        check: &PlannedCheck,
        spec: &CheckSpec,
        taken: Option<&Snapshot>,
    ) -> Result<Result<Snapshot, Miss>, AppError> {
        match (check.snapshot, taken) {
            (SnapshotKind::GitTree, _) => self.index_tree(scope, check, spec),
            (_, Some(snapshot)) => Ok(Ok(snapshot.clone())),
            (_, None) => self.head_snapshot(scope).map(Ok),
        }
    }

    fn head_snapshot(&self, scope: &Scope<'_>) -> Result<Snapshot, AppError> {
        match self.probe.probe() {
            Ok(ProbeOutcome::Reproducible(snapshot)) => Ok(snapshot),
            Ok(ProbeOutcome::Dirty) => {
                Err(AppError::worktree_dirty("")
                    .with_context(scope.refusal_context("worktree-dirty")))
            }
            Err(error) => Err(AppError::probe_failed(&error.message)),
        }
    }

    /// Árvore do índice; arquivo não rastreado também recusa, e a sonda que falha para a fila.
    fn index_tree(
        &self,
        scope: &Scope<'_>,
        check: &PlannedCheck,
        spec: &CheckSpec,
    ) -> Result<Result<Snapshot, Miss>, AppError> {
        match self.probe.probe_tree(TreeSource::Index) {
            Ok(ProbeOutcome::Reproducible(tree)) => Ok(Ok(tree)),
            Ok(ProbeOutcome::Dirty) => Err(AppError::worktree_differs_from_index("")
                .with_context(scope.refusal_context("worktree-differs-from-index"))),
            Err(error) => {
                let context = scope.context("probe-failed", &check.id, Some(&spec.run));
                Ok(Err(probe_stop(check, context, &error)))
            }
        }
    }

    /// Árvore alterada pelo comando, ou sonda que falha depois dele, param a fila sem gravar nada.
    fn tree_change(
        &self,
        scope: &Scope<'_>,
        check: &PlannedCheck,
        spec: &CheckSpec,
        snapshot: &Snapshot,
        report: &RunReport,
    ) -> Option<Miss> {
        let (changed, code) = match check.snapshot {
            SnapshotKind::GitTree => (self.probe.tree_dirty(), "command-changed-worktree"),
            _ => (
                self.probe
                    .probe()
                    .map(|outcome| outcome != ProbeOutcome::Reproducible(snapshot.clone())),
                "command-changed-tree",
            ),
        };
        let kind = match &changed {
            Ok(false) => return None,
            Ok(true) => "tree-changed",
            Err(_) => "probe-failed",
        };
        let mut context = scope.ran_context(kind, &check.id, &spec.run, snapshot);
        context["output_tail"] = output_tail(report);
        if let Err(error) = changed {
            return Some(probe_stop(check, context, &error));
        }
        context["hint"] = json!(TREE_CHANGED_HINT);

        Some(Miss {
            error: AppError::check_not_run,
            detail: check_detail(check, code, "the command changed the tree", context),
            stops: true,
        })
    }

    fn execute(
        &mut self,
        runner: &dyn CommandRunner,
        scope: &Scope<'_>,
        check: &PlannedCheck,
        spec: &CheckSpec,
        snapshot: &Snapshot,
    ) -> Result<Step, AppError> {
        let request = RunSpec {
            argv: spec.run.clone(),
            cwd: scope.cwd.to_path_buf(),
            project_root: scope.project_root.to_path_buf(),
            timeout: Duration::from_secs(u64::from(spec.timeout_secs)),
            env_set: Vec::new(),
            env_remove: HOOK_GIT_ENV
                .iter()
                .chain(&TOOLCHAIN_ENV)
                .map(|name| (*name).to_owned())
                .collect(),
            max_stream_bytes: MAX_STREAM_BYTES,
        };
        let program = runner.resolve_program(&request);
        let report = match runner.run(&request) {
            Ok(report) => report,
            Err(error) => return Ok(Step::Miss(run_error(scope, check, spec, snapshot, &error))),
        };
        if let Some(miss) = self.tree_change(scope, check, spec, snapshot, &report) {
            return Ok(Step::Miss(miss));
        }
        let stdout = String::from_utf8_lossy(&report.stdout);
        let stderr = String::from_utf8_lossy(&report.stderr);
        let evaluation = evaluate_with_reason(
            spec,
            &Observation {
                exit_code: report.exit_code,
                stdout: &stdout,
                stderr: &stderr,
                timed_out: report.timed_out,
                tree_changed: false,
                truncated: report.truncated,
            },
        );
        let outcome = evaluation.outcome;
        let cut_note = if evaluation.pattern_unchecked {
            CUT_OUTPUT_NOTE
        } else {
            ""
        };
        let failed = |message: String, context: Option<Value>| {
            let prefix = if check.blocking {
                ""
            } else {
                NON_BLOCKING_PREFIX
            };
            (outcome == Outcome::Failed).then(|| CheckDetail {
                check: check.id.as_str().to_owned(),
                code: "failed",
                message: format!("{prefix}{cut_note}{message}"),
                context,
            })
        };
        let verdict = |failed: Option<CheckDetail>| Step::Verdict {
            result: CheckResult {
                id: check.id.clone(),
                outcome,
                blocking: check.blocking,
            },
            output: with_concern(
                with_run_fields(
                    json!({
                        "check": check.id,
                        "outcome": outcome_name(outcome),
                        "ok": outcome != Outcome::Failed,
                        "blocking": check.blocking,
                    }),
                    &report,
                ),
                check,
            ),
            failed,
        };

        let Some(work_item) = scope.work_item else {
            // Na `develop` nada é gravado: a saída só existe no `context`.
            let mut context = scope.ran_context("failed", &check.id, &spec.run, snapshot);
            context["output_tail"] = output_tail(&report);
            if let (Some(context), Value::Object(fields)) = (
                context.as_object_mut(),
                failure_fields(check, spec, &report),
            ) {
                context.extend(fields);
            }
            let message = "output in context.output_tail".to_owned();
            return Ok(verdict(failed(message, Some(context))));
        };
        let store = self
            .byproducts
            .ok_or_else(AppError::byproducts_unavailable)?;
        let mut context = scope.ran_context("guard-refused", &check.id, &spec.run, snapshot);
        context["expected"] = json!(outcome_name(outcome));
        // O blob vai para o store antes do `append`: a attestation nunca aponta para um blob ausente.
        let byproduct = match store.put(&byproduct_blob(&report)) {
            Ok(hash) => hash,
            Err(error) => {
                context["kind"] = json!("blob-failed");
                context["os_error"] = json!(error.message);
                context["output_tail"] = output_tail(&report);
                context["hint"] = json!(BLOB_FAILED_HINT);
                return Ok(Step::Miss(Miss {
                    error: AppError::check_not_run,
                    detail: check_detail(
                        check,
                        "not-run",
                        "the output could not be stored",
                        context,
                    ),
                    stops: true,
                }));
            }
        };
        context["byproduct"] = json!(byproduct.as_str());
        let data = run_attestation_data(
            check, spec, program, work_item, outcome, &report, &byproduct,
        );
        let recording = self.record(check, work_item, snapshot, &data)?;
        let miss = match recording {
            Recording::Recorded => {
                let message = format!(
                    "omhl byproduct {} {} --tail 40",
                    check.id.as_str(),
                    work_item.as_str()
                );
                return Ok(verdict(failed(
                    message,
                    Some(failure_fields(check, spec, &report)),
                )));
            }
            Recording::Busy => {
                context["kind"] = json!("busy-exhausted");
                context["output_tail"] = output_tail(&report);
                context["hint"] = json!(BUSY_HINT);
                Miss {
                    error: AppError::attestation_not_recorded,
                    detail: check_detail(
                        check,
                        "attestation-not-recorded",
                        "the attestation was not recorded",
                        context,
                    ),
                    stops: false,
                }
            }
            Recording::Failed(error) => {
                context["kind"] = json!("attestation-not-recorded");
                context["output_tail"] = output_tail(&report);
                Miss {
                    error: AppError::attestation_not_recorded,
                    detail: check_detail(
                        check,
                        "attestation-not-recorded",
                        &format!("the attestation was not recorded: {}", error.code()),
                        context,
                    ),
                    stops: false,
                }
            }
            Recording::Refused { code, stored } => {
                if let Some(stored) = stored {
                    context["stored"] = json!(stored);
                }
                if let Some(hint) = refusal_hint(code) {
                    context["hint"] = json!(hint);
                }
                Miss {
                    error: AppError::attestation_refused,
                    detail: check_detail(
                        check,
                        code.as_str(),
                        "the guard refused the attestation",
                        context,
                    ),
                    // A árvore mudou durante o comando: os checks seguintes rodariam sobre outra.
                    stops: matches!(
                        code,
                        RuleCode::SnapshotTreeMismatch
                            | RuleCode::SnapshotDirty
                            | RuleCode::SnapshotNotHead
                    ),
                }
            }
        };

        Ok(Step::Miss(miss))
    }

    /// Recusa da guarda, `Busy` esgotado e outro erro do motor viram resultado.
    fn record(
        &mut self,
        check: &PlannedCheck,
        work_item: &SemanticId,
        snapshot: &Snapshot,
        data: &Value,
    ) -> Result<Recording, AppError> {
        let id = match derived_id(snapshot, &check.id, work_item) {
            Ok(id) => id,
            Err(OmhlError::Rule { code, .. }) => {
                return Ok(Recording::Refused { code, stored: None });
            }
            Err(error) => return Err(error.into()),
        };
        let policy = self.retry_after_run.clone();
        // O `get` do `base` fica dentro da tentativa: outro processo pode ter gravado entre elas.
        let attempt = with_retry(&policy, || {
            let item = AppendItem {
                body: ItemBody::Node {
                    id: id.clone(),
                    type_name: TypeName::parse(ATTESTATION)?,
                    data: data.clone(),
                },
                base: self.current_base(&id)?,
                snapshot: Some(snapshot.clone()),
                external_id: None,
            };
            let request = AppendRequest {
                tenant: self.tenant.clone(),
                author: Caller::executor().author(),
                key: None,
                items: vec![item],
            };
            match self.omhl.append(&request) {
                Ok(_) => Ok(None),
                Err(OmhlError::Rule { code, .. }) => Ok(Some(code)),
                Err(error) => Err(AppError::from(error)),
            }
        });

        match attempt {
            Ok(None) => Ok(Recording::Recorded),
            Ok(Some(code)) => Ok(Recording::Refused {
                code,
                stored: self.stored_outcome(code, &id),
            }),
            Err(error) if error.code() == ErrorCode::Busy.as_str() => Ok(Recording::Busy),
            Err(error) => Ok(Recording::Failed(error)),
        }
    }

    /// Só para o `context`: uma leitura que falhe deixa o campo de fora em vez de esconder a recusa.
    fn stored_outcome(&mut self, code: RuleCode, id: &SemanticId) -> Option<String> {
        if code != RuleCode::OutcomeImmutable {
            return None;
        }
        let view = self.omhl.get(&self.tenant, id).ok().flatten()?;

        view.data["outcome"].as_str().map(str::to_owned)
    }
}

impl Scope<'_> {
    /// O TOML da working tree não declara o check (inválido) ou o declara com outra definição que a do banco.
    fn stale(&self, check: &PlannedCheck) -> bool {
        self.tree_digests.get(check.id.as_str()) != Some(&check.spec_digest)
    }

    /// Campos do `context` comuns a todo detalhe de um check.
    fn context(&self, kind: &str, check: &SemanticId, argv: Option<&[String]>) -> Value {
        let mut context = json!({
            "kind": kind,
            "event": self.event,
            "check": check,
            "branch": self.branch,
            "cwd": self.cwd.to_string_lossy(),
        });
        if let Some(work_item) = self.work_item {
            context["work_item"] = json!(work_item);
        }
        if let Some(argv) = argv {
            context["argv"] = json!(argv);
        }

        context
    }

    /// `context` de um check cujo comando rodou sobre `snapshot`.
    fn ran_context(
        &self,
        kind: &str,
        check: &SemanticId,
        argv: &[String],
        snapshot: &Snapshot,
    ) -> Value {
        let mut context = self.context(kind, check, Some(argv));
        context["snapshot"] = json!(snapshot);

        context
    }

    /// `context` de uma recusa da árvore antes de qualquer check.
    fn refusal_context(&self, kind: &str) -> Value {
        let mut context = json!({ "kind": kind, "event": self.event, "branch": self.branch });
        if let Some(work_item) = self.work_item {
            context["work_item"] = json!(work_item);
        }

        context
    }
}

/// `pre-push`: lê a stdin inteira antes de decidir.
fn check_push(stdin: &str, facts: &HeadFacts, event: &str) -> Result<PushPlan, AppError> {
    let checkout = current_branch(facts).ok();

    plan_push(stdin, facts.head.as_deref(), checkout.as_deref()).map_err(|error| match error {
        PushError::ProtectedTarget { remote_ref } => {
            AppError::branch_protected("").with_context(json!({
                "kind": "push-to-main",
                "event": event,
                "rref": remote_ref,
                "hint": PUSH_TO_MAIN_HINT,
            }))
        }
        PushError::NotCheckedOut {
            local_ref,
            remote_ref,
        } => AppError::push_ref_not_checked_out("").with_context(json!({
            "kind": "push-ref-not-checked-out",
            "event": event,
            "lref": local_ref,
            "rref": remote_ref,
            "hint": format!("push {local_ref} from its own worktree; the pre-push hook attests only HEAD"),
        })),
        PushError::Malformed { .. } => AppError::push_input_invalid("").with_context(json!({
            "kind": "push-input-invalid",
            "event": event,
            "hint": PUSH_INPUT_HINT,
        })),
    })
}

/// Digest da definição de cada check que o TOML da working tree declara; arquivo inválido fica de fora, e o banco é a fonte da verdade só enquanto o arquivo o confirma (arquivo ausente é órfão e nem chega aqui).
fn tree_digests(root: &Path) -> HashMap<String, String> {
    collect(root)
        .extensions
        .into_iter()
        .filter(|extension| extension.kind == ExtensionKind::Check)
        .map(|extension| (extension.id, spec_digest(&extension.data)))
        .collect()
}

/// A recusa de um check cujo TOML da working tree é inválido ou diverge do banco: nada roda.
fn spec_stale_miss(scope: &Scope<'_>, check: &PlannedCheck) -> Miss {
    let mut context = scope.context("spec-stale", &check.id, None);
    context["hint"] = json!(SPEC_STALE_HINT);

    Miss {
        error: AppError::check_not_run,
        detail: check_detail(
            check,
            "spec-stale",
            "the check file is invalid or differs from the stored definition",
            context,
        ),
        stops: false,
    }
}

/// Spec inválido não executa e segue para o próximo check.
fn ready_spec<'p>(scope: &Scope<'_>, check: &'p PlannedCheck) -> Result<&'p CheckSpec, Miss> {
    let error = match &check.spec {
        Ok(spec) => return Ok(spec),
        Err(error) => error,
    };

    Err(Miss {
        error: AppError::check_not_run,
        detail: check_detail(
            check,
            "spec-invalid",
            &format!("{} at {}", error.code, error.path),
            scope.context("spec-invalid", &check.id, None),
        ),
        stops: false,
    })
}

/// Erro de spawn segue; plataforma sem suporte e pai perdido param a fila (tabela 2.3).
fn run_error(
    scope: &Scope<'_>,
    check: &PlannedCheck,
    spec: &CheckSpec,
    snapshot: &Snapshot,
    error: &RunError,
) -> Miss {
    let argv0 = spec.run.first().map_or("", String::as_str);
    let (kind, code, hint, failure): (&str, &str, Option<String>, MissError) = match error.kind {
        RunErrorKind::NotFound => (
            "spawn-not-found",
            "spawn-failed",
            Some(format!(
                "`{argv0}` was not found; install it or add it to the PATH that git hooks see, then run the same `omhl check run` again"
            )),
            AppError::check_not_run,
        ),
        RunErrorKind::PermissionDenied => (
            "spawn-permission-denied",
            "spawn-failed",
            Some(format!("`{argv0}` is not executable; fix its permissions")),
            AppError::check_not_run,
        ),
        RunErrorKind::Spawn | RunErrorKind::Io => (
            "spawn-failed",
            "spawn-failed",
            None,
            AppError::check_not_run,
        ),
        RunErrorKind::UnsupportedPlatform => (
            "unsupported-platform",
            "unsupported-platform",
            None,
            AppError::unsupported_platform,
        ),
        RunErrorKind::ParentGone => (
            "parent-gone",
            "not-run",
            Some(PARENT_GONE_HINT.to_owned()),
            AppError::check_not_run,
        ),
    };
    let mut context = scope.ran_context(kind, &check.id, &spec.run, snapshot);
    context["os_error"] = json!(error.message);
    if let Some(hint) = hint {
        context["hint"] = json!(hint);
    }

    Miss {
        error: failure,
        detail: check_detail(check, code, "the command could not run", context),
        stops: matches!(
            error.kind,
            RunErrorKind::UnsupportedPlatform | RunErrorKind::ParentGone
        ),
    }
}

/// Sonda que falha antes ou depois do comando para a fila; só o `context.kind` a distingue.
fn probe_stop(check: &PlannedCheck, mut context: Value, error: &ProbeError) -> Miss {
    context["os_error"] = json!(error.message);
    context["hint"] = json!(PROBE_FAILED_HINT);

    Miss {
        error: AppError::check_not_run,
        detail: check_detail(check, "not-run", "git could not read the tree", context),
        stops: true,
    }
}

/// Check que ficou na fila depois de um erro que para tudo.
fn not_run(scope: &Scope<'_>, check: &PlannedCheck) -> CheckDetail {
    let argv = check.spec.as_ref().ok().map(|spec| spec.run.as_slice());
    let context = scope.context("not-run", &check.id, argv);

    check_detail(check, "not-run", "the check did not run", context)
}

fn check_detail(
    check: &PlannedCheck,
    code: &'static str,
    message: &str,
    context: Value,
) -> CheckDetail {
    CheckDetail {
        check: check.id.as_str().to_owned(),
        code,
        message: message.to_owned(),
        context: Some(context),
    }
}

fn refusal_hint(code: RuleCode) -> Option<&'static str> {
    match code {
        RuleCode::OutcomeImmutable => Some(OUTCOME_IMMUTABLE_HINT),
        RuleCode::ProbeFailed => Some(PROBE_FAILED_HINT),
        _ if code.as_str().starts_with("snapshot-") => Some(SNAPSHOT_REFUSED_HINT),
        _ => None,
    }
}

/// Erro sem veredito vence `checks-failed`; entre eles vale o primeiro na ordem de execução.
fn finish(
    scope: &Scope<'_>,
    tally: Tally,
    checks_planned: usize,
    orphans: &[PlannedCheck],
) -> Result<Value, AppError> {
    if let Some(error) = tally.error {
        return Err(error(tally.details));
    }
    let summary = summarize(&tally.results);
    if !summary.blocked.is_empty() {
        return Err(AppError::checks_failed(
            tally.details,
            failure_context(scope, &tally.results, checks_planned),
        ));
    }

    let mut output = json!({
        "event": scope.event,
        "work_item": scope.work_item,
        "snapshot": tally.snapshot,
        "recorded": scope.work_item.is_some(),
        "results": tally.outputs,
        "blocked": summary.blocked,
    });
    let mut warnings = Vec::new();
    if checks_planned == 0 {
        // Evento sem check no banco sai com 0, mas o aviso evita confundir vazio com aprovado.
        output["checks_planned"] = json!(0);
        warnings.push(json!({ "code": "no-checks-for-event" }));
    }
    warnings.extend(orphans.iter().map(
        |check| json!({ "code": "check-orphaned", "check": check.id, "hint": ORPHANED_HINT }),
    ));
    if !warnings.is_empty() {
        output["warnings"] = json!(warnings);
    }

    Ok(output)
}

/// O que vale para o `checks-failed` inteiro: evento, work item (ausente na `develop`) e o resumo do plano.
fn failure_context(scope: &Scope<'_>, results: &[CheckResult], checks_planned: usize) -> Value {
    let failed = results
        .iter()
        .filter(|result| result.outcome == Outcome::Failed)
        .count();
    let mut context = json!({
        "event": scope.event,
        "summary": {
            "planned": checks_planned,
            "passed": results.len() - failed,
            "failed": failed,
            "not_run": checks_planned - results.len(),
        },
    });
    if let Some(work_item) = scope.work_item {
        context["work_item"] = json!(work_item);
    }

    context
}

/// `concern` e `variant` só entram quando o check os declara.
fn with_concern(mut value: Value, check: &PlannedCheck) -> Value {
    for (key, field) in [("concern", &check.concern), ("variant", &check.variant)] {
        if let Some(field) = field {
            value[key] = json!(field);
        }
    }

    value
}

/// Campos curtos de um check reprovado, sem a saída do comando nem o `how`.
fn failure_fields(check: &PlannedCheck, spec: &CheckSpec, report: &RunReport) -> Value {
    with_run_fields(
        with_concern(
            json!({
                "blocking": check.blocking,
                "run": spec.run,
                "timeout_secs": spec.timeout_secs,
            }),
            check,
        ),
        report,
    )
}

/// Cabeçalho e os dois fluxos completos; nada do ambiente, do `argv`, do `cwd` nem do `context`.
fn byproduct_blob(report: &RunReport) -> Vec<u8> {
    byproduct::encode(&report.stdout, &report.stderr)
}

/// O `command` gravado leva o arquivo que rodou no lugar de `argv[0]`: o mesmo nome em `PATH` diferente muda o veredito.
fn run_attestation_data(
    check: &PlannedCheck,
    spec: &CheckSpec,
    program: Option<PathBuf>,
    work_item: &SemanticId,
    outcome: Outcome,
    report: &RunReport,
    byproduct: &ByproductHash,
) -> Value {
    let mut data = attestation_data(
        &check.id,
        work_item,
        outcome_name(outcome),
        None,
        Some(byproduct.as_str()),
    );
    let mut command = spec.run.clone();
    if let (Some(first), Some(program)) = (command.first_mut(), program) {
        *first = program.to_string_lossy().into_owned();
    }
    data["command"] = json!(command);
    data["spec_digest"] = json!(check.spec_digest);

    with_run_fields(data, report)
}

/// `exit_code` fica de fora, nunca `null`, quando o comando não terminou com código.
fn with_run_fields(mut value: Value, report: &RunReport) -> Value {
    if let Some(code) = report.exit_code {
        value["exit_code"] = json!(code);
    }
    value["duration_ms"] = json!(u64::try_from(report.duration.as_millis()).unwrap_or(u64::MAX));
    value["timed_out"] = json!(report.timed_out);
    value["truncated"] = json!(report.truncated);

    value
}

/// As últimas linhas de cada fluxo, convertidas com perda e cortadas.
fn output_tail(report: &RunReport) -> Value {
    json!({ "stdout": tail(&report.stdout), "stderr": tail(&report.stderr) })
}

fn tail(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().collect();

    lines[lines.len().saturating_sub(TAIL_LINES)..]
        .iter()
        .map(|line| line.chars().take(TAIL_LINE_CHARS).collect())
        .collect()
}

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Passed => "passed",
        Outcome::PassedWithWarning => "passed_with_warning",
        Outcome::Failed => "failed",
    }
}
