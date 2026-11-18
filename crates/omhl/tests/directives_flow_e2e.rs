#![cfg(unix)]

mod support;

use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use support::child::{Cmd, Output, omhl_json};
use support::git::git_in;
use support::project::Project;
use tempfile::TempDir;

const W1: &str = "work-items.w1";
const BRANCH: &str = "feat/wi-w1__login";
const SPEC_FOLDER: &str = "wi-w1__login";
const BUSINESS_PREMISE: &str = "premises.login-rule";
const DIVERGENCE: &str = "divergences.login-cmp-001";
const GAP: &str = "gaps.session-storage";
const GAP_ANSWER: &str = "premises.session-in-cookie";
const DECISION: &str = "decisions.cookie-session";
const BUSINESS_GUIDELINE: &str = "guidelines.pay-upfront";
const ARCHITECTURE_GUIDELINE: &str = "guidelines.hexagonal";
const NEW_BUSINESS_GUIDELINE: &str = "guidelines.pay-monthly";

// Repositório git temporário; o `PATH` do filho tem só o `omhl` do teste e o `git`.
// Sem `node`: é a prova do AC 15.

struct Flow {
    project: Project,
    bin: TempDir,
    repo: PathBuf,
}

impl Flow {
    fn new() -> Flow {
        let project = Project::new("acme");
        for args in [
            &["init", "-b", "main"][..],
            &["config", "user.name", "Test Author"],
            &["config", "user.email", "author@example.com"],
            &["add", "-A"],
            &["commit", "-m", "add marker"],
        ] {
            let output = git_in(project.dir(), args);
            assert!(output.status.success(), "git {args:?} failed");
        }
        let repo = fs::canonicalize(project.dir()).expect("the repository exists");
        let bin = TempDir::new().expect("bin directory is created");
        symlink(env!("CARGO_BIN_EXE_omhl"), bin.path().join("omhl")).expect("omhl is linked");
        symlink(find_in_path("git"), bin.path().join("git")).expect("git is linked");

        Flow { project, bin, repo }
    }

