//! Política de branch declarada no `[branch]` do `workflow.toml`: formato do nome, exceções e integração.

use hexlog::{Name, SemanticId};

use crate::workflow::names::WORK_ITEM_ID_PREFIX;

pub const DEFAULT_FORMAT: &str = "<type>/wi-<id>__<slug>";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchConfig {
    pub format: BranchFormat,
    pub integration: Option<String>,
    pub exceptions: Vec<ExceptionRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionRule {
    pub pattern: String,
    pub mode: ExceptionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionMode {
    DryRunWithMark,
    DryRun,
    Skip,
}

impl ExceptionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ExceptionMode::DryRunWithMark => "dry-run-with-mark",
            ExceptionMode::DryRun => "dry-run",
            ExceptionMode::Skip => "skip",
        }
    }

    pub fn parse(text: &str) -> Option<ExceptionMode> {
        [
            ExceptionMode::DryRunWithMark,
            ExceptionMode::DryRun,
            ExceptionMode::Skip,
        ]
        .into_iter()
        .find(|mode| mode.as_str() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    Empty,
    UnknownField,
    IdMissing,
    IdRepeated,
    TypeRepeated,
    SlugRepeated,
    InvalidLiteral,
    AmbiguousField,
}

impl FormatError {
    pub fn reason(self) -> &'static str {
        match self {
            FormatError::Empty => "branch.format must not be empty",
            FormatError::UnknownField => {
                "branch.format has a field other than <id>, <type> and <slug>"
            }
            FormatError::IdMissing => "branch.format must have one <id>",
            FormatError::IdRepeated => "branch.format must have only one <id>",
            FormatError::TypeRepeated => "branch.format must have at most one <type>",
            FormatError::SlugRepeated => "branch.format must have at most one <slug>",
            FormatError::InvalidLiteral => {
                "branch.format has a character not allowed in a git branch name"
            }
            FormatError::AmbiguousField => {
                "branch.format has a field whose end cannot be told from the next text"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Id,
    Type,
    Slug,
}

impl Field {
    /// Alfabeto do campo: decide se o literal seguinte marca o fim dele sem ambiguidade.
    fn accepts(self, ch: char) -> bool {
        match self {
            Field::Id => ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-',
            Field::Type => ch != '/',
            Field::Slug => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Literal(String),
    Field(Field),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchFormat {
    template: String,
    parts: Vec<Part>,
}

impl Default for BranchFormat {
    fn default() -> Self {
        BranchFormat::parse(DEFAULT_FORMAT).expect("the default format is valid")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchMatch {
    pub id: SemanticId,
    pub kind: Option<String>,
}

impl BranchFormat {
    pub fn parse(template: &str) -> Result<BranchFormat, FormatError> {
        if template.is_empty() {
            return Err(FormatError::Empty);
        }
        let parts = tokenize(template)?;
        let count = |field| {
            parts
                .iter()
                .filter(|part| **part == Part::Field(field))
                .count()
        };
        let (ids, types, slugs) = (count(Field::Id), count(Field::Type), count(Field::Slug));
        if ids > 1 {
            return Err(FormatError::IdRepeated);
        }
        if types > 1 {
            return Err(FormatError::TypeRepeated);
        }
        if slugs > 1 {
            return Err(FormatError::SlugRepeated);
        }
        if ids == 0 {
            return Err(FormatError::IdMissing);
        }
        let literal_ok = |text: &str| {
            text.chars()
                .all(|ch| is_ref_char(ch) && !matches!(ch, '<' | '>'))
        };
        if parts
            .iter()
            .any(|part| matches!(part, Part::Literal(text) if !literal_ok(text)))
        {
            return Err(FormatError::InvalidLiteral);
        }
        for (index, part) in parts.iter().enumerate() {
            let Part::Field(field) = part else { continue };
            let ambiguous = match parts.get(index + 1) {
                None => false,
                Some(Part::Field(_)) => true,
                Some(Part::Literal(text)) => {
                    text.chars().next().is_some_and(|ch| field.accepts(ch))
                }
            };
            if ambiguous {
                return Err(FormatError::AmbiguousField);
            }
        }
        Ok(BranchFormat {
            template: template.to_owned(),
            parts,
        })
    }

    pub fn template(&self) -> &str {
        &self.template
    }

    pub fn has_type(&self) -> bool {
        self.parts.contains(&Part::Field(Field::Type))
    }

    pub fn match_branch(&self, branch: &str) -> Option<BranchMatch> {
        let mut rest = branch;
        let (mut id, mut kind) = (None, None);
        let mut parts = self.parts.iter().peekable();
        while let Some(part) = parts.next() {
            match part {
                Part::Literal(text) => rest = rest.strip_prefix(text.as_str())?,
                Part::Field(field) => {
                    let end = match parts.peek() {
                        Some(Part::Literal(next)) => rest.find(next.as_str())?,
                        _ => rest.len(),
                    };
                    let value = &rest[..end];
                    rest = &rest[end..];
                    match field {
                        Field::Id => {
                            Name::parse(value).ok()?;
                            id = Some(value);
                        }
                        Field::Type => {
                            if value.is_empty() || value.contains('/') {
                                return None;
                            }
                            kind = Some(value.to_owned());
                        }
                        Field::Slug if value.is_empty() => return None,
                        Field::Slug => {}
                    }
                }
            }
        }
        if !rest.is_empty() {
            return None;
        }
        let id = SemanticId::parse(&format!("{WORK_ITEM_ID_PREFIX}{}", id?)).ok()?;
        Some(BranchMatch { id, kind })
    }
}

fn tokenize(template: &str) -> Result<Vec<Part>, FormatError> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = template;
    while let Some(ch) = rest.chars().next() {
        let closed = (ch == '<')
            .then(|| rest[1..].find(['<', '>']))
            .flatten()
            .filter(|end| rest[1 + end..].starts_with('>'));
        let Some(end) = closed else {
            literal.push(ch);
            rest = &rest[ch.len_utf8()..];
            continue;
        };
        let field = match &rest[1..1 + end] {
            "id" => Field::Id,
            "type" => Field::Type,
            "slug" => Field::Slug,
            _ => return Err(FormatError::UnknownField),
        };
        if !literal.is_empty() {
            parts.push(Part::Literal(std::mem::take(&mut literal)));
        }
        parts.push(Part::Field(field));
        rest = &rest[end + 2..];
    }
    if !literal.is_empty() {
        parts.push(Part::Literal(literal));
    }
    Ok(parts)
}

fn is_ref_char(ch: char) -> bool {
    !(ch.is_ascii_control() || matches!(ch, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
}

impl BranchConfig {
    /// O nome exato vence o prefixo, e entre prefixos vence o mais longo.
    pub fn exception_for(&self, branch: &str) -> Option<ExceptionMode> {
        if let Some(rule) = self.exceptions.iter().find(|rule| rule.pattern == branch) {
            return Some(rule.mode);
        }
        self.exceptions
            .iter()
            .filter_map(|rule| {
                let prefix = rule.pattern.strip_suffix('*')?;
                let tail = branch.strip_prefix(prefix)?;
                (!tail.is_empty()).then_some((prefix.len(), rule.mode))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, mode)| mode)
    }
}

pub fn is_valid_branch_name(name: &str) -> bool {
    if name.is_empty()
        || name == "HEAD"
        || name == "@"
        || name.starts_with('-')
        || name.starts_with('/')
        || name.starts_with("refs/")
        || name.ends_with('/')
        || name.ends_with('.')
        || name.contains("..")
        || name.contains("//")
        || name.contains("@{")
        || !name.chars().all(is_ref_char)
    {
        return false;
    }
    name.split('/')
        .all(|component| !component.starts_with('.') && !component.ends_with(".lock"))
}

#[cfg(test)]
mod tests;
