//! Varredura dos rótulos do OMC no texto do plugin: só a linha de origem de cada agente pode citá-los.

use std::fs;
use std::path::Path;
use std::process::Command;

use regex::Regex;

const ORIGIN_PREFIX: &str = "Ported from oh-my-claudecode ";
const OMC_LABELS: &str =
    r"(?i)oh-my-claudecode|\bomc\b|ralplan|\b(?:executor|test-engineer|quality-reviewer)\b";

/// Arquivos rastreados de `plugin/`, fora de qualquer pasta `.omc/`.
pub fn tracked_plugin_files(root: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "plugin"])
        .current_dir(root)
        .output()
        .expect("git ls-files runs");
    assert!(output.status.success(), "git ls-files plugin failed");

    String::from_utf8(output.stdout)
        .expect("the listing is UTF-8")
        .lines()
        .filter(|path| !path.split('/').any(|part| part == ".omc"))
        .map(str::to_owned)
        .collect()
}

/// `arquivo:linha` de cada linha com rótulo do OMC que não seja linha de origem.
pub fn scan_files(root: &Path, files: &[String]) -> Vec<String> {
    let labels = Regex::new(OMC_LABELS).expect("regex");
    let mut found = Vec::new();
    for file in files {
        let text = fs::read_to_string(root.join(file))
            .unwrap_or_else(|error| panic!("{file} is unreadable: {error}"));
        for (index, line) in text.lines().enumerate() {
            if !line.starts_with(ORIGIN_PREFIX) && labels.is_match(line) {
                found.push(format!("{file}:{}", index + 1));
            }
        }
    }

    found
}

#[cfg(test)]
mod tests;
