//! Árvore `.omhl/` em disco para os testes de extension: uma raiz de projeto temporária e métodos que montam
//! arquivos, pastas e symlinks sob ela.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

const OMHL_DIR: &str = ".omhl";

pub struct ExtTree {
    root: PathBuf,
    _owned: Option<TempDir>,
}

impl ExtTree {
    pub fn new() -> ExtTree {
        let dir = TempDir::new().expect("project directory is created");

        ExtTree {
            root: dir.path().to_path_buf(),
            _owned: Some(dir),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

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
