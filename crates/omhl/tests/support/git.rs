use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

/// Roda o `git` real com o ambiente neutralizado: toda variável `GIT_*` herdada sai, o
/// `~/.gitconfig` e a configuração do sistema não entram, autor e committer são fixos.
pub fn git_in(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(&key);
        }
    }
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test Author")
        .env("GIT_AUTHOR_EMAIL", "author@example.com")
        .env("GIT_COMMITTER_NAME", "Test Committer")
        .env("GIT_COMMITTER_EMAIL", "committer@example.com")
        .env("LC_ALL", "C")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("the git executable must be on the PATH to run these tests")
}

/// Transforma a pasta de um projeto de teste num repositório git limpo, com o marcador commitado
/// (o que o `close` exige).
pub fn commit_marker(dir: &Path) {
    let steps: [&[&str]; 5] = [
        &["init", "-b", "main"],
        &["config", "user.name", "Test Author"],
        &["config", "user.email", "author@example.com"],
        &["add", "-A"],
        &["commit", "-m", "add marker"],
    ];
    for args in steps {
        let output = git_in(dir, args);
        assert!(output.status.success(), "git {args:?} failed");
    }
}

/// O `Command` do `git` com o ambiente dos hooks de teste: como `git_in` (toda `GIT_*` herdada sai,
/// config global e de sistema neutras, autor e committer fixos), mais `XDG_DATA_HOME` e `HOME`
/// explícitos, `-C dir`, stdin nulo, stdout e stderr em cano. Não roda: quem chama faz `spawn` ou
/// `output` (e pode pôr `process_group(0)` antes do `spawn`).
pub fn git_command_with_env(
    dir: &Path,
    xdg_data_home: &Path,
    home: &Path,
    args: &[&str],
) -> Command {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(&key);
        }
    }
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test Author")
        .env("GIT_AUTHOR_EMAIL", "author@example.com")
        .env("GIT_COMMITTER_NAME", "Test Committer")
        .env("GIT_COMMITTER_EMAIL", "committer@example.com")
        .env("LC_ALL", "C")
        .env("XDG_DATA_HOME", xdg_data_home)
        .env("HOME", home)
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// `git_command_with_env(..).output()`; não confere o código de saída.
pub fn git_with_env(dir: &Path, xdg_data_home: &Path, home: &Path, args: &[&str]) -> Output {
    git_command_with_env(dir, xdg_data_home, home, args)
        .output()
        .expect("the git executable must be on the PATH to run these tests")
}

pub struct GitRepo {
    dir: TempDir,
}

impl GitRepo {
    pub fn init() -> GitRepo {
        let repo = GitRepo {
            dir: TempDir::new().expect("temp dir"),
        };
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.name", "Test Author"]);
        repo.git(&["config", "user.email", "author@example.com"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write(&self, rel: &str, content: &str) {
        let target = self.dir.path().join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(target, content).expect("write file");
    }

    /// Commita tudo (`git add -A`) e devolve `git rev-parse HEAD`.
    pub fn commit_all(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", message]);
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    /// Roda o `git` no repositório sem checar o código de saída (um merge com conflito sai com 1).
    pub fn git_unchecked(&self, args: &[&str]) -> Output {
        git_in(self.dir.path(), args)
    }

    /// Roda o `git` no repositório; o teste falha se o código de saída não for 0. Devolve o stdout.
    pub fn git(&self, args: &[&str]) -> String {
        let output = self.git_unchecked(args);
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

/// Grava `content` em `path` com modo 0755 sem abrir o arquivo para escrita neste processo: quem grava
/// é um `sh` filho, que já terminou quando a função volta. Gravar com `fs::write` nesta thread deixa
/// o descritor de escrita aberto enquanto outra thread do teste faz `fork`; o filho herda o
/// descritor até o seu `exec`, e executar o script nessa janela dá `ETXTBSY` ("Text file busy").
pub fn write_executable(path: &Path, content: &str) {
    let status = Command::new("sh")
        .args([
            "-c",
            r#"printf '%s' "$1" > "$2" && chmod 755 "$2""#,
            "sh",
            content,
        ])
        .arg(path)
        .status()
        .expect("run sh to write the executable");
    assert!(status.success(), "could not write {}", path.display());
}

/// Grava `#!/bin/sh\n<script_body>` em `dir/fake-git` com modo 0755 e devolve o caminho.
pub fn fake_program(dir: &Path, script_body: &str) -> PathBuf {
    let path = dir.join("fake-git");
    write_executable(&path, &format!("#!/bin/sh\n{script_body}\n"));
    path
}
