//! Instalação dos hooks do git (`fs`, `chmod` e `git config`, nunca em `app/`).

use std::ffi::OsStr;
use std::fs;
use std::io::{self, ErrorKind};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SAMPLE_SUFFIX: &str = ".sample";
const SCRIPT_MODE: u32 = 0o755;

/// O que a instalação pede ao git e ao sistema de arquivos; os erros do git saem com o stderr aparado.
pub trait HookInstaller {
    /// `git rev-parse --path-format=absolute --git-path hooks`.
    fn resolved_hooks_dir(&self) -> Result<PathBuf, String>;
    /// `git config --get core.hooksPath`, em todos os escopos; `None` quando ausente.
    fn configured_hooks_path(&self) -> Result<Option<String>, String>;
    /// Alguma entrada de `dir` que não termina em `.sample`; pasta inexistente conta como vazia.
    fn has_foreign_hooks(&self, dir: &Path) -> bool;
    /// Raiz de cada worktree ligada do clone (`git worktree list --porcelain` sem a primeira entrada, a principal).
    fn linked_worktree_roots(&self) -> Result<Vec<PathBuf>, String>;
    fn binary(&self) -> io::Result<PathBuf>;
    /// Cria `dir` e grava cada `(nome, conteúdo)` com modo `0755`, sobrescrevendo.
    fn write_scripts(&self, dir: &Path, scripts: &[(&str, String)]) -> io::Result<()>;
    /// `git config --local core.hooksPath <dir>`.
    fn set_local_hooks_path(&self, dir: &Path) -> Result<(), String>;
    fn canonical(&self, dir: &Path) -> io::Result<PathBuf>;
}

/// O `git` desta máquina sobre o repositório que envolve `root`, com o ambiente do usuário inteiro
/// (`GIT_CEILING_DIRECTORIES` e `GIT_CONFIG_GLOBAL` valem).
pub struct GitHooks {
    root: PathBuf,
}

impl GitHooks {
    pub fn new(root: &Path) -> GitHooks {
        GitHooks {
            root: root.to_path_buf(),
        }
    }

    fn git(&self, args: &[&OsStr]) -> Result<Output, String> {
        Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| error.to_string())
    }

    fn git_line(&self, args: &[&OsStr]) -> Result<String, String> {
        let output = self.git(args)?;
        if !output.status.success() {
            return Err(stderr_text(&output));
        }

        Ok(stdout_line(&output))
    }
}

fn stdout_line(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

impl HookInstaller for GitHooks {
    fn resolved_hooks_dir(&self) -> Result<PathBuf, String> {
        let args = ["rev-parse", "--path-format=absolute", "--git-path", "hooks"];

        self.git_line(&args.map(OsStr::new)).map(PathBuf::from)
    }

    fn configured_hooks_path(&self) -> Result<Option<String>, String> {
        let output = self.git(&["config", "--get", "core.hooksPath"].map(OsStr::new))?;

        match output.status.code() {
            Some(0) => Ok(Some(stdout_line(&output))),
            Some(1) => Ok(None),
            _ => Err(stderr_text(&output)),
        }
    }

    fn has_foreign_hooks(&self, dir: &Path) -> bool {
        match fs::read_dir(dir) {
            Ok(entries) => entries
                .flatten()
                .any(|entry| !entry.file_name().to_string_lossy().ends_with(SAMPLE_SUFFIX)),
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            // Pasta que não se lê pode guardar hooks: recusar é o lado seguro.
            Err(_) => true,
        }
    }

    fn linked_worktree_roots(&self) -> Result<Vec<PathBuf>, String> {
        let listing = self.git_line(&["worktree", "list", "--porcelain"].map(OsStr::new))?;

        Ok(listing
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .skip(1)
            .map(PathBuf::from)
            .collect())
    }

    fn binary(&self) -> io::Result<PathBuf> {
        std::env::current_exe()
    }

    fn write_scripts(&self, dir: &Path, scripts: &[(&str, String)]) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        for (name, content) in scripts {
            let path = dir.join(name);
            fs::write(&path, content)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(SCRIPT_MODE))?;
        }

        Ok(())
    }

    fn set_local_hooks_path(&self, dir: &Path) -> Result<(), String> {
        let args = ["config", "--local", "core.hooksPath"].map(OsStr::new);

        self.git_line(&[&args[..], &[dir.as_os_str()]].concat())
            .map(drop)
    }

    fn canonical(&self, dir: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(dir)
    }
}
