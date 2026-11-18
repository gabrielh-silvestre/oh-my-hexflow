mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::child::{Cmd as ChildCmd, Output as Child, omhl};
use support::git::git_in;
use support::legacy::seed_legacy_confirmed_premise;
use support::omhl::work_item_data;
use support::project::Project;
use tempfile::TempDir;

const WORK_ITEM: &str = "work-items.w1";
const SLUG: &str = "login";
const BRANCH: &str = "feat/wi-w1__login";
const DEFAULT_SPECS: &str = ".omhl/specs";
const CUSTOM_SPECS: &str = "docs/team-specs";
const PART_DIR: &str = "wi-w1__login";
const WORKFLOW_PATH: &str = ".omhl/workflow.toml";
const CANARY: &str = "CANARY-7f3a91";
const CITED_PREMISE: &str = "premises.login-rule";
const CITED_DIVERGENCE: &str = "divergences.login-cmp-001";

// Camada pura: `omhl::workflow::spec::validate_spec`

// Uma parte válida de 19 linhas; o índice no vetor é a linha menos um.
const PART_OBJECTIVE_LINE: usize = 3;
const PART_CMP_LINE: usize = 11;
const PART_CA_LINE: usize = 15;
const PART_SECOND_CITATION_LINE: usize = 8;

fn valid_part() -> Vec<&'static str> {
    vec![
        "## Objetivo",
        "",
        "Permitir o login.",
        "",
        "## Diretivas e premissas aplicáveis",
        "",
        "- premises.login-rule",
        "",
        "## Comportamentos",
        "",
        "- CMP-001: o sistema autentica o usuário.",
        "",
        "## Critérios de aceitação",
        "",
        "- CA-001: login válido devolve sessão.",
        "",
        "## Fora de escopo",
        "",
        "Logout.",
    ]
}

// Uma emenda válida de 11 linhas que corrige o `CMP-001` da parte acima.
const AMENDMENT_MOTIVE_BODY_LINE: usize = 11;

fn valid_amendment() -> Vec<&'static str> {
    vec![
        "## Divergência",
        "",
        "divergences.login-cmp-001",
        "",
        "## Correções",
        "",
        "- EMD-001 CMP-001: o sistema autentica o usuário e registra o acesso.",
        "",
        "## Motivo",
        "",
        "A spec omitia o registro de acesso.",
    ]
}

fn text(lines: &[&str]) -> String {
    format!("{}\n", lines.join("\n"))
}

fn with_line<'a>(mut lines: Vec<&'a str>, line: usize, replacement: &'a str) -> Vec<&'a str> {
    lines[line - 1] = replacement;

    lines
}

// Camada de CLI: `omhl spec validate work-items.<id>`

/// Repositório git real com o marcador commitado (e `files` por cima), o work item `w1` aberto no
/// banco e uma worktree ligada na branch `feat/wi-w1__login`.
struct Fixture {
    project: Project,
    _scratch: TempDir,
    linked: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture::with_files(&[])
    }

    fn with_files(files: &[(&str, &str)]) -> Fixture {
        let project = Project::new("acme");
        for (rel, content) in files {
            let target = project.dir().join(rel);
            fs::create_dir_all(target.parent().expect("the file has a parent")).expect("parent");
            fs::write(target, content).expect("file is written");
        }
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
        let scratch = TempDir::new().expect("scratch directory is created");
        let linked = scratch.path().join("wt");
        let added = git_in(
            project.dir(),
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                BRANCH,
                &linked.to_string_lossy(),
            ],
        );
        assert!(added.status.success(), "git worktree add failed");
        let fixture = Fixture {
            project,
            _scratch: scratch,
            linked,
        };
        fixture.append(vec![node(WORK_ITEM, "work-item", work_item_data("t"))]);
        fixture.append(vec![
            node(CITED_PREMISE, "premise", business_premise_data()),
            node(CITED_DIVERGENCE, "divergence", open_divergence_data()),
        ]);

        fixture
    }

    fn append(&self, items: Vec<Value>) {
        self.append_with(items, omhl(&["append"]));
    }

    /// `append` sob o Claude Code com `OMHL_ROLE=omhl:critic`: o autor gravado é o critic.
    fn append_as_critic(&self, items: Vec<Value>) {
        let critic = omhl(&["append"])
            .env("CLAUDECODE", "1")
            .env("OMHL_ROLE", "omhl:critic");

        self.append_with(items, critic);
    }

    fn append_with(&self, items: Vec<Value>, command: ChildCmd) {
        let mut request = json!({});
        request["items"] = Value::Array(items);
        let appended = command
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .stdin(request.to_string())
            .run();

        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
    }

    fn run_in(&self, cwd: &Path, args: &[&str]) -> Child {
        omhl(args).cwd(cwd).xdg(self.project.xdg()).run()
    }

    fn validate_from_linked(&self) -> Child {
        self.run_in(&self.linked, &["spec", "validate", WORK_ITEM])
    }

    fn main_dir(&self) -> PathBuf {
        fs::canonicalize(self.project.dir()).expect("the main directory exists")
    }

    /// Escreve `name` na pasta de spec do work item, sob o diretório principal.
    fn write_in(&self, specs_dir: &str, name: &str, lines: &[&str]) {
        let dir = self.main_dir().join(specs_dir).join(PART_DIR);
        fs::create_dir_all(&dir).expect("spec directory is created");
        fs::write(dir.join(name), text(lines)).expect("spec file is written");
    }

    fn write(&self, name: &str, lines: &[&str]) {
        self.write_in(DEFAULT_SPECS, name, lines);
    }

    /// Sobrescreve o `workflow.toml` da worktree ligada (a versão da branch, não commitada).
    fn write_linked_workflow(&self, toml: &str) {
        let path = self.linked.join(WORKFLOW_PATH);
        fs::create_dir_all(path.parent().expect("the file has a parent")).expect("parent");
        fs::write(path, toml).expect("workflow.toml is written");
    }
}

