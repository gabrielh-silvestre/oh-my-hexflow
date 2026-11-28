//! Um `gh` falso para os testes que rodam o binário: um script posto no começo do `PATH` do `omhl`.
//! Cada chamada grava o argv (um argumento por linha) e o ambiente numa pasta de registro, anota
//! `gh <args>` no registro de ordem, e responde com o arquivo de resposta do subcomando.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::git::write_executable;

/// Ajuda de um `gh` que tem a guarda de ponta do merge.
pub const HELP_WITH_PIN: &str = "Usage: gh pr merge [<number> | <url> | <branch>] [flags]\n      --match-head-commit SHA   Commit SHA that the pull request head must match to allow merge\n";
/// Ajuda de um `gh` antigo, sem `--match-head-commit`.
pub const HELP_WITHOUT_PIN: &str = "Usage: gh pr merge [<number> | <url> | <branch>] [flags]\n  -s, --squash   Squash the commits into one commit and merge it into the base branch\n";
/// O `oid` do commit de merge que a releitura devolve.
pub const MERGE_COMMIT: &str = "9999999999999999999999999999999999999999";

// Cada resposta é um arquivo `<nome>` (stdout), `<nome>.err` (stderr), `<nome>.exit` e `<nome>.sleep`.
// `merged` existe só para virar o `state` quando o `pr merge` roda (o GitHub mergeou).
const SCRIPT: &str = r#"#!/bin/sh
A="@ANSWERS@"
L="@CALLS@"
n=$(ls "$L" | wc -l | tr -d ' ')
mkdir "$L/$n"
for a in "$@"; do printf '%s\n' "$a"; done > "$L/$n/argv"
env > "$L/$n/env"
printf 'gh %s\n' "$*" >> "@ORDER@"
answer() {
  if [ -f "$A/$1.sleep" ]; then sleep "$(cat "$A/$1.sleep")"; fi
  if [ -f "$A/$1.err" ]; then cat "$A/$1.err" >&2; fi
  if [ -f "$A/$1" ]; then cat "$A/$1"; fi
  if [ -f "$A/$1.exit" ]; then exit "$(cat "$A/$1.exit")"; fi
  exit 0
}
case "$*" in
  "auth status"*) answer auth ;;
  "pr merge --help"*) answer help ;;
  "pr merge "*) if [ -f "$A/merged" ]; then cp "$A/merged" "$A/state"; fi; answer merge ;;
  "pr list "*) answer list ;;
  "pr view "*statusCheckRollup*) answer checks ;;
  "pr view "*) answer state ;;
  "repo view "*isFork*) answer fork ;;
  "repo view "*) answer default ;;
  "api graphql"*) answer graphql ;;
esac
exit 0
"#;

/// Uma chamada registrada: o argv sem o `gh` e as linhas `NOME=valor` do ambiente.
#[derive(Debug, Clone)]
pub struct GhCall {
    pub args: Vec<String>,
    pub env: Vec<String>,
}

impl GhCall {
    pub fn line(&self) -> String {
        self.args.join(" ")
    }

    pub fn has(&self, word: &str) -> bool {
        self.args.iter().any(|arg| arg == word)
    }

    pub fn env_value(&self, name: &str) -> Option<String> {
        let prefix = format!("{name}=");
        self.env
            .iter()
            .find_map(|line| line.strip_prefix(&prefix).map(str::to_owned))
    }
}

pub struct FakeGh {
    bin: PathBuf,
    answers: PathBuf,
    calls: PathBuf,
    order: PathBuf,
}

impl FakeGh {
    /// Monta o script em `root/gh-bin/gh`; sem nada respondido, o `gh` está logado, tem a guarda de
    /// ponta, a branch padrão é `main` e o `origin` não é fork.
    pub fn new(root: &Path) -> FakeGh {
        let gh = FakeGh {
            bin: root.join("gh-bin"),
            answers: root.join("gh-answers"),
            calls: root.join("gh-calls"),
            order: root.join("order.log"),
        };
        for dir in [&gh.bin, &gh.answers, &gh.calls] {
            fs::create_dir_all(dir).expect("the fake gh folder is created");
        }
        let script = SCRIPT
            .replace("@ANSWERS@", &gh.answers.display().to_string())
            .replace("@CALLS@", &gh.calls.display().to_string())
            .replace("@ORDER@", &gh.order.display().to_string());
        write_executable(&gh.bin.join("gh"), &script);
        gh.answer("help", HELP_WITH_PIN);
        gh.answer(
            "default",
            &json!({ "defaultBranchRef": { "name": "main" } }).to_string(),
        );
        gh.answer(
            "fork",
            &json!({ "isFork": false, "parent": null }).to_string(),
        );

        gh
    }

    /// A pasta a pôr no começo do `PATH`.
    pub fn bin(&self) -> &Path {
        &self.bin
    }

    /// O registro de ordem, que os checks de teste também podem anotar.
    pub fn order_log(&self) -> &Path {
        &self.order
    }

