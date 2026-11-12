use crate::project::layout::{CHECKS_DIR, GUIDELINES_DIR};

/// Tipo de extension; a ordem (`Check` antes de `Guideline`) é a ordem de listagem.
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
    /// Exatamente o `data` do node, já com `"vocabulary":"closed"` no check.
    pub data: serde_json::Value,
    /// Só ids de check, na ordem do arquivo.
    pub after: Vec<String>,
}
