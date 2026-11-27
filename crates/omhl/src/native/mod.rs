use hexlog::{NativeNode, NativeNodes, RelationTypeName, SemanticId, TypeName};

pub const NATIVE_CATALOG_VERSION: &str = "1.1.0";

const RUN_TESTS_ID: &str = "checks.run-tests";
pub(crate) const CI_GREEN_ID: &str = "checks.ci-green-before-merge";
pub(crate) const CONFIG_LOOSENING_REVIEW_ID: &str = "checks.config-loosening-review";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCheck {
    pub id: &'static str,
    pub event: &'static str,
    pub how: &'static str,
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeGuideline {
    pub id: &'static str,
    pub text: &'static str,
}

// A ordem é a do contrato: o plugin nunca remove nem reordena um id nativo.
static CHECKS: [NativeCheck; 4] = [
    NativeCheck {
        id: RUN_TESTS_ID,
        event: "before-commit",
        how: "run the project's test suite and confirm it passes before creating a commit",
        blocking: true,
    },
    NativeCheck {
        id: CI_GREEN_ID,
        event: "before-merge",
        how: "wait until every check run on the head commit is green; if one fails, fix it, push, and run CI again",
        blocking: true,
    },
    NativeCheck {
        id: "checks.verify-review-comment-anchoring",
        event: "before-publishing-review-comments",
        how: "compare path, line and side of each inline comment with the diff of the head SHA; post the approved text unchanged",
        blocking: true,
    },
    NativeCheck {
        id: CONFIG_LOOSENING_REVIEW_ID,
        event: "before-pr",
        how: "have oh-my-hexflow:code-reviewer review every loosening of a check or gate configuration in .omhl/workflow.toml or .omhl/checks/ in the diff, fix the findings and let the human attest it before opening the PR",
        blocking: true,
    },
];

static GUIDELINES: [NativeGuideline; 2] = [
    NativeGuideline {
        id: "guidelines.one-decision-per-turn",
        text: "when walking through pending items, ask one question per turn with 2 to 4 options and wait for the answer before the next",
    },
    NativeGuideline {
        id: "guidelines.commit-policy",
        text: "by default, ask before committing and pushing at the end of a task; a project guideline may state a different policy",
    },
];

/// Os caminhos da lista que mudam a configuração de checks ou de workflow.
pub fn review_trigger_paths(changed: &[String]) -> Vec<String> {
    changed
        .iter()
        .filter(|path| {
            path.as_str() == ".omhl/workflow.toml"
                || path
                    .strip_prefix(".omhl/checks/")
                    .is_some_and(|name| !name.contains('/') && is_check_file(name))
        })
        .cloned()
        .collect()
}

fn is_check_file(name: &str) -> bool {
    [".toml", ".md"].iter().any(|extension| {
        name.strip_suffix(extension)
            .is_some_and(|stem| !stem.is_empty())
    })
}

#[derive(Debug, Clone, Copy)]
pub struct NativeBase;

impl NativeBase {
    pub fn checks(&self) -> &'static [NativeCheck] {
        &CHECKS
    }

    pub fn guidelines(&self) -> &'static [NativeGuideline] {
        &GUIDELINES
    }
}

impl NativeNodes for NativeBase {
    fn lookup(&self, id: &SemanticId) -> Option<NativeNode> {
        let type_name = if self.checks().iter().any(|check| check.id == id.as_str()) {
            "check"
        } else if self
            .guidelines()
            .iter()
            .any(|guideline| guideline.id == id.as_str())
        {
            "guideline"
        } else {
            return None;
        };

        Some(NativeNode {
            type_name: TypeName::parse(type_name).ok()?,
        })
    }

    fn native_relations(&self, relation_type: &RelationTypeName) -> Vec<(SemanticId, SemanticId)> {
        if relation_type.as_str() != "after" {
            return Vec::new();
        }

        match (
            SemanticId::parse(CI_GREEN_ID),
            SemanticId::parse(RUN_TESTS_ID),
        ) {
            (Ok(from), Ok(to)) => vec![(from, to)],
            _ => Vec::new(),
        }
    }

    fn catalog_version(&self) -> &str {
        NATIVE_CATALOG_VERSION
    }
}

#[cfg(test)]
mod tests;