fn node(id: &str, type_name: &str, data: Value) -> Value {
    let mut item = json!({ "body": { "kind": "node", "id": id, "type": type_name } });
    item["body"]["data"] = data;

    item
}

fn relation(relation_type: &str, from: &str, to: &str) -> Value {
    json!({ "body": {
        "kind": "relation", "relation_type": relation_type, "from": from, "to": to,
    } })
}

/// A premissa que `valid_part` cita, confirmada e do próprio work item.
fn business_premise_data() -> Value {
    json!({
        "text": "t", "kind": "business", "status": "confirmed", "work_item": WORK_ITEM,
        "confirmed_by": { "by": "person:ana", "question": "q", "answer": "a" },
    })
}

/// A divergência que `valid_amendment` cita.
fn open_divergence_data() -> Value {
    json!({
        "source_ref": "CMP-001",
        "kind": "partial",
        "text": "o código registra o acesso e a spec não diz",
        "evidence": [{ "source": "docs", "summary": "o doc diz isso" }],
        "resolution": "open",
        "work_item": WORK_ITEM,
    })
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_code(output: &Child) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
}

/// Cada detalhe do erro vira `"<path> <code>"`.
fn error_details(output: &Child) -> Vec<String> {
    json_line(&output.stderr)["error"]["details"]
        .as_array()
        .expect("details is an array")
        .iter()
        .map(|detail| {
            format!(
                "{} {}",
                detail["path"].as_str().expect("detail path"),
                detail["code"].as_str().expect("detail code")
            )
        })
        .collect()
}

// Spec válida

#[test]
fn it_should_print_ok_with_the_part_names_for_a_valid_spec() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "ok": true, "parts": ["parte-01.md"], "amendments": [] })
    );
}

#[test]
fn it_should_list_the_parts_in_name_order() {
    let fixture = Fixture::new();
    let second = with_line(
        with_line(
            valid_part(),
            PART_CMP_LINE,
            "- CMP-002: o sistema encerra a sessão.",
        ),
        PART_CA_LINE,
        "- CA-002: logout invalida a sessão.",
    );
    fixture.write("parte-02.md", &second);
    fixture.write("parte-01.md", &valid_part());

    let output = fixture.validate_from_linked();

    assert_eq!(
        json_line(&output.stdout)["parts"],
        json!(["parte-01.md", "parte-02.md"])
    );
}

#[test]
fn it_should_validate_from_the_main_directory_too() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());

    let output = fixture.run_in(fixture.project.dir(), &["spec", "validate", WORK_ITEM]);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["ok"], json!(true));
}

#[test]
fn it_should_ignore_files_that_are_neither_parts_nor_amendments() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write("notes.md", &["TODO"]);

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["parts"], json!(["parte-01.md"]));
}

// Spec reprovada

