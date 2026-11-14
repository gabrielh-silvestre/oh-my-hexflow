use std::fs;
use std::path::Path;

use super::model::{ExtensionIssue, ExtensionKind, RawExtension};
use crate::ports::files::{list_sorted_capped, read_regular_utf8};
use crate::project::layout::{
    DIR_ENTRIES_MAX, EXT_FILE_MAX_BYTES, OMHL_DIR, PROJECTION_DIR, SPECS_DIR, WORKFLOW_FILE,
};

const KINDS: [ExtensionKind; 2] = [ExtensionKind::Check, ExtensionKind::Guideline];
const SLUG_MAX_LEN: usize = 63;

#[derive(Debug, Default)]
pub struct ScanOutput {
    pub files: Vec<RawExtension>,
    pub errors: Vec<ExtensionIssue>,
    pub warnings: Vec<ExtensionIssue>,
}

pub fn scan(root: &Path) -> ScanOutput {
    scan_with(root, &[])
}

/// `extra_ignored` são nomes diretos de `.omhl/` ignorados além dos fixos, como o `specs.dir` configurado.
pub fn scan_with(root: &Path, extra_ignored: &[&str]) -> ScanOutput {
    let mut output = ScanOutput::default();
    let omhl = root.join(OMHL_DIR);
    match fs::symlink_metadata(&omhl) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(metadata) if metadata.is_symlink() => {
            output.errors.push(issue(OMHL_DIR.to_owned(), "symlink"));
            return output;
        }
        _ => return output,
    }
    match names_in(&omhl, OMHL_DIR) {
        Ok(names) => {
            for name in &names {
                scan_root_entry(root, name, extra_ignored, &mut output);
            }
        }
        Err(error) => output.errors.push(error),
    }
    output
        .files
        .sort_by(|left, right| (left.kind, &left.slug).cmp(&(right.kind, &right.slug)));
    output
        .errors
        .sort_by(|left, right| (&left.path, left.code).cmp(&(&right.path, right.code)));
    output
        .warnings
        .sort_by(|left, right| (&left.path, left.code).cmp(&(&right.path, right.code)));
    output
}

fn issue(path: String, code: &'static str) -> ExtensionIssue {
    let message = match code {
        "symlink" => "entry is a symbolic link",
        "not-regular-file" => "entry is not a regular file or directory as expected",
        "file-too-large" => "file is larger than the allowed size",
        "not-utf8" => "file is not valid UTF-8",
        "bad-file-name" => "file name is not a valid slug with a .toml or .md extension",
        "too-many-entries" => "directory has more entries than allowed",
        "hidden-file-ignored" => "hidden file ignored",
        "orphan-markdown" => "markdown file has no sibling .toml",
        "unknown-entry" => "entry is not part of the .omhl layout",
        _ => "could not read the entry",
    };
    ExtensionIssue {
        path,
        code,
        message: message.to_owned(),
        next_step: None,
    }
}

fn names_in(dir: &Path, rel: &str) -> Result<Vec<String>, ExtensionIssue> {
    let names =
        list_sorted_capped(dir, DIR_ENTRIES_MAX).map_err(|code| issue(rel.to_owned(), code))?;
    Ok(names
        .into_iter()
        .map(|name| {
            name.into_string()
                .unwrap_or_else(|name| name.to_string_lossy().into_owned())
        })
        .collect())
}

fn scan_root_entry(root: &Path, name: &str, extra_ignored: &[&str], output: &mut ScanOutput) {
    if let Some(kind) = KINDS.into_iter().find(|kind| kind.dir() == name) {
        scan_dir(root, kind, output);
    } else if ![PROJECTION_DIR, SPECS_DIR, WORKFLOW_FILE].contains(&name)
        && !extra_ignored.contains(&name)
    {
        output
            .warnings
            .push(issue(format!("{OMHL_DIR}/{name}"), "unknown-entry"));
    }
}

fn scan_dir(root: &Path, kind: ExtensionKind, output: &mut ScanOutput) {
    let dir_rel = format!("{OMHL_DIR}/{}", kind.dir());
    let dir = root.join(&dir_rel);
    match fs::symlink_metadata(&dir) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(metadata) => {
            let code = if metadata.is_symlink() {
                "symlink"
            } else {
                "not-regular-file"
            };
            output.errors.push(issue(dir_rel, code));
            return;
        }
        Err(_) => {
            output.errors.push(issue(dir_rel, "io-error"));
            return;
        }
    }
    match names_in(&dir, &dir_rel) {
        Ok(names) => {
            for name in &names {
                scan_entry(root, kind, &dir_rel, &names, name, output);
            }
        }
        Err(error) => output.errors.push(error),
    }
}

fn scan_entry(
    root: &Path,
    kind: ExtensionKind,
    dir_rel: &str,
    names: &[String],
    name: &str,
    output: &mut ScanOutput,
) {
    let path = || format!("{dir_rel}/{name}");
    if name.starts_with('.') {
        return output.warnings.push(issue(path(), "hidden-file-ignored"));
    }
    let (slug, is_toml) = match (name.strip_suffix(".toml"), name.strip_suffix(".md")) {
        (Some(slug), _) => (slug, true),
        (_, Some(slug)) => (slug, false),
        _ => return output.errors.push(issue(path(), "bad-file-name")),
    };
    if !is_valid_slug(slug) {
        return output.errors.push(issue(path(), "bad-file-name"));
    }
    if is_toml {
        let has_markdown = names.iter().any(|n| n.strip_suffix(".md") == Some(slug));
        match read_extension(root, kind, dir_rel, slug, has_markdown) {
            Ok(file) => output.files.push(file),
            Err(error) => output.errors.push(error),
        }
    } else if !names.iter().any(|n| n.strip_suffix(".toml") == Some(slug)) {
        output.warnings.push(issue(path(), "orphan-markdown"));
    }
}

fn read_extension(
    root: &Path,
    kind: ExtensionKind,
    dir_rel: &str,
    slug: &str,
    has_markdown: bool,
) -> Result<RawExtension, ExtensionIssue> {
    let toml_path = format!("{dir_rel}/{slug}.toml");
    let toml = match read_regular_utf8(&root.join(&toml_path), EXT_FILE_MAX_BYTES) {
        Ok(toml) => toml,
        Err(code) => return Err(issue(toml_path, code)),
    };
    let markdown_path = has_markdown.then(|| format!("{dir_rel}/{slug}.md"));
    let markdown = match &markdown_path {
        Some(path) => Some(
            read_regular_utf8(&root.join(path), EXT_FILE_MAX_BYTES)
                .map_err(|code| issue(path.clone(), code))?,
        ),
        None => None,
    };
    Ok(RawExtension {
        kind,
        slug: slug.to_owned(),
        toml_path,
        toml,
        markdown_path,
        markdown,
    })
}

fn is_valid_slug(slug: &str) -> bool {
    slug.len() <= SLUG_MAX_LEN
        && slug.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}
