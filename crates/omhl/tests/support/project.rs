use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::git::{commit_marker, git_ok};

/// A tabela `[branch]` que declara a `develop`, para anexar a um `workflow.toml` que já tem `version`.
pub const DEVELOP_BRANCH_TABLE: &str = "\n[branch]\nintegration = \"develop\"\n\n[[branch.exception]]\npattern = \"develop\"\nmode = \"dry-run-with-mark\"\n";

/// O que o `workflow.toml` deste repositório declara para a `develop`.
pub const DEVELOP_WORKFLOW: &str = "version = 1\n\n[branch]\nintegration = \"develop\"\n\n[[branch.exception]]\npattern = \"develop\"\nmode = \"dry-run-with-mark\"\n";

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

    /// O diretório principal resolve e o `render` não traz o aviso `main-dir-unknown`.
    pub fn with_git(tenant: &str) -> Project {
        let project = Project::new(tenant);
        commit_marker(project.dir());

        project
    }

    pub fn with_workflow(workflow: Option<&str>) -> Project {
        let project = Project::new("acme");
        if let Some(text) = workflow {
            project.write_workflow(text);
        }
        commit_marker(project.dir());

        project
    }

    pub fn write_workflow(&self, text: &str) {
        let dir = self.dir.path().join(".omhl");
        fs::create_dir_all(&dir).expect("the .omhl directory is created");
        fs::write(dir.join("workflow.toml"), text).expect("the workflow file is written");
    }

    /// Declara a `develop` como exceção e a cria a partir do HEAD, com a declaração já commitada.
    pub fn switch_to_declared_develop(&self) {
        self.write_workflow(DEVELOP_WORKFLOW);
        git_ok(self.dir(), &["switch", "-q", "-c", "develop"]);
        git_ok(self.dir(), &["add", "-A"]);
        git_ok(self.dir(), &["commit", "-q", "-m", "declare develop"]);
    }

    /// O `Project::new` não liga nenhum check nativo.
    pub fn enable_natives(&self, ids: &[&str]) {
        let list = ids
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(", ");

        self.write_workflow(&format!("version = 1\n\n[native]\nchecks = [{list}]\n"));
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
