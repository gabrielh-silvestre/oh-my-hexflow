//! Árvore `.omhl/` em disco para os testes de extension: uma raiz de projeto temporária (ou uma
//! raiz já existente) e métodos que montam arquivos, pastas, symlinks e FIFOs sob ela.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

const OMHL_DIR: &str = ".omhl";

pub struct ExtTree {
    root: PathBuf,
    _owned: Option<TempDir>,
}

impl ExtTree {
    /// Raiz de projeto em um diretório temporário que some junto com a árvore.
    pub fn new() -> ExtTree {
        let dir = TempDir::new().expect("project directory is created");

        ExtTree {
            root: dir.path().to_path_buf(),
            _owned: Some(dir),
        }
    }

    /// Árvore sobre uma raiz que já existe e pertence a quem a criou.
    pub fn at(root: &Path) -> ExtTree {
        ExtTree {
            root: root.to_path_buf(),
            _owned: None,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Arquivo em `rel` (relativo à raiz do projeto), com as pastas de cima criadas.
    pub fn file(&self, rel: &str, bytes: impl AsRef<[u8]>) -> &ExtTree {
        let target = self.root.join(rel);
        self.make_parent(&target);
        fs::write(&target, bytes).unwrap_or_else(|error| panic!("cannot write {rel}: {error}"));

        self
    }

    pub fn check(&self, slug: &str, toml: &str) -> &ExtTree {
        self.file(&format!("{OMHL_DIR}/checks/{slug}.toml"), toml)
    }

    pub fn guideline(&self, slug: &str, toml: &str) -> &ExtTree {
        self.file(&format!("{OMHL_DIR}/guidelines/{slug}.toml"), toml)
    }

    /// Texto longo `.omhl/<kind_dir>/<slug>.md`.
    pub fn markdown(&self, kind_dir: &str, slug: &str, text: &str) -> &ExtTree {
        self.file(&format!("{OMHL_DIR}/{kind_dir}/{slug}.md"), text)
    }

    pub fn dir(&self, rel: &str) -> &ExtTree {
        let target = self.root.join(rel);
        fs::create_dir_all(&target).unwrap_or_else(|error| panic!("cannot create {rel}: {error}"));

        self
    }

    pub fn symlink(&self, rel: &str, target: &str) -> &ExtTree {
        let link = self.root.join(rel);
        self.make_parent(&link);
        symlink(target, &link).unwrap_or_else(|error| panic!("cannot link {rel}: {error}"));

        self
    }

    pub fn fifo(&self, rel: &str) -> &ExtTree {
        let target = self.root.join(rel);
        self.make_parent(&target);
        let made = Command::new("mkfifo")
            .arg(&target)
            .status()
            .expect("mkfifo runs");
        assert!(made.success(), "mkfifo failed for {rel}");

        self
    }

    pub fn read(&self, rel: &str) -> Vec<u8> {
        fs::read(self.root.join(rel)).unwrap_or_else(|error| panic!("cannot read {rel}: {error}"))
    }

    fn make_parent(&self, target: &Path) {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("cannot create {}: {error}", parent.display()));
        }
    }
}

impl Default for ExtTree {
    fn default() -> ExtTree {
        ExtTree::new()
    }
}

/// Grava `.omhl/checks/<slug>.toml` com o que o `spec_digest` do banco cobre, para o `check run` achar o arquivo que confirma o check semeado direto no banco.
/// Numa raiz que é repositório git a pasta `.omhl/` vai para o `info/exclude`, para o arquivo não sujar a árvore.
pub fn write_check_file(root: &Path, id: &str, data: &Value) {
    let Some(slug) = id.strip_prefix("checks.") else {
        return;
    };
    if data.get("run").is_none() {
        return;
    }
    let mut text: String = ["blocking", "how", "run", "timeout_secs", "expect"]
        .iter()
        .map(|key| format!("{key} = {}\n", toml_value(&data[*key])))
        .collect();
    text.push_str(&format!(
        "when = {{ event = {} }}\n",
        toml_value(&data["when"]["event"])
    ));
    ExtTree::at(root).check(slug, &text);
    let exclude = root.join(".git/info/exclude");
    if exclude.parent().is_some_and(Path::is_dir) {
        fs::write(&exclude, format!("{OMHL_DIR}/\n")).expect("the exclude file is written");
    }
}

/// Texto, número e booleano de JSON já são TOML válido; tabela e lista mudam só a pontuação.
fn toml_value(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(key, item)| format!("{key} = {}", toml_value(item)))
                .collect();
            format!("{{ {} }}", pairs.join(", "))
        }
        Value::Array(items) => {
            let rendered: Vec<String> = items.iter().map(toml_value).collect();
            format!("[{}]", rendered.join(", "))
        }
        other => other.to_string(),
    }
}