    fn command(&self, args: &[&str]) -> Cmd {
        let db = self.project.db_path();
        let db = db.to_str().expect("the path is UTF-8");
        let mut all = vec!["--db", db];
        all.extend_from_slice(args);

        omhl_json(&all).cwd(&self.repo).env("PATH", self.bin.path())
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).run()
    }

    fn run_ok(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

        json_line(&output.stdout)
    }

    fn append(&self, items: &[Value]) {
        let request = json!({ "items": items });
        let appended = self.command(&["append"]).stdin(request.to_string()).run();

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }

    fn gate_worktree(&self) -> Output {
        self.run(&["gate-worktree", "--branch", BRANCH])
    }

    fn spec_dir(&self) -> PathBuf {
        self.repo.join(".omhl/specs").join(SPEC_FOLDER)
    }

    /// Entrega ao hook `spec-lock` o evento `PreToolUse` do Claude Code, pelo stdin.
    fn spec_lock(&self, tool: &str, file: &str) -> Output {
        let event = json!({
            "hook_event_name": "PreToolUse",
            "tool_name": tool,
            "tool_input": { "file_path": self.spec_dir().join(file) },
            "cwd": self.repo,
        });

        self.command(&["hook", "spec-lock"])
            .stdin(event.to_string())
            .run()
    }

    fn open_work_item(&self) {
        self.append(&[node(
            W1,
            "work-item",
            json!({ "title": "login", "status": "open" }),
        )]);
    }

    fn confirm_business_premise(&self) {
        let data = json!({
            "text": "users must log in before using the app",
            "kind": "business",
            "work_item": W1,
            "status": "confirmed",
            "confirmed_by": confirmed_by("Is this premise right?", "Yes, it is."),
        });
        self.append(&[node(BUSINESS_PREMISE, "premise", data)]);
    }

    fn write_valid_spec(&self) {
        let init = self.run_ok(&["spec", "init", W1, "login"]);
        assert_eq!(init["slug"], json!("login"));
        fs::write(self.spec_dir().join("parte-01.md"), VALID_PART).expect("the part is written");
    }

    fn create_branch(&self) {
        let output = git_in(&self.repo, &["branch", BRANCH]);
        assert!(output.status.success(), "git branch failed");
    }

    /// Como `open_work_item`, `confirm_business_premise` e `write_valid_spec`, e a branch criada.
    fn with_frozen_spec() -> Flow {
        let flow = Flow::new();
        flow.open_work_item();
        flow.confirm_business_premise();
        flow.write_valid_spec();
        flow.create_branch();

        flow
    }

    /// Gap técnico respondido por uma premissa do agente, sem diretiva que a ampare.
    fn answer_a_technical_gap(&self) {
        let gap = json!({ "question": "Where is the session kept?", "context": "login", "work_item": W1 });
        let answer = json!({
            "text": "keep the session in a cookie",
            "kind": "technical",
            "work_item": W1,
            "status": "assumed",
        });
        self.append(&[node(GAP, "gap", gap), node(GAP_ANSWER, "premise", answer)]);
        self.append(&[relation("answers", GAP_ANSWER, GAP)]);
    }

    /// Decisão do work item sem `rests-on`.
    fn record_a_decision(&self) {
        let data = json!({
            "text": "keep the session in a cookie",
            "work_item": W1,
            "evidence": [{ "source": "docs", "summary": "the docs recommend cookies" }],
        });
        self.append(&[node(DECISION, "decision", data)]);
    }

    /// Apoia a decisão na premissa de negócio confirmada.
    fn rest_the_decision_on_the_business_premise(&self) {
        self.append(&[relation("rests-on", DECISION, BUSINESS_PREMISE)]);
    }

    fn write_guideline(&self, slug: &str, kind: &str) {
        let dir = self.repo.join(".omhl/guidelines");
        fs::create_dir_all(&dir).expect("the guidelines folder is created");
        let toml = format!("kind = \"{kind}\"\ntext = \"rule {slug}\"\n");
        fs::write(dir.join(format!("{slug}.toml")), toml).expect("the guideline is written");
    }

    /// O gate do PR exige a árvore limpa.
    fn commit_everything(&self) {
        for args in [&["add", "-A"][..], &["commit", "-m", "add guidelines"]] {
            let output = git_in(&self.repo, args);
            assert!(output.status.success(), "git {args:?} failed");
        }
    }

    fn rest_on(&self, from: &str, to: &str) {
        self.append(&[relation("rests-on", from, to)]);
    }

    /// Diretrizes de negócio e de arquitetura carregadas, a segunda apoiada na primeira.
    fn with_guideline_chain() -> Flow {
        let flow = Flow::with_frozen_spec();
        flow.write_guideline("pay-upfront", "business");
        flow.write_guideline("hexagonal", "architecture");
        flow.run_ok(&["load"]);
        flow.commit_everything();
        flow.attest_the_required_checks();
        flow.rest_on(ARCHITECTURE_GUIDELINE, BUSINESS_GUIDELINE);

        flow
    }

    /// Carrega a nova diretriz de negócio e apaga o arquivo da antiga, que fica órfã.
    fn orphan_the_business_guideline(&self) {
        self.write_guideline("pay-monthly", "business");
        self.run_ok(&["load"]);
        fs::remove_file(self.repo.join(".omhl/guidelines/pay-upfront.toml"))
            .expect("the guideline file is removed");
    }

    fn retire_the_business_guideline(&self) -> Value {
        self.run_ok(&[
            "guideline",
            "retire",
            BUSINESS_GUIDELINE,
            "--by",
            NEW_BUSINESS_GUIDELINE,
        ])
    }

    fn open_a_divergence(&self) {
        self.append(&[node(DIVERGENCE, "divergence", divergence_data())]);
    }

    fn write_amendment(&self) {
        fs::write(self.spec_dir().join("emenda-01.md"), amendment_text())
            .expect("the amendment is written");
    }

    /// Revisão `spec-stale` da divergência: append-only, com `base` na revisão corrente.
    fn approve_the_amendment(&self) {
        let current = self.run_ok(&["get", DIVERGENCE]);
        let mut data = divergence_data();
        data["resolution"] = json!("spec-stale");
        data["amendment"] = json!("emenda-01");
        data["confirmed_by"] = confirmed_by("Approve emenda-01?", "Yes, approved.");
        let mut revision = node(DIVERGENCE, "divergence", data);
        revision["base"] = current["node"]["record_id"].clone();

        self.append(&[revision]);
    }

    /// Os dois checks nativos que o gate do PR exige.
    fn attest_the_required_checks(&self) {
        for check in ["checks.run-tests", "checks.ci-green-before-merge"] {
            self.run_ok(&["attest", check, W1, "--outcome", "passed"]);
        }
    }

    /// A árvore precisa estar limpa para o `close`.
    fn close(&self) -> Value {
        self.run_ok(&["close", W1])
    }
}

