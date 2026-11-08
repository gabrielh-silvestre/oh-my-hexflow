use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// Projeto de teste: uma pasta com `.omhl.toml` e uma pasta de dados (XDG) fora dela.
pub struct Project {
    dir: TempDir,
    xdg: TempDir,
}

impl Project {
    pub fn new(tenant: &str) -> Project {
        let project = Project {
            dir: TempDir::new().expect("project directory is created"),
            xdg: TempDir::new().expect("data directory is created"),
        };
        fs::write(
            project.dir.path().join(".omhl.toml"),
            format!("version = 1\ntenant = \"{tenant}\"\n"),
        )
        .expect("marker is written");

        project
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    pub fn xdg(&self) -> &Path {
        self.xdg.path()
    }

    pub fn db_path(&self) -> PathBuf {
        self.xdg.path().join("omhl").join("omhl.db")
    }
}
