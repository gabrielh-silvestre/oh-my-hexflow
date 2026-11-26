//! Varredura dos rótulos do OMC no texto do plugin: só a linha de origem de cada agente pode citá-los.

use std::fs;
use std::path::Path;

use regex::Regex;

const ORIGIN_PREFIX: &str = "Ported from oh-my-claudecode ";
const OMC_LABELS: &str =
    r"(?i)oh-my-claudecode|\bomc\b|ralplan|\b(?:executor|test-engineer|quality-reviewer)\b";

/// Arquivos de `plugin/`, fora de qualquer pasta `.omc/`, relativos a `root`.
pub fn plugin_files(root: &Path) -> Vec<String> {
    // Percorre a pasta em vez de `git ls-files`: teste unitário não abre processo (regra de arquitetura).
    let mut found = Vec::new();
    let mut pending = vec![root.join("plugin")];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("plugin folder is readable") {
            let path = entry.expect("entry is readable").path();
            if path.file_name().is_some_and(|name| name == ".omc") {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(root).expect("path is under root");
                found.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    found.sort();

    found
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