// O `PATH` do filho é só esta pasta: procura o `git` no `PATH` do teste.
fn find_in_path(program: &str) -> PathBuf {
    let path = env::var_os("PATH").expect("the test has a PATH");

    env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("{program} is not on the PATH"))
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn node(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

fn relation(relation_type: &str, from: &str, to: &str) -> Value {
    json!({ "body": { "kind": "relation", "relation_type": relation_type, "from": from, "to": to } })
}

fn confirmed_by(question: &str, answer: &str) -> Value {
    json!({ "by": "person:ana", "question": question, "answer": answer })
}

fn divergence_data() -> Value {
    json!({
        "source_ref": "CMP-001",
        "kind": "partial",
        "text": "the code records the access and the spec does not say so",
        "evidence": [{ "source": "src/login.rs", "summary": "login records the access" }],
        "resolution": "open",
        "work_item": W1,
    })
}

const VALID_PART: &str = "## Objetivo

Permitir o login.

## Diretivas e premissas aplicáveis

- premises.login-rule

## Comportamentos

- CMP-001: o sistema autentica o usuário.

## Critérios de aceitação

- CA-001: login válido devolve sessão.

## Fora de escopo

Logout.
";

fn amendment_text() -> String {
    format!(
        "## Divergência

{DIVERGENCE}

## Correções

- EMD-001 CMP-001: o sistema autentica o usuário e registra o acesso.

## Motivo

A spec omitia o registro de acesso.
"
    )
}

fn error_code(output: &Output) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
}

fn missing_atoms(output: &Output) -> Value {
    json_line(&output.stderr)["error"]["details"][0]["context"]["missing"].clone()
}

/// A pergunta do relatório do gate com o `label` dado.
fn gate_question(gate: &Value, label: &str) -> Value {
    gate["questions"]
        .as_array()
        .expect("the gate lists its questions")
        .iter()
        .find(|question| question["label"] == label)
        .unwrap_or_else(|| panic!("the gate asks {label}: {gate}"))
        .clone()
}

fn decision(output: &Output) -> Value {
    json_line(&output.stdout)["hookSpecificOutput"]["permissionDecision"].clone()
}

// O ciclo do AC 16, passo a passo, pela CLI

#[test]
fn it_should_refuse_the_worktree_when_there_is_no_confirmed_premise() {
    let flow = Flow::new();
    flow.open_work_item();

    let gate = flow.gate_worktree();

    assert_eq!(
        (gate.code, error_code(&gate), missing_atoms(&gate)),
        (
            Some(1),
            json!("worktree-gate-refused"),
            json!(["business-premise", "spec"])
        )
    );
}

#[test]
fn it_should_refuse_the_worktree_when_the_premise_is_confirmed_and_there_is_no_spec() {
    let flow = Flow::new();
    flow.open_work_item();
    flow.confirm_business_premise();

    let gate = flow.gate_worktree();

    assert_eq!(
        (gate.code, error_code(&gate), missing_atoms(&gate)),
        (Some(1), json!("worktree-gate-refused"), json!(["spec"]))
    );
}

#[test]
fn it_should_validate_the_spec_the_agent_wrote() {
    let flow = Flow::new();
    flow.open_work_item();
    flow.confirm_business_premise();
    flow.write_valid_spec();

    let validated = flow.run_ok(&["spec", "validate", W1]);

    assert_eq!(
        validated,
        json!({ "ok": true, "parts": ["parte-01.md"], "amendments": [] })
    );
}

#[test]
fn it_should_allow_the_worktree_once_the_premise_is_confirmed_and_the_spec_is_valid() {
    let flow = Flow::new();
    flow.open_work_item();
    flow.confirm_business_premise();
    flow.write_valid_spec();

    let allowed = flow.run_ok(&["gate-worktree", "--branch", BRANCH]);

    assert_eq!(
        allowed,
        json!({ "allowed": true, "branch": BRANCH, "reason": "matrix-satisfied" })
    );
}

#[test]
fn it_should_deny_an_edit_of_the_spec_once_the_branch_exists() {
    let flow = Flow::with_frozen_spec();

    let locked = flow.spec_lock("Edit", "parte-01.md");

    assert_eq!((locked.code, decision(&locked)), (Some(0), json!("deny")));
}

