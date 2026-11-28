use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Component, Path};

use serde::Deserialize;
use toml::Spanned;

use serde_json::{Value, json};

use crate::mechanical::spec::{CheckSpec, spec_digest};
use crate::native::{NativeBase, RUN_TESTS_ID};
use crate::project::ProjectError;
use crate::project::layout::{
    CHECKS_DIR, DOT_GIT_DIR, GUIDELINES_DIR, OMHL_DIR, PROJECTION_DIR, SPECS_DIR, WORKFLOW_FILE,
};
use crate::project::text::{position_at, toml_error_position};
use crate::workflow::branch_policy::{
    BranchConfig, BranchFormat, ExceptionMode, ExceptionRule, is_valid_branch_name,
};

const SUPPORTED_VERSION: u32 = 1;
pub const INVALID_WORKFLOW_CONFIG: &str = "invalid-workflow-config";

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowConfig {
    pub specs: SpecsConfig,
    pub promotion: PromotionConfig,
    pub native: NativeConfig,
    pub interview: InterviewConfig,
    pub prefixes: BTreeMap<String, Vec<RequireAtom>>,
    pub branch: BranchConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterviewConfig {
    pub threshold: f64,
    pub weights: InterviewWeights,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterviewWeights {
    pub objective: f64,
    pub constraints: f64,
    pub success: f64,
    pub context: f64,
}

impl Default for InterviewConfig {
    fn default() -> Self {
        InterviewConfig {
            threshold: 0.20,
            weights: InterviewWeights {
                objective: 0.35,
                constraints: 0.25,
                success: 0.25,
                context: 0.15,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecsConfig {
    pub dir: String,
    pub max_lines: u32,
    pub placeholders: Vec<String>,
    pub cite_business_premise: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionConfig {
    pub min_work_items: u32,
}

/// Resultado da resolução do diretório principal: a ativação dos nativos e a tabela efetiva de tipos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NativeConfig {
    pub checks: BTreeSet<&'static str>,
    pub run: BTreeMap<String, NativeRun>,
    pub invalid_run: BTreeMap<String, String>,
    pub prefixes: BTreeMap<String, Vec<RequireAtom>>,
    /// Só vale na saída do `NativeResolver` e do `repaired_native`; a cópia avaliada fica no padrão.
    pub branch: BranchConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeRun {
    pub run: Vec<String>,
    pub timeout_secs: u32,
    pub expect: Value,
}

fn default_expect() -> Value {
    json!([{ "exit": 0, "outcome": "passed" }])
}

pub fn native_run_digest(entry: &NativeRun) -> String {
    spec_digest(&json!({
        "run": entry.run,
        "timeout_secs": entry.timeout_secs,
        "expect": entry.expect,
    }))
}

/// Digest da tabela como escrita, sem padrão preenchido: é o que o gate pergunta de uma entrada inválida.
pub fn raw_native_run_digest(table: &toml::Table) -> String {
    spec_digest(&serde_json::to_value(table).unwrap_or(Value::Null))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RequireAtom {
    BusinessPremise,
    TechnicalPremiseConfirmed,
    AgentPremise,
    Spec,
}

impl RequireAtom {
    pub fn as_str(self) -> &'static str {
        match self {
            RequireAtom::BusinessPremise => "business-premise",
            RequireAtom::TechnicalPremiseConfirmed => "technical-premise-confirmed",
            RequireAtom::AgentPremise => "agent-premise",
            RequireAtom::Spec => "spec",
        }
    }
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        use RequireAtom::{AgentPremise, BusinessPremise, Spec, TechnicalPremiseConfirmed};

        let prefixes = [
            ("feat", vec![BusinessPremise, Spec]),
            ("fix", vec![BusinessPremise, Spec]),
            ("refactor", vec![TechnicalPremiseConfirmed]),
            ("tests", vec![TechnicalPremiseConfirmed]),
            ("bugfix", vec![AgentPremise]),
            ("chore", vec![]),
            ("docs", vec![]),
        ];

        WorkflowConfig {
            specs: SpecsConfig {
                dir: format!("{OMHL_DIR}/{SPECS_DIR}"),
                max_lines: 300,
                placeholders: ["TODO", "TBD", "FIXME", "???"].map(str::to_owned).to_vec(),
                cite_business_premise: true,
            },
            promotion: PromotionConfig { min_work_items: 3 },
            native: NativeConfig::default(),
            interview: InterviewConfig::default(),
            prefixes: prefixes
                .into_iter()
                .map(|(prefix, atoms)| (prefix.to_owned(), atoms))
                .collect(),
            branch: BranchConfig::default(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkflow {
    version: Spanned<u32>,
    specs: Option<RawSpecs>,
    promotion: Option<RawPromotion>,
    native: Option<RawNative>,
    interview: Option<RawInterview>,
    prefix: Option<BTreeMap<String, RawPrefix>>,
    branch: Option<RawBranch>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBranch {
    format: Option<Spanned<String>>,
    integration: Option<Spanned<String>>,
    exception: Option<Vec<RawException>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawException {
    pattern: Spanned<String>,
    mode: Spanned<String>,
}

/// Um ref maior não seria gravado: a recusa vem no parse, não só no `close --failed-delivery`.
const INTEGRATION_MAX_CHARS: usize = 200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSpecs {
    dir: Option<Spanned<String>>,
    max_lines: Option<Spanned<u32>>,
    placeholders: Option<Vec<String>>,
    cite_business_premise: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPromotion {
    min_work_items: Option<Spanned<u32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNative {
    checks: Option<Vec<Spanned<String>>>,
    run: Option<BTreeMap<String, RawNativeRun>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNativeRun {
    run: Value,
    timeout_secs: Option<Value>,
    expect: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInterview {
    threshold: Option<f64>,
    weights: Option<RawInterviewWeights>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInterviewWeights {
    objective: Option<f64>,
    constraints: Option<f64>,
    success: Option<f64>,
    context: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPrefix {
    require: Vec<RequireAtom>,
}

fn invalid(path: &Path, reason: impl std::fmt::Display) -> ProjectError {
    ProjectError {
        code: INVALID_WORKFLOW_CONFIG,
        message: format!("{}: {reason}", path.display()),
    }
}

/// O texto de erro do TOML repete trechos do arquivo: só a posição sai.
fn invalid_at(path: &Path, text: &str, span: Range<usize>, reason: &str) -> ProjectError {
    let (line, column) = position_at(text, span.start);
    invalid(path, format!("{reason} at line {line}, column {column}"))
}

/// Subárvores de `.omhl/` com dono próprio: `specs.dir` dentro delas misturaria as specs com outro conteúdo.
const RESERVED_OMHL_SUBTREES: [&str; 4] =
    [CHECKS_DIR, GUIDELINES_DIR, PROJECTION_DIR, WORKFLOW_FILE];

/// `specs.dir` é relativo ao diretório principal: absoluto ou com `..` escaparia dele.
fn is_usable_specs_dir(dir: &str) -> bool {
    let dir = Path::new(dir);
    if dir.has_root() || dir.components().any(|c| c == Component::ParentDir) {
        return false;
    }
    let mut names = dir.components().filter_map(|c| match c {
        Component::Normal(name) => name.to_str(),
        _ => None,
    });
    match (names.next(), names.next()) {
        (None, _) | (Some(DOT_GIT_DIR), _) | (Some(OMHL_DIR), None) => false,
        (Some(OMHL_DIR), Some(child)) => !RESERVED_OMHL_SUBTREES.contains(&child),
        _ => true,
    }
}

/// Só o `checks.run-tests` aceita comando; a régua é a do `CheckSpec`.
fn parse_native_run(id: &str, entry: RawNativeRun) -> Result<NativeRun, String> {
    if id != RUN_TESTS_ID {
        return Err(format!(
            "native.run.{id}: only {RUN_TESTS_ID} takes a command"
        ));
    }
    let expect = entry.expect.unwrap_or_else(default_expect);
    let mut data = json!({
        "when": { "event": "before-commit" },
        "run": entry.run,
        "expect": expect,
    });
    if let Some(timeout) = entry.timeout_secs {
        data["timeout_secs"] = timeout;
    }
    let spec = CheckSpec::from_data(&data)
        .map_err(|error| format!("native.run.{id}: {} at {}", error.code, error.path))?
        .ok_or_else(|| format!("native.run.{id}: run is required"))?;

    Ok(NativeRun {
        run: spec.run,
        timeout_secs: spec.timeout_secs,
        expect,
    })
}

fn parse_interview(raw: RawInterview) -> Result<InterviewConfig, String> {
    let mut interview = InterviewConfig::default();
    if let Some(threshold) = raw.threshold {
        if !(threshold > 0.0 && threshold <= 1.0) {
            return Err("interview.threshold must be in (0, 1]".to_owned());
        }
        interview.threshold = threshold;
    }
    if let Some(raw) = raw.weights {
        let w = &mut interview.weights;
        w.objective = raw.objective.unwrap_or(w.objective);
        w.constraints = raw.constraints.unwrap_or(w.constraints);
        w.success = raw.success.unwrap_or(w.success);
        w.context = raw.context.unwrap_or(w.context);
        let all = [w.objective, w.constraints, w.success, w.context];
        if !all.iter().all(|weight| (0.0..=1.0).contains(weight)) {
            return Err("interview.weights must each be in [0, 1]".to_owned());
        }
        if (all.iter().sum::<f64>() - 1.0).abs() > 1e-9 {
            return Err("interview.weights must sum to 1".to_owned());
        }
    }
    Ok(interview)
}

fn parse_branch(path: &Path, text: &str, raw: RawBranch) -> Result<BranchConfig, ProjectError> {
    let mut branch = BranchConfig::default();
    if let Some(format) = raw.format {
        branch.format = BranchFormat::parse(format.as_ref())
            .map_err(|error| invalid_at(path, text, format.span(), error.reason()))?;
    }
    if let Some(integration) = raw.integration {
        let name = integration.as_ref();
        if !is_valid_branch_name(name) || name.chars().count() > INTEGRATION_MAX_CHARS {
            return Err(invalid_at(
                path,
                text,
                integration.span(),
                "branch.integration must be a valid git branch name of at most 200 characters",
            ));
        }
        branch.integration = Some(integration.into_inner());
    }
    for entry in raw.exception.unwrap_or_default() {
        let mode = ExceptionMode::parse(entry.mode.as_ref()).ok_or_else(|| {
            invalid_at(
                path,
                text,
                entry.mode.span(),
                "branch.exception.mode must be dry-run-with-mark, dry-run or skip",
            )
        })?;
        let pattern = entry.pattern.as_ref();
        if !is_valid_pattern(pattern) {
            return Err(invalid_at(
                path,
                text,
                entry.pattern.span(),
                "branch.exception.pattern must be a branch name or prefix/*, other than main",
            ));
        }
        if branch
            .exceptions
            .iter()
            .any(|rule| rule.pattern == *pattern)
        {
            return Err(invalid_at(
                path,
                text,
                entry.pattern.span(),
                "branch.exception.pattern repeats another entry",
            ));
        }
        branch.exceptions.push(ExceptionRule {
            pattern: entry.pattern.into_inner(),
            mode,
        });
    }
    Ok(branch)
}

/// O `*` só vale no fim de `prefixo/*`.
fn is_valid_pattern(pattern: &str) -> bool {
    let name = match pattern.strip_suffix("/*") {
        Some(prefix) if !prefix.is_empty() => format!("{prefix}/x"),
        Some(_) => return false,
        None => pattern.to_owned(),
    };
    pattern != "main" && is_valid_branch_name(&name)
}

pub fn parse_workflow(path: &Path, bytes: &[u8]) -> Result<WorkflowConfig, ProjectError> {
    parse_with(path, bytes, false)
}

pub fn parse_evaluated_workflow(path: &Path, bytes: &[u8]) -> Result<WorkflowConfig, ProjectError> {
    parse_with(path, bytes, true)
}

fn raw_run_table(text: &str, id: &str) -> toml::Table {
    let table = text.parse::<toml::Table>().unwrap_or_default();
    table
        .get("native")
        .and_then(|native| native.get("run"))
        .and_then(|run| run.get(id))
        .and_then(|entry| entry.as_table().cloned())
        .unwrap_or_default()
}

fn parse_with(path: &Path, bytes: &[u8], lenient: bool) -> Result<WorkflowConfig, ProjectError> {
    let text = std::str::from_utf8(bytes).map_err(|_| invalid(path, "not valid UTF-8"))?;
    let raw: RawWorkflow = toml::from_str(text).map_err(|error| {
        let (line, column) = toml_error_position(text, &error);
        invalid(
            path,
            format!("not a valid workflow file at line {line}, column {column}"),
        )
    })?;
    if *raw.version.as_ref() != SUPPORTED_VERSION {
        return Err(invalid_at(
            path,
            text,
            raw.version.span(),
            "unsupported version",
        ));
    }

    let mut config = WorkflowConfig::default();
    if let Some(specs) = raw.specs {
        if let Some(dir) = specs.dir {
            if !is_usable_specs_dir(dir.as_ref()) {
                return Err(invalid_at(
                    path,
                    text,
                    dir.span(),
                    "specs.dir must be a relative folder inside the main directory, outside .git and the reserved .omhl subtrees",
                ));
            }
            config.specs.dir = dir.into_inner();
        }
        if let Some(max_lines) = specs.max_lines {
            if *max_lines.as_ref() == 0 {
                return Err(invalid_at(
                    path,
                    text,
                    max_lines.span(),
                    "specs.max_lines must be at least 1",
                ));
            }
            config.specs.max_lines = max_lines.into_inner();
        }
        if let Some(placeholders) = specs.placeholders {
            config.specs.placeholders = placeholders;
        }
        if let Some(cite) = specs.cite_business_premise {
            config.specs.cite_business_premise = cite;
        }
    }
    if let Some(min_work_items) = raw.promotion.and_then(|promotion| promotion.min_work_items) {
        if *min_work_items.as_ref() == 0 {
            return Err(invalid_at(
                path,
                text,
                min_work_items.span(),
                "promotion.min_work_items must be at least 1",
            ));
        }
        config.promotion.min_work_items = min_work_items.into_inner();
    }
    if let Some(interview) = raw.interview {
        config.interview = parse_interview(interview).map_err(|reason| invalid(path, reason))?;
    }
    let (native_checks, native_run) = raw
        .native
        .map(|native| (native.checks, native.run))
        .unwrap_or_default();
    for (id, entry) in native_run.unwrap_or_default() {
        match parse_native_run(&id, entry) {
            Ok(run) => {
                config.native.run.insert(id, run);
            }
            Err(_) if lenient => {
                let digest = raw_native_run_digest(&raw_run_table(text, &id));
                config.native.invalid_run.insert(id, digest);
            }
            Err(reason) => return Err(invalid(path, reason)),
        }
    }
    for id in native_checks.unwrap_or_default() {
        let Some(check) = NativeBase
            .checks()
            .iter()
            .find(|check| check.id == id.as_ref().as_str())
        else {
            return Err(invalid_at(
                path,
                text,
                id.span(),
                "native.checks must list only native check ids",
            ));
        };
        config.native.checks.insert(check.id);
    }
    if let Some(branch) = raw.branch {
        config.branch = parse_branch(path, text, branch)?;
    }
    for (prefix, table) in raw.prefix.unwrap_or_default() {
        // O projeto soma os átomos aos padrões do tipo: uma lista vazia não os tira.
        let atoms = config.prefixes.entry(prefix).or_default();
        for atom in table.require {
            if !atoms.contains(&atom) {
                atoms.push(atom);
            }
        }
    }

    Ok(config)
}

#[cfg(test)]
mod tests;