#[test]
fn it_should_refuse_the_skeleton_created_by_spec_init_with_one_detail_per_empty_section() {
    let fixture = Fixture::new();
    let init = fixture.run_in(&fixture.linked, &["spec", "init", WORK_ITEM, SLUG]);

    let output = fixture.validate_from_linked();

    assert_eq!(init.code, Some(0), "stderr: {}", init.stderr);
    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(&output), json!("spec-invalid"));
    assert_eq!(
        error_details(&output),
        vec![
            "parte-01.md:1 section-empty",
            "parte-01.md:3 section-empty",
            "parte-01.md:3 business-premise-missing",
            "parte-01.md:5 section-empty",
            "parte-01.md:7 section-empty",
            "parte-01.md:9 section-empty",
        ]
    );
}

#[test]
fn it_should_refuse_a_spec_that_cites_a_legacy_confirmed_premise() {
    let fixture = Fixture::new();
    seed_legacy_confirmed_premise(
        &fixture.project.db_path(),
        "premises.legacy-rule",
        WORK_ITEM,
        "technical",
    );
    let lines = with_line(
        valid_part(),
        PART_SECOND_CITATION_LINE,
        "- premises.legacy-rule",
    );
    fixture.write("parte-01.md", &lines);

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(1));
    assert_eq!(
        error_details(&output),
        vec!["parte-01.md:8 cited-id-unconfirmed"]
    );
}

#[test]
fn it_should_report_one_detail_with_path_and_code_per_violation() {
    let fixture = Fixture::new();
    let lines = with_line(valid_part(), PART_OBJECTIVE_LINE, "Permitir o login TODO.");
    let lines = with_line(lines, PART_CMP_LINE, "- CMP-1: o sistema autentica.");
    fixture.write("parte-01.md", &lines);

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(&output), json!("spec-invalid"));
    assert_eq!(
        error_details(&output),
        vec![
            "parte-01.md:3 placeholder-found",
            "parte-01.md:11 item-id-invalid"
        ]
    );
}

#[test]
fn it_should_not_echo_the_spec_content_in_the_error() {
    let fixture = Fixture::new();
    let line = format!("Permitir o login TODO {CANARY}.");
    fixture.write(
        "parte-01.md",
        &with_line(valid_part(), PART_OBJECTIVE_LINE, &line),
    );

    let output = fixture.validate_from_linked();

    assert_eq!(error_code(&output), json!("spec-invalid"));
    assert!(!output.stderr.contains(CANARY), "stderr: {}", output.stderr);
    assert!(!output.stdout.contains(CANARY), "stdout: {}", output.stdout);
}

#[test]
fn it_should_apply_the_max_lines_of_the_workflow_file_without_recompiling() {
    let fixture =
        Fixture::with_files(&[(WORKFLOW_PATH, "version = 1\n\n[specs]\nmax_lines = 10\n")]);
    fixture.write("parte-01.md", &valid_part());

    let output = fixture.validate_from_linked();

    assert_eq!(error_code(&output), json!("spec-invalid"));
    assert_eq!(error_details(&output), vec!["parte-01.md:11 file-too-long"]);
}

#[test]
fn it_should_read_the_parts_from_the_configured_specs_dir() {
    let fixture = Fixture::with_files(&[(
        WORKFLOW_PATH,
        "version = 1\n\n[specs]\ndir = \"docs/team-specs\"\n",
    )]);
    fixture.write_in(CUSTOM_SPECS, "parte-01.md", &valid_part());

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["parts"], json!(["parte-01.md"]));
}

#[test]
fn it_should_apply_the_max_lines_of_the_main_directory_workflow_file_from_a_linked_worktree() {
    let fixture =
        Fixture::with_files(&[(WORKFLOW_PATH, "version = 1\n\n[specs]\nmax_lines = 10\n")]);
    fixture.write_linked_workflow("version = 1\n");
    fixture.write("parte-01.md", &valid_part());

    let output = fixture.validate_from_linked();

    assert_eq!(error_code(&output), json!("spec-invalid"));
    assert_eq!(error_details(&output), vec!["parte-01.md:11 file-too-long"]);
}

#[test]
fn it_should_read_the_specs_dir_of_the_main_directory_workflow_file_from_a_linked_worktree() {
    let fixture = Fixture::with_files(&[(
        WORKFLOW_PATH,
        "version = 1\n\n[specs]\ndir = \"docs/team-specs\"\n",
    )]);
    fixture.write_linked_workflow("version = 1\n");
    fixture.write_in(CUSTOM_SPECS, "parte-01.md", &valid_part());

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["parts"], json!(["parte-01.md"]));
}