    pub fn order(&self) -> Vec<String> {
        fs::read_to_string(&self.order)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Grava o stdout de uma resposta (`auth`, `help`, `list`, `checks`, `state`, `merged`, `merge`, `fork`, `default`, `graphql`).
    pub fn answer(&self, name: &str, stdout: &str) {
        fs::write(self.answers.join(name), stdout).expect("the answer is written");
    }

    pub fn answer_json(&self, name: &str, value: &Value) {
        self.answer(name, &value.to_string());
    }

    pub fn exit(&self, name: &str, code: i32) {
        fs::write(self.answers.join(format!("{name}.exit")), code.to_string())
            .expect("the exit code is written");
    }

    pub fn stderr(&self, name: &str, text: &str) {
        fs::write(self.answers.join(format!("{name}.err")), text).expect("the stderr is written");
    }

    pub fn sleep(&self, name: &str, seconds: u32) {
        fs::write(
            self.answers.join(format!("{name}.sleep")),
            seconds.to_string(),
        )
        .expect("the sleep is written");
    }

    /// As chamadas na ordem em que aconteceram.
    pub fn calls(&self) -> Vec<GhCall> {
        let count = fs::read_dir(&self.calls).map(Iterator::count).unwrap_or(0);

        (0..count)
            .map(|n| {
                let dir = self.calls.join(n.to_string());
                let read = |name: &str| {
                    fs::read_to_string(dir.join(name))
                        .unwrap_or_default()
                        .lines()
                        .map(str::to_owned)
                        .collect::<Vec<String>>()
                };
                GhCall {
                    args: read("argv"),
                    env: read("env"),
                }
            })
            .collect()
    }

    /// As linhas `args` de cada chamada, para conferir presença e ausência de subcomandos.
    pub fn lines(&self) -> Vec<String> {
        self.calls().iter().map(GhCall::line).collect()
    }

    pub fn called(&self, prefix: &str) -> bool {
        self.lines().iter().any(|line| line.starts_with(prefix))
    }

    /// Verdadeiro quando o `pr merge` de verdade (não a ajuda) foi chamado.
    pub fn merge_called(&self) -> bool {
        self.lines()
            .iter()
            .any(|line| line.starts_with("pr merge ") && !line.contains("--help"))
    }
}

/// Um PR do `gh pr list`, nos campos que a busca pede.
pub fn pr(
    number: u64,
    state: &str,
    head: &str,
    base: &str,
    cross: bool,
    auto_merge: bool,
) -> Value {
    json!({
        "number": number,
        "state": state,
        "headRefOid": head,
        "isCrossRepository": cross,
        "baseRefName": base,
        "autoMergeRequest": if auto_merge {
            json!({ "enabledAt": "2026-11-28T00:00:00Z", "mergeMethod": "MERGE" })
        } else {
            Value::Null
        },
    })
}

/// Um check run do `statusCheckRollup`.
pub fn check_run(name: &str, status: &str, conclusion: &str) -> Value {
    json!({ "__typename": "CheckRun", "name": name, "status": status, "conclusion": conclusion })
}

/// Um contexto de status do `statusCheckRollup`.
pub fn status_context(context: &str, state: &str) -> Value {
    json!({ "__typename": "StatusContext", "context": context, "state": state })
}

pub fn rollup(head: &str, items: &[Value]) -> Value {
    json!({ "headRefOid": head, "statusCheckRollup": items })
}

/// A releitura do `gh pr view` depois do merge.
pub fn state(
    state: &str,
    head: &str,
    base: &str,
    merge_commit: Option<&str>,
    auto_merge: bool,
) -> Value {
    json!({
        "state": state,
        "autoMergeRequest": if auto_merge { json!({ "mergeMethod": "MERGE" }) } else { Value::Null },
        "baseRefName": base,
        "headRefOid": head,
        "mergeCommit": merge_commit.map(|oid| json!({ "oid": oid })),
    })
}

/// A resposta do `gh api graphql --paginate --slurp`: um array de páginas; `required` é `(nome, isRequired)`.
pub fn required_pages(oid: &str, pages: &[&[(&str, bool)]]) -> Value {
    let last = pages.len().saturating_sub(1);
    let pages: Vec<Value> = pages
        .iter()
        .enumerate()
        .map(|(index, contexts)| {
            let nodes: Vec<Value> = contexts
                .iter()
                .map(|(name, required)| {
                    json!({ "__typename": "CheckRun", "name": name, "isRequired": required })
                })
                .collect();
            json!({ "data": { "repository": { "pullRequest": { "commits": { "nodes": [{ "commit": {
                "oid": oid,
                "statusCheckRollup": { "contexts": {
                    "pageInfo": { "hasNextPage": index < last, "endCursor": format!("c{index}") },
                    "nodes": nodes,
                } },
            } }] } } } } })
        })
        .collect();

    Value::Array(pages)
}