#[test]
fn it_should_name_the_next_amendment_when_it_denies_an_edit_of_the_spec() {
    let flow = Flow::with_frozen_spec();

    let locked = flow.spec_lock("Edit", "parte-01.md");

    let reason = json_line(&locked.stdout)["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("the reason is text")
        .to_owned();
    assert!(
        reason.starts_with("spec-frozen") && reason.contains("emenda-01.md"),
        "reason: {reason}"
    );
}

#[test]
fn it_should_let_the_agent_create_the_next_amendment_once_the_spec_is_frozen() {
    let flow = Flow::with_frozen_spec();

    let allowed = flow.spec_lock("Write", "emenda-01.md");

    assert_eq!(
        (allowed.code, allowed.stdout, allowed.stderr),
        (Some(0), String::new(), String::new())
    );
}

#[test]
fn it_should_list_a_technical_gap_and_its_answer_in_the_answers_report() {
    let flow = Flow::with_frozen_spec();
    flow.answer_a_technical_gap();

    let report = flow.run_ok(&["report", "answers", "--work-item", W1]);

    let gap = &report["gaps"][0];
    assert_eq!(
        (
            &gap["id"],
            &gap["state"],
            &gap["answered_by"],
            &gap["answerer"]
        ),
        (
            &json!(GAP),
            &json!("answered"),
            &json!(GAP_ANSWER),
            &json!("autonomous")
        )
    );
}

#[test]
fn it_should_validate_a_pending_amendment_without_approving_it() {
    let flow = Flow::with_frozen_spec();
    flow.open_a_divergence();
    flow.write_amendment();

    let validated = flow.run_ok(&["spec", "validate", W1]);

    assert_eq!(
        validated["amendments"],
        json!([{ "name": "emenda-01.md", "state": "pending", "violations": [] }])
    );
}

#[test]
fn it_should_validate_the_spec_with_the_amendment_approved() {
    let flow = Flow::with_frozen_spec();
    flow.open_a_divergence();
    flow.write_amendment();
    flow.approve_the_amendment();

    let validated = flow.run_ok(&["spec", "validate", W1]);

    assert_eq!(
        validated,
        json!({
            "ok": true,
            "parts": ["parte-01.md"],
            "amendments": [{ "name": "emenda-01.md", "state": "approved", "violations": [] }],
        })
    );
}

#[test]
fn it_should_pass_the_pull_request_gate_after_the_cycle() {
    let flow = Flow::with_frozen_spec();
    flow.answer_a_technical_gap();
    flow.open_a_divergence();
    flow.write_amendment();
    flow.approve_the_amendment();
    flow.attest_the_required_checks();

    let gate = flow.run_ok(&["gate-pr", W1]);

    assert_eq!(gate["passed"], json!(true));
}

#[test]
fn it_should_fail_the_gate_for_an_unanchored_decision_and_pass_once_anchored() {
    let flow = Flow::with_frozen_spec();
    flow.attest_the_required_checks();
    flow.record_a_decision();
    let unanchored = flow.run_ok(&["gate-pr", W1]);

    flow.rest_the_decision_on_the_business_premise();
    let anchored = flow.run_ok(&["gate-pr", W1]);

    assert_eq!(
        (unanchored["passed"].clone(), anchored["passed"].clone()),
        (json!(false), json!(true))
    );
}

#[test]
fn it_should_serialize_the_next_step_only_on_a_failing_question() {
    let flow = Flow::with_frozen_spec();
    flow.attest_the_required_checks();
    flow.record_a_decision();

    let gate = flow.run_ok(&["gate-pr", W1]);

    let failing = gate_question(&gate, "decisions-anchored");
    let passing = gate_question(&gate, "anchors-confirmed");
    assert_eq!(
        (failing["passed"].clone(), failing["next_step"].clone()),
        (
            json!(false),
            json!("record a rests-on from the decision to a confirmed guideline or premise")
        )
    );
    assert_eq!(passing.get("next_step"), None);
}

#[test]
fn it_should_fail_the_unanchored_decision_with_a_next_step_when_guidelines_exist() {
    let flow = Flow::with_guideline_chain();
    flow.record_a_decision();

    let gate = flow.run_ok(&["gate-pr", W1]);

    let question = gate_question(&gate, "decisions-anchored");
    assert_eq!(question["passed"], json!(false));
    assert!(
        question["next_step"]
            .as_str()
            .is_some_and(|step| !step.is_empty()),
        "question: {question}"
    );
}

#[test]
fn it_should_pass_the_anchor_questions_once_the_decision_rests_on_the_architecture_guideline() {
    let flow = Flow::with_guideline_chain();
    flow.record_a_decision();
    flow.rest_on(DECISION, ARCHITECTURE_GUIDELINE);

    let gate = flow.run_ok(&["gate-pr", W1]);

    assert_eq!(
        (
            gate_question(&gate, "decisions-anchored")["passed"].clone(),
            gate_question(&gate, "anchors-confirmed")["passed"].clone()
        ),
        (json!(true), json!(true))
    );
}

#[test]
fn it_should_report_the_retired_guideline_and_its_successor() {
    let flow = Flow::with_guideline_chain();
    flow.orphan_the_business_guideline();

    let report = flow.retire_the_business_guideline();

    assert_eq!(
        report,
        json!({ "retired": BUSINESS_GUIDELINE, "superseded_by": NEW_BUSINESS_GUIDELINE })
    );
}

#[test]
fn it_should_drop_the_retired_guideline_from_the_session_context() {
    let flow = Flow::with_guideline_chain();
    flow.orphan_the_business_guideline();
    flow.retire_the_business_guideline();

    let context = flow.run_ok(&["render"]);

    let text = context["text"].as_str().expect("the render has a text");
    assert!(!text.contains(BUSINESS_GUIDELINE), "context: {text}");
    assert!(text.contains(NEW_BUSINESS_GUIDELINE), "context: {text}");
}

#[test]
fn it_should_keep_the_hash_chain_valid_after_retiring_a_guideline() {
    let flow = Flow::with_guideline_chain();
    flow.orphan_the_business_guideline();
    flow.retire_the_business_guideline();

    let verified = flow.run_ok(&["verify"]);

    assert_eq!(verified["ok"], json!(true));
}

#[test]
fn it_should_close_with_a_retro_that_carries_the_approved_amendment() {
    let flow = Flow::with_frozen_spec();
    flow.answer_a_technical_gap();
    flow.open_a_divergence();
    flow.write_amendment();
    flow.approve_the_amendment();
    flow.attest_the_required_checks();

    let closed = flow.close();

    assert_eq!(
        closed["retro"],
        json!({
            "divergences": [{ "kind": "partial", "resolution": "spec-stale", "ids": [DIVERGENCE] }],
            "amendments": {
                "approved": [{ "divergence": DIVERGENCE, "name": "emenda-01.md" }],
                "pending": [],
            },
            "no_criterion": [{ "gap": GAP, "answered_by": GAP_ANSWER }],
            "open_gaps": [],
        })
    );
}

#[test]
fn it_should_close_with_a_null_retro_when_there_is_no_divergence_and_no_gap() {
    let flow = Flow::with_frozen_spec();
    flow.attest_the_required_checks();

    let closed = flow.close();

    assert_eq!(closed["retro"], Value::Null);
}

// O `case` do hook `pre-switch` do `wt`, sem o `wt`

const WT_CONFIG: &str = "../../.config/wt.toml";

/// O texto do comando `omhl-gate` de `[pre-switch]` em `.config/wt.toml`, na raiz do repositório.
fn pre_switch_template() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(WT_CONFIG);
    let text = fs::read_to_string(&path).expect("the wt project config is readable");
    let config: toml::Value = toml::from_str(&text).expect("the wt project config is valid TOML");

    config["pre-switch"]["omhl-gate"]
        .as_str()
        .expect("the omhl-gate hook is a string")
        .to_owned()
}