#[test]
fn it_should_refuse_a_work_item_without_a_spec_folder() {
    let fixture = Fixture::new();

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(&output), json!("spec-not-found"));
}

// Pasta de spec inutilizável

/// Tira a leitura da pasta até o fim do escopo; `None` quando o processo ignora permissões (root).
struct Unreadable(PathBuf);

impl Unreadable {
    fn new(dir: &Path) -> Option<Unreadable> {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o000)).expect("permissions change");
        let guard = Unreadable(dir.to_owned());

        fs::read_dir(dir).is_err().then_some(guard)
    }
}

impl Drop for Unreadable {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

fn assert_spec_store_unusable(output: &Child) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(output), json!("spec-store-unusable"));
    assert_eq!(
        error_details(output),
        vec!["/work_item spec-store-unusable"]
    );
}

#[test]
fn it_should_report_file_too_large_for_a_part_over_the_byte_limit_with_few_lines() {
    let fixture = Fixture::new();
    let long_line = "a".repeat(70 * 1024);
    fixture.write(
        "parte-01.md",
        &with_line(valid_part(), PART_OBJECTIVE_LINE, &long_line),
    );

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    assert_eq!(error_code(&output), json!("spec-invalid"));
    let details = error_details(&output);
    assert!(
        details
            .iter()
            .any(|detail| detail.starts_with("parte-01.md:") && detail.ends_with(" file-too-large")),
        "details: {details:?}"
    );
}

#[test]
fn it_should_not_echo_the_content_of_a_part_over_the_byte_limit() {
    let fixture = Fixture::new();
    let long_line = format!("{CANARY}{}", "a".repeat(70 * 1024));
    fixture.write(
        "parte-01.md",
        &with_line(valid_part(), PART_OBJECTIVE_LINE, &long_line),
    );

    let output = fixture.validate_from_linked();

    assert!(!output.stderr.contains(CANARY), "stderr: {}", output.stderr);
}

#[test]
fn it_should_refuse_with_spec_store_unusable_when_a_part_is_a_directory() {
    let fixture = Fixture::new();
    let part = fixture
        .main_dir()
        .join(DEFAULT_SPECS)
        .join(PART_DIR)
        .join("parte-01.md");
    fs::create_dir_all(&part).expect("directory takes the part name");

    let output = fixture.validate_from_linked();

    assert_spec_store_unusable(&output);
}

#[test]
fn it_should_refuse_with_spec_store_unusable_when_the_spec_folder_cannot_be_read() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    let folder = fixture.main_dir().join(DEFAULT_SPECS).join(PART_DIR);
    let Some(_unreadable) = Unreadable::new(&folder) else {
        return;
    };

    let output = fixture.validate_from_linked();

    assert_spec_store_unusable(&output);
}

#[test]
fn it_should_refuse_with_spec_store_unusable_when_the_work_item_has_two_spec_folders() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    let other = fixture.main_dir().join(DEFAULT_SPECS).join("wi-w1__other");
    fs::create_dir_all(&other).expect("second spec directory is created");
    fs::write(other.join("parte-01.md"), text(&valid_part())).expect("second part is written");

    let output = fixture.validate_from_linked();

    assert_spec_store_unusable(&output);
}

// Ids de vários segmentos

#[test]
fn it_should_refuse_a_citation_of_an_unconfirmed_premise_whose_id_extends_a_confirmed_one() {
    let fixture = Fixture::new();
    let assumed = json!({
        "text": "t", "kind": "business", "status": "assumed", "work_item": WORK_ITEM,
    });
    fixture.append(vec![
        node("premises.row-limit", "premise", business_premise_data()),
        node("premises.row-limit.fallback", "premise", assumed),
    ]);
    let lines = with_line(
        valid_part(),
        PART_SECOND_CITATION_LINE,
        "- premises.row-limit.fallback",
    );
    fixture.write("parte-01.md", &lines);

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(1));
    assert_eq!(
        error_details(&output),
        vec!["parte-01.md:8 cited-id-unconfirmed"]
    );
}

#[test]
fn it_should_accept_a_citation_of_a_confirmed_premise_with_a_multi_segment_id() {
    let fixture = Fixture::new();
    fixture.append(vec![node(
        "premises.a.b",
        "premise",
        business_premise_data(),
    )]);
    fixture.write("parte-01.md", &with_line(valid_part(), 7, "- premises.a.b"));

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(json_line(&output.stdout)["ok"], json!(true));
}

