use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Component, Path};

use serde::Deserialize;
use toml::Spanned;

use crate::project::ProjectError;
use crate::project::layout::{CHECKS_DIR, GUIDELINES_DIR, OMHL_DIR, PROJECTION_DIR, WORKFLOW_FILE};
use crate::project::text::{position_at, toml_error_position};

const SUPPORTED_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowConfig {
    pub specs: SpecsConfig,
    pub promotion: PromotionConfig,
    pub prefixes: BTreeMap<String, Vec<RequireAtom>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecsConfig {
    /// Relativo ao diretório principal.
    pub dir: String,
    pub max_lines: u32,
    pub placeholders: Vec<String>,
    pub cite_business_premise: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionConfig {
    pub min_work_items: u32,
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
                dir: ".omhl/specs".to_owned(),
                max_lines: 300,
                placeholders: ["TODO", "TBD", "FIXME", "???"].map(str::to_owned).to_vec(),
                cite_business_premise: true,
            },
            promotion: PromotionConfig { min_work_items: 3 },
            prefixes: prefixes
                .into_iter()
                .map(|(prefix, atoms)| (prefix.to_owned(), atoms))
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkflow {
    version: Spanned<u32>,
    specs: Option<RawSpecs>,
    promotion: Option<RawPromotion>,
    prefix: Option<BTreeMap<String, RawPrefix>>,
}

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
    min_work_items: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPrefix {
    require: Vec<RequireAtom>,
}

fn invalid(path: &Path, reason: impl std::fmt::Display) -> ProjectError {
    ProjectError {
        code: "invalid-workflow-config",
        message: format!("{}: {reason}", path.display()),
    }
}

/// O texto de erro do TOML repete trechos do arquivo: só a posição sai.
fn invalid_at(path: &Path, text: &str, span: Range<usize>, reason: &str) -> ProjectError {
    let (line, column) = position_at(text, span.start);
    invalid(path, format!("{reason} at line {line}, column {column}"))
}

/// Pasta do git na raiz do projeto: nenhuma spec cabe nela.
const GIT_DIR: &str = ".git";

/// Subárvores de `.omhl/` com dono próprio: `specs.dir` dentro delas misturaria as specs com outro conteúdo.
const RESERVED_OMHL_SUBTREES: [&str; 4] =
    [CHECKS_DIR, GUIDELINES_DIR, PROJECTION_DIR, WORKFLOW_FILE];

/// `specs.dir` é relativo ao diretório principal: absoluto ou com `..` escaparia dele.
/// Precisa ter pasta própria (`""`, `.` e `./` são a raiz) e ficar fora do `.git`, da raiz do `.omhl`
/// e das subárvores reservadas dele.
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
        (None, _) | (Some(GIT_DIR), _) | (Some(OMHL_DIR), None) => false,
        (Some(OMHL_DIR), Some(child)) => !RESERVED_OMHL_SUBTREES.contains(&child),
        _ => true,
    }
}

pub fn parse_workflow(path: &Path, bytes: &[u8]) -> Result<WorkflowConfig, ProjectError> {
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
        config.promotion.min_work_items = min_work_items;
    }
    for (prefix, table) in raw.prefix.unwrap_or_default() {
        config.prefixes.insert(prefix, table.require);
    }

    Ok(config)
}
