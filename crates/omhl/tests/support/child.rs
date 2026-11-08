use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const DEADLINE: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(5);

pub struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Um comando `omhl` ainda não executado: argumentos, stdin, pasta de trabalho e ambiente extra.
pub struct Cmd {
    args: Vec<OsString>,
    stdin: Vec<u8>,
    cwd: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
}

/// O binário `omhl` de `CARGO_BIN_EXE_omhl` com os argumentos dados.
pub fn omhl(args: &[&str]) -> Cmd {
    Cmd {
        args: args.iter().map(OsString::from).collect(),
        stdin: Vec::new(),
        cwd: None,
        env: Vec::new(),
    }
}

impl Cmd {
    pub fn stdin(mut self, bytes: impl AsRef<[u8]>) -> Cmd {
        self.stdin = bytes.as_ref().to_vec();
        self
    }

    pub fn cwd(mut self, dir: &Path) -> Cmd {
        self.cwd = Some(dir.to_path_buf());
        self
    }

    /// Define `XDG_DATA_HOME` do filho.
    pub fn xdg(self, dir: &Path) -> Cmd {
        self.env("XDG_DATA_HOME", dir)
    }

    pub fn env(mut self, key: &str, value: impl AsRef<OsStr>) -> Cmd {
        self.env
            .push((OsString::from(key), value.as_ref().to_os_string()));
        self
    }

    /// Roda o filho com ambiente limpo (só `PATH`, um `HOME` vazio e o que o `Cmd` pediu).
    /// Passados 30 s, mata o filho e falha o teste.
    pub fn run(self) -> Output {
        let home = TempDir::new().expect("home directory is created");
        let mut command = Command::new(env!("CARGO_BIN_EXE_omhl"));
        command
            .args(&self.args)
            .env_clear()
            .env("HOME", home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        for (key, value) in &self.env {
            command.env(key, value);
        }
        if let Some(dir) = &self.cwd {
            command.current_dir(dir);
        }
        let mut child = command.spawn().expect("the omhl binary starts");

        let mut stdin = child.stdin.take().expect("stdin is piped");
        let bytes = self.stdin;
        let writer = thread::spawn(move || {
            // O filho pode sair sem ler tudo (ex.: entrada grande demais): pipe quebrado é esperado.
            let _ = stdin.write_all(&bytes);
        });
        let stdout = spawn_reader(child.stdout.take().expect("stdout is piped"));
        let stderr = spawn_reader(child.stderr.take().expect("stderr is piped"));

        let deadline = Instant::now() + DEADLINE;
        let status = loop {
            if let Some(status) = child.try_wait().expect("the child is polled") {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().expect("the stuck child is killed");
                child.wait().expect("the killed child is reaped");
                panic!("omhl {:?} did not exit within {DEADLINE:?}", self.args);
            }
            thread::sleep(POLL);
        };

        writer.join().expect("the stdin writer finishes");
        Output {
            code: status.code(),
            stdout: String::from_utf8_lossy(&stdout.join().expect("stdout is read")).into_owned(),
            stderr: String::from_utf8_lossy(&stderr.join().expect("stderr is read")).into_owned(),
        }
    }
}

fn spawn_reader(mut stream: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).expect("the stream is read");
        bytes
    })
}
