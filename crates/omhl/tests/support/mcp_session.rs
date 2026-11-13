use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use tempfile::TempDir;

const DEADLINE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(5);

/// Um `omhl mcp` vivo, falado linha a linha pelo stdin e lido por uma thread que repassa cada
/// linha do stdout. O filho tem `HOME` e `XDG_DATA_HOME` temporários.
pub struct McpSession {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    _home: TempDir,
}

impl McpSession {
    /// Sobe `omhl mcp --project-dir <cwd>` com `XDG_DATA_HOME = xdg` e ambiente limpo.
    pub fn start(cwd: &Path, xdg: &Path) -> McpSession {
        let home = TempDir::new().expect("home directory is created");
        let mut command = Command::new(env!("CARGO_BIN_EXE_omhl"));
        command
            .args(["mcp", "--project-dir"])
            .arg(cwd)
            .env_clear()
            .env("HOME", home.path())
            .env("XDG_DATA_HOME", xdg)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        let mut child = command.spawn().expect("the omhl binary starts");
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });

        McpSession {
            child,
            stdin: Some(stdin),
            lines,
            _home: home,
        }
    }

    /// Envia a requisição como uma linha terminada em LF.
    pub fn send(&self, request: &Value) {
        let mut bytes = request.to_string().into_bytes();
        bytes.push(b'\n');
        self.send_raw(&bytes);
    }

    pub fn send_raw(&self, bytes: &[u8]) {
        let mut stdin = self.stdin.as_ref().expect("stdin is still open");
        stdin.write_all(bytes).expect("the request is written");
        stdin.flush().expect("the request is flushed");
    }

    /// A próxima linha do stdout como JSON; sem linha em 60 s, falha o teste.
    pub fn recv(&self) -> Value {
        let line = self
            .lines
            .recv_timeout(DEADLINE)
            .expect("the server answers within 60 seconds");

        serde_json::from_str(&line).expect("stdout carries only JSON lines")
    }

    /// Fecha o stdin e espera o filho sair em até 60 s; devolve o código de saída.
    pub fn finish(mut self) -> Option<i32> {
        drop(self.stdin.take());
        let deadline = Instant::now() + DEADLINE;
        loop {
            if let Some(status) = self.child.try_wait().expect("the child is polled") {
                return status.code();
            }
            assert!(
                Instant::now() < deadline,
                "the server did not exit within 60 seconds of stdin EOF"
            );
            thread::sleep(POLL);
        }
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
