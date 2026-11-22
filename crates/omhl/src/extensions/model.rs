use crate::guidance::Guidance;
use crate::project::layout::{CHECKS_DIR, GUIDELINES_DIR};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtensionKind {
    Check,
    Guideline,
}

impl ExtensionKind {
    pub fn dir(self) -> &'static str {
        match self {
            Self::Check => CHECKS_DIR,
            Self::Guideline => GUIDELINES_DIR,
        }
    }

    pub fn type_name(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Guideline => "guideline",
        }
    }

    pub fn id_prefix(self) -> &'static str {
        match self {
            Self::Check => "checks.",
            Self::Guideline => "guidelines.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionIssue {
    pub path: String,
    pub code: &'static str,
    pub message: String,
    pub next_step: Option<Guidance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawExtension {
    pub kind: ExtensionKind,
    pub slug: String,
    pub toml_path: String,
    pub toml: String,
    pub markdown_path: Option<String>,
    pub markdown: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extension {
    pub kind: ExtensionKind,
    pub id: String,
    pub path: String,
    pub data: serde_json::Value,
    pub after: Vec<String>,
}