/// Executa o template sob `sh`, como o `wt` faz, num `PATH` que é uma pasta vazia: sem o `omhl`.
fn run_pre_switch(branch: &str) -> Option<i32> {
    let script = pre_switch_template().replace("{{ branch }}", branch);
    let empty = TempDir::new().expect("empty PATH directory is created");

    Command::new("/bin/sh")
        .args(["-c", &script])
        .env_clear()
        .env("PATH", empty.path())
        .stdin(Stdio::null())
        .output()
        .expect("sh runs")
        .status
        .code()
}

#[test]
fn it_should_let_develop_through_the_pre_switch_hook_without_omhl_on_the_path() {
    let code = run_pre_switch("develop");

    assert_eq!(code, Some(0));
}

#[test]
fn it_should_let_main_through_the_pre_switch_hook_without_omhl_on_the_path() {
    let code = run_pre_switch("main");

    assert_eq!(code, Some(0));
}

#[test]
fn it_should_fail_the_pre_switch_hook_with_127_for_any_other_branch_when_omhl_is_missing() {
    let code = run_pre_switch(BRANCH);

    assert_eq!(code, Some(127));
}

#[test]
fn it_should_not_wrap_the_branch_placeholder_in_quotes_in_the_pre_switch_template() {
    let template = pre_switch_template();

    let parts: Vec<&str> = template.split("{{ branch }}").collect();

    let quoted = parts
        .windows(2)
        .any(|pair| pair[0].ends_with(['\'', '"']) || pair[1].starts_with(['\'', '"']));
    assert!(parts.len() > 2, "template: {template}");
    assert!(!quoted, "template: {template}");
}