#[test]
fn it_should_pass_the_whole_multi_segment_gap_id_of_a_pending_marker_to_the_graph() {
    let fixture = Fixture::new();
    let gap = json!({ "question": "q", "context": "c", "work_item": WORK_ITEM });
    fixture.append(vec![node("gaps.login.timeout", "gap", gap)]);
    let lines = with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "Depende de [PENDENTE: gaps.login.timeout].",
    );
    fixture.write("parte-01.md", &lines);

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(1));
    assert_eq!(
        error_details(&output),
        vec!["parte-01.md:3 pending-gap-unresolved"]
    );
}

#[test]
fn it_should_keep_a_pending_gap_marker_unresolved_when_the_critic_answer_contradicts_a_confirmed_directive()
 {
    let fixture = Fixture::new();
    let gap = json!({
        "question": "q", "context": "c", "work_item": WORK_ITEM, "kind": "technical",
    });
    let directive = json!({
        "text": "t", "kind": "business",
        "confirmed_by": { "by": "person:ana", "question": "q", "answer": "a" },
    });
    let answer = json!({
        "text": "t", "kind": "technical", "status": "confirmed", "work_item": WORK_ITEM,
        "confirmed_by": { "by": "critic", "question": "q", "answer": "a" },
    });
    fixture.append(vec![
        node("gaps.pay-day", "gap", gap),
        node("guidelines.tone", "guideline", directive),
    ]);
    fixture.append_as_critic(vec![node("premises.pay-day", "premise", answer)]);
    fixture.append_as_critic(vec![
        relation("answers", "premises.pay-day", "gaps.pay-day"),
        relation("contradicts", "premises.pay-day", "guidelines.tone"),
    ]);
    let lines = with_line(
        valid_part(),
        PART_OBJECTIVE_LINE,
        "Depende de [PENDENTE: gaps.pay-day].",
    );
    fixture.write("parte-01.md", &lines);

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(1));
    assert_eq!(
        error_details(&output),
        vec!["parte-01.md:3 pending-gap-unresolved"]
    );
}

#[test]
fn it_should_cut_a_cited_id_before_a_dot_that_does_not_continue_the_name() {
    let fixture = Fixture::new();
    fixture.append(vec![node("premises.a", "premise", business_premise_data())]);
    for citation in ["- premises.a.", "- premises.a.B", "- premises.a.-b"] {
        fixture.write(
            "parte-01.md",
            &with_line(valid_part(), PART_SECOND_CITATION_LINE, citation),
        );

        let output = fixture.validate_from_linked();

        assert_eq!(
            output.code,
            Some(0),
            "citation {citation}: {}",
            output.stderr
        );
    }
}

// Emendas na saída: nenhuma está aprovada nestes testes, então todas saem `pending`

#[test]
fn it_should_list_a_valid_amendment_with_no_violations() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write("emenda-01.md", &valid_amendment());

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout)["amendments"],
        json!([{ "name": "emenda-01.md", "state": "pending", "violations": [] }])
    );
}

#[test]
fn it_should_list_an_invalid_amendment_without_failing_ok() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write(
        "emenda-01.md",
        &with_line(
            valid_amendment(),
            AMENDMENT_MOTIVE_BODY_LINE,
            "Motivo TODO.",
        ),
    );

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout),
        json!({
            "ok": true,
            "parts": ["parte-01.md"],
            "amendments": [{
                "name": "emenda-01.md",
                "state": "pending",
                "violations": [{ "path": "emenda-01.md:11", "code": "placeholder-found" }],
            }],
        })
    );
}

#[test]
fn it_should_list_an_amendment_with_a_malformed_file_name_as_invalid() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    fixture.write("emenda-1.md", &valid_amendment());

    let output = fixture.validate_from_linked();

    assert_eq!(
        json_line(&output.stdout)["amendments"],
        json!([{
            "name": "emenda-1.md",
            "state": "pending",
            "violations": [{ "path": "emenda-1.md:1", "code": "amendment-name-invalid" }],
        }])
    );
}

#[test]
fn it_should_not_echo_the_amendment_content_in_the_output() {
    let fixture = Fixture::new();
    fixture.write("parte-01.md", &valid_part());
    let line = format!("Motivo TODO {CANARY}.");
    fixture.write(
        "emenda-01.md",
        &with_line(valid_amendment(), AMENDMENT_MOTIVE_BODY_LINE, &line),
    );

    let output = fixture.validate_from_linked();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(!output.stdout.contains(CANARY), "stdout: {}", output.stdout);
}
