use crate::support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use ::omhl::byproduct::ByproductStore;
use ::omhl::facade::{Omhl, OmhlDeps};
use ::omhl::ports::blob::BlobStore;
use hexlog::{AppendRequest, Config};
use serde_json::{Value, json};
use support::child::{Output, omhl, omhl_json};
use support::ext::ExtTree;
use support::omhl::{acme, attestation_id_for, node_item, with_snapshot, work_item_data};
use support::project::Project;
use support::{FakeProbe, FixedClock, SeqIds, executor_attestation_data, executor_author, snap};

const LINT_RUN_TOML: &str = r#"blocking = true
how = "Run lint"
run = ["cargo", "clippy"]
timeout_secs = 60
expect = [{ exit = 0, outcome = "passed" }]

[when]
event = "before-pr"
"#;
const CHECK: &str = "checks.lint";
const W1: &str = "work-items.w1";
const W2: &str = "work-items.w2";

// SHA-256 dos blobs montados à mão (oráculo independente do código)
const B1000_SHA: &str = "b01dcaddfdccba222da3cd3f9174b32adf001a0d41ce28916a0aee38046552b8";
const BLONG_SHA: &str = "ca8e77d1adc23806e3800feefb40538e3469570a9fe516b6e84a3259fb29e5fe";
const BSMALL_SHA: &str = "a834c640cc27439d8a70b00733ebf8165a1a9d5aac5ccb6a48cb33dff45c48cd";
const BOLD_SHA: &str = "a70000123a86ac68ca89f9d24c68274450d0219382a369353d8125183681fa39";
const BWINDOW_SHA: &str = "246f6399fcbb83c92c027d999aec980cae16c2e6f0a828a3161844f2f6ccf515";
const BNEW_SHA: &str = "ee08c8b2e66a380ea0f78adb2f10ca49ed60f588af8dade87cd957e9d0c89feb";
const BTWOLONG_SHA: &str = "8e42172b4b8e767a12b5b636416fe8334fe22f234130d2ac62348d95a353fe3b";
const BSECRET_SHA: &str = "161396cc48aa255b2254043d79d51620e24bf2e1e5ae92aeb978b57d2d7009fe";
const BMULTIBYTE_SHA: &str = "b89360b3f394ce566fe51579db3e5ff44f551dff5314abdbfbf2a1069edcad08";

// Apoio

struct Fixture {
    project: Project,
    clock: FixedClock,
    ids: SeqIds,
}

impl Fixture {
    /// Projeto fora de repositório git, com `checks.lint` carregado e os work items `w1` e `w2` abertos.
    fn new() -> Fixture {
        let project = Project::new("acme");
        ExtTree::at(project.dir()).check("lint", LINT_RUN_TOML);
        let fixture = Fixture {
            project,
            clock: FixedClock::default(),
            ids: SeqIds::new(),
        };
        let loaded = fixture.omhl(&["load"]);
        assert_eq!(loaded.code, Some(0), "stderr: {}", loaded.stderr);
        let items: Vec<Value> = [W1, W2]
            .iter()
            .map(|id| {
                json!({ "body": {
                    "kind": "node", "id": id, "type": "work-item", "data": work_item_data("t"),
                } })
            })
            .collect();
        let appended = omhl(&["append"])
            .cwd(fixture.project.dir())
            .xdg(fixture.project.xdg())
            .stdin(json!({ "items": items }).to_string())
            .run();
        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);

        fixture
    }

    fn omhl(&self, args: &[&str]) -> Output {
        omhl_json(args)
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .run()
    }

    /// Grava o blob `cabeçalho + stdout + stderr` pelo `BlobStore` real e confere o hash do oráculo.
    fn put_blob(&self, stdout: &str, stderr: &str, expected_sha: &str) -> String {
        let mut bytes = format!(
            "omhl-byproduct v1 stdout={} stderr={}\n",
            stdout.len(),
            stderr.len()
        )
        .into_bytes();
        bytes.extend_from_slice(stdout.as_bytes());
        bytes.extend_from_slice(stderr.as_bytes());
        let store = BlobStore::new(&self.project.db_path(), &acme());

        let hash = store.put(&bytes).expect("the blob is stored");

        assert_eq!(hash.as_str(), expected_sha);
        hash.as_str().to_owned()
    }

    fn blob_file(&self, sha: &str) -> PathBuf {
        self.project
            .xdg()
            .join("omhl/blobs/by-db/omhl.db/acme")
            .join(&sha[..2])
            .join(sha)
    }

    fn legacy_blob_file(&self, sha: &str) -> PathBuf {
        self.project
            .xdg()
            .join("omhl/blobs")
            .join(&sha[..2])
            .join(sha)
    }

    /// Deixa o blob só no layout antigo `blobs/<xx>/<hash>`.
    fn move_to_legacy(&self, sha: &str) {
        let legacy = self.legacy_blob_file(sha);
        fs::create_dir_all(legacy.parent().expect("the blob has a folder"))
            .expect("the legacy folder is created");
        fs::rename(self.blob_file(sha), legacy).expect("the blob is moved");
    }

    /// Grava a attestation de `checks.lint` em `snap(fill)` e devolve o id dela.
    fn attest(&self, fill: char, work_item: &str, outcome: &str, byproduct: &str) -> String {
        let probe = FakeProbe::clean(snap(fill));
        let deps = OmhlDeps {
            clock: &self.clock,
            ids: &self.ids,
            probe: &probe,
            session: &crate::support::NO_SESSION,
        };
        let mut db = Omhl::open(&Config::new(self.project.db_path()), deps).expect("omhl opens");
        db.ensure_tenant(&acme()).expect("the tenant is created");
        let id = attestation_id_for(&snap(fill), CHECK, work_item);
        let data = executor_attestation_data(CHECK, work_item, outcome, byproduct);
        let item = with_snapshot(node_item(&id, "attestation", data), &snap(fill));

        db.append(&AppendRequest {
            tenant: acme(),
            author: executor_author(),
            key: None,
            items: vec![item],
        })
        .expect("the attestation is appended");

        id
    }
}

fn json_line(text: &str) -> Value {
    assert_eq!(text.lines().count(), 1, "expected one line, got: {text:?}");

    serde_json::from_str(text).expect("the line is JSON")
}

fn success(output: &Output) -> Value {
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(output.stderr, "");

    json_line(&output.stdout)
}

fn failure(output: &Output) -> Value {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(output.stdout, "");

    json_line(&output.stderr)
}

fn lines_of(result: &Value) -> Vec<&str> {
    result["lines"]
        .as_array()
        .expect("lines is an array")
        .iter()
        .map(|line| line.as_str().expect("a line is text"))
        .collect()
}

/// Blob sem máscara, como o gravado antes da redação: um bloco PEM de várias linhas no `stdout` e uma atribuição no `stderr`.
fn put_bsecret(fixture: &Fixture) -> String {
    let stdout = "build ok\n-----BEGIN RSA PRIVATE KEY-----\nMIIEabc\nxyz123\n-----END RSA PRIVATE KEY-----\ndone\n";

    fixture.put_blob(stdout, "API_TOKEN=abcdef123456\n", BSECRET_SHA)
}

/// stdout `out 1..out 998` e stderr `err 1`, `err 2`: 1.000 linhas.
fn put_b1000(fixture: &Fixture) -> String {
    let stdout: String = (1..=998).map(|n| format!("out {n}\n")).collect();

    fixture.put_blob(&stdout, "err 1\nerr 2\n", B1000_SHA)
}

fn put_bsmall(fixture: &Fixture) -> String {
    fixture.put_blob("one\ntwo\n", "three\n", BSMALL_SHA)
}

// Testes

#[test]
fn it_should_return_the_last_lines_with_stderr_last() {
    let fixture = Fixture::new();
    let blob = put_b1000(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "3"]);

    assert_eq!(
        success(&output),
        json!({
            "check": CHECK,
            "work_item": W1,
            "outcome": "failed",
            "byproduct": B1000_SHA,
            "lines": ["out 998", "err 1", "err 2"],
        })
    );
}

#[test]
fn it_should_default_to_the_last_50_lines() {
    let fixture = Fixture::new();
    let blob = put_b1000(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1]);

    let result = success(&output);
    let lines = lines_of(&result);
    assert_eq!(lines.len(), 50);
    assert_eq!(lines[0], "out 951");
    assert_eq!(lines[47], "out 998");
    assert_eq!(lines[48], "err 1");
    assert_eq!(lines[49], "err 2");
}

#[test]
fn it_should_refuse_tail_above_500() {
    let fixture = Fixture::new();
    let blob = put_b1000(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "501"]);

    let error = failure(&output);
    assert_eq!(error["error"]["code"], json!("invalid-arguments"));
    assert_eq!(error["error"]["details"][0]["path"], json!("/tail"));
}

#[test]
fn it_should_accept_tail_of_exactly_500() {
    let fixture = Fixture::new();
    let blob = put_b1000(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "500"]);

    let result = success(&output);
    let lines = lines_of(&result);
    assert_eq!(lines.len(), 500);
    assert_eq!(lines[0], "out 501");
    assert_eq!(lines[499], "err 2");
}

#[test]
fn it_should_explain_the_tail_rule() {
    let fixture = Fixture::new();
    let blob = put_b1000(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let below = failure(&fixture.omhl(&["byproduct", CHECK, W1, "--tail", "0"]));
    let above = failure(&fixture.omhl(&["byproduct", CHECK, W1, "--tail", "501"]));

    for error in [below, above] {
        assert_eq!(
            error["error"]["message"],
            json!("tail must be between 1 and 500")
        );
        assert_eq!(
            error["error"]["details"][0]["message"],
            json!("tail must be between 1 and 500")
        );
    }
}

#[test]
fn it_should_reject_a_tail_of_zero() {
    let fixture = Fixture::new();
    let blob = put_b1000(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "0"]);

    let error = failure(&output);
    assert_eq!(error["error"]["code"], json!("invalid-arguments"));
    assert_eq!(error["error"]["details"][0]["path"], json!("/tail"));
}

#[test]
fn it_should_mask_secrets_of_a_blob_stored_without_redaction() {
    let fixture = Fixture::new();
    let blob = put_bsecret(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1]);

    let result = success(&output);
    assert_eq!(
        lines_of(&result),
        [
            "build ok",
            "[REDACTED:private-key]",
            "done",
            "API_TOKEN=[REDACTED]"
        ]
    );
    assert_eq!(result["byproduct"], json!(BSECRET_SHA));
}

#[test]
fn it_should_count_the_tail_window_over_the_masked_lines() {
    let fixture = Fixture::new();
    let blob = put_bsecret(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "3"]);

    assert_eq!(
        lines_of(&success(&output)),
        ["[REDACTED:private-key]", "done", "API_TOKEN=[REDACTED]"]
    );
}

#[test]
fn it_should_leave_the_blob_file_unredacted_on_disk() {
    let fixture = Fixture::new();
    let blob = put_bsecret(&fixture);
    fixture.attest('a', W1, "failed", &blob);
    fixture.omhl(&["byproduct", CHECK, W1]);

    let on_disk = fs::read_to_string(fixture.blob_file(BSECRET_SHA)).expect("the blob is stored");

    assert!(on_disk.contains("MIIEabc"));
}

#[test]
fn it_should_mask_secrets_of_a_legacy_layout_blob() {
    let fixture = Fixture::new();
    let blob = put_bsecret(&fixture);
    fixture.move_to_legacy(&blob);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1]);

    let result = success(&output);
    assert_eq!(
        lines_of(&result),
        [
            "build ok",
            "[REDACTED:private-key]",
            "done",
            "API_TOKEN=[REDACTED]"
        ]
    );
    assert_eq!(result["byproduct"], json!(BSECRET_SHA));
}

#[test]
fn it_should_count_the_tail_window_over_the_masked_lines_of_a_legacy_layout_blob() {
    let fixture = Fixture::new();
    let blob = put_bsecret(&fixture);
    fixture.move_to_legacy(&blob);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "3"]);

    assert_eq!(
        lines_of(&success(&output)),
        ["[REDACTED:private-key]", "done", "API_TOKEN=[REDACTED]"]
    );
}

#[test]
fn it_should_leave_a_legacy_layout_blob_unredacted_on_disk() {
    let fixture = Fixture::new();
    let blob = put_bsecret(&fixture);
    fixture.move_to_legacy(&blob);
    fixture.attest('a', W1, "failed", &blob);
    fixture.omhl(&["byproduct", CHECK, W1]);

    let on_disk =
        fs::read_to_string(fixture.legacy_blob_file(BSECRET_SHA)).expect("the blob is stored");

    assert!(on_disk.contains("MIIEabc"));
}

#[test]
fn it_should_cap_the_size_of_each_returned_line() {
    let fixture = Fixture::new();
    let blob = fixture.put_blob(&format!("{}\n", "x".repeat(2500)), "", BLONG_SHA);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "1"]);

    assert_eq!(lines_of(&success(&output)), ["x".repeat(2000)]);
}

#[test]
fn it_should_report_truncated_lines() {
    let fixture = Fixture::new();
    let blob = fixture.put_blob(&format!("{}\n", "x".repeat(2500)), "", BLONG_SHA);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "1"]);

    assert_eq!(success(&output)["truncated_lines"], json!(1));
}

#[test]
fn it_should_omit_truncated_lines_when_none() {
    let fixture = Fixture::new();
    let blob = put_bsmall(&fixture);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1]);

    assert!(success(&output).get("truncated_lines").is_none());
}

#[test]
fn it_should_count_only_the_returned_lines_that_were_truncated() {
    let fixture = Fixture::new();
    let stdout = format!("{}\n{}\nshort\n", "a".repeat(2500), "b".repeat(2500));
    let blob = fixture.put_blob(&stdout, "", BWINDOW_SHA);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "2"]);

    assert_eq!(success(&output)["truncated_lines"], json!(1));
}

#[test]
fn it_should_count_every_truncated_line_in_the_window() {
    let fixture = Fixture::new();
    let stdout = format!("{}\n{}\n", "a".repeat(2500), "b".repeat(2500));
    let blob = fixture.put_blob(&stdout, "", BTWOLONG_SHA);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "2"]);

    let result = success(&output);
    assert_eq!(result["truncated_lines"], json!(2));
    assert_eq!(lines_of(&result), ["a".repeat(2000), "b".repeat(2000)]);
}

#[test]
fn it_should_cut_a_multibyte_line_at_a_character_boundary() {
    let fixture = Fixture::new();
    let blob = fixture.put_blob(&format!("{}\n", "é".repeat(2001)), "", BMULTIBYTE_SHA);
    fixture.attest('a', W1, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1, "--tail", "1"]);

    let result = success(&output);
    assert_eq!(lines_of(&result), ["é".repeat(2000)]);
    assert_eq!(result["truncated_lines"], json!(1));
}

#[test]
fn it_should_answer_byproduct_missing_when_the_blob_is_gone() {
    let fixture = Fixture::new();
    let blob = put_bsmall(&fixture);
    fixture.attest('a', W1, "failed", &blob);
    fs::remove_file(fixture.blob_file(&blob)).expect("the blob file is removed");

    let output = fixture.omhl(&["byproduct", CHECK, W1]);

    assert_eq!(
        failure(&output)["error"]["code"],
        json!("byproduct-missing")
    );
}

#[test]
fn it_should_answer_byproduct_corrupt_when_the_blob_bytes_do_not_match_its_hash() {
    let fixture = Fixture::new();
    let blob = put_bsmall(&fixture);
    fixture.attest('a', W1, "failed", &blob);
    fs::write(fixture.blob_file(&blob), b"").expect("the blob file is truncated");

    let output = fixture.omhl(&["byproduct", CHECK, W1]);

    assert_eq!(
        failure(&output)["error"]["code"],
        json!("byproduct-corrupt")
    );
}

#[test]
fn it_should_keep_the_attestation_valid_when_the_blob_is_gone() {
    let fixture = Fixture::new();
    let blob = put_bsmall(&fixture);
    let id = fixture.attest('a', W1, "failed", &blob);
    let before = json_line(&fixture.omhl(&["get", &id]).stdout)["node"].clone();
    fs::remove_file(fixture.blob_file(&blob)).expect("the blob file is removed");

    let lookup = fixture.omhl(&["byproduct", CHECK, W1]);

    let after = json_line(&fixture.omhl(&["get", &id]).stdout)["node"].clone();
    let verified = success(&fixture.omhl(&["verify"]));
    assert_eq!(
        failure(&lookup)["error"]["code"],
        json!("byproduct-missing")
    );
    assert_eq!(after, before);
    assert_eq!(after["data"]["outcome"], json!("failed"));
    assert_eq!(after["data"]["byproduct"], json!(BSMALL_SHA));
    assert_eq!(verified["ok"], json!(true));
    assert_eq!(verified["breaks"], json!([]));
}

#[test]
fn it_should_answer_check_not_found_for_an_unknown_check() {
    let fixture = Fixture::new();

    let output = fixture.omhl(&["byproduct", "checks.nope", W1]);

    assert_eq!(failure(&output)["error"]["code"], json!("check-not-found"));
}

#[test]
fn it_should_answer_attestation_not_found_for_a_check_never_run_on_the_work_item() {
    let fixture = Fixture::new();
    let blob = put_bsmall(&fixture);
    fixture.attest('a', W2, "failed", &blob);

    let output = fixture.omhl(&["byproduct", CHECK, W1]);

    assert_eq!(
        failure(&output)["error"]["code"],
        json!("attestation-not-found")
    );
}

#[test]
fn it_should_pick_the_attestation_with_the_highest_sequence() {
    let fixture = Fixture::new();
    let old = fixture.put_blob("old\n", "", BOLD_SHA);
    let new = fixture.put_blob("new\n", "", BNEW_SHA);
    fixture.attest('b', W1, "passed", &old);
    fixture.attest('a', W1, "failed", &new);

    let output = fixture.omhl(&["byproduct", CHECK, W1]);

    assert_eq!(
        success(&output),
        json!({
            "check": CHECK,
            "work_item": W1,
            "outcome": "failed",
            "byproduct": BNEW_SHA,
            "lines": ["new"],
        })
    );
}

#[test]
fn it_should_reject_a_work_item_above_256_characters() {
    let fixture = Fixture::new();
    let long_work_item = format!("work-items.{}", "a".repeat(256));

    let output = fixture.omhl(&["byproduct", CHECK, &long_work_item]);

    assert_eq!(
        failure(&output)["error"]["details"][0]["code"],
        json!("where-eq-value-too-long")
    );
}

// byproduct prune

const EIGHT_DAYS: Duration = Duration::from_secs(8 * 24 * 60 * 60);
const THIRTY_DAYS: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Dá ao arquivo a data de `age` atrás, como se ele tivesse sido gravado naquele dia.
fn age_file(path: &Path, age: Duration) {
    let file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("the file opens");
    file.set_modified(SystemTime::now() - age)
        .expect("the date is set");
}

#[test]
fn it_should_prune_a_stale_output_and_keep_the_live_one_with_the_counts() {
    let fixture = Fixture::new();
    let live = put_bsmall(&fixture);
    fixture.attest('a', W1, "failed", &live);
    put_b1000(&fixture);
    age_file(&fixture.blob_file(BSMALL_SHA), THIRTY_DAYS);
    age_file(&fixture.blob_file(B1000_SHA), EIGHT_DAYS);
    let stale_bytes = fs::metadata(fixture.blob_file(B1000_SHA))
        .expect("the stale blob exists")
        .len();

    let output = fixture.omhl(&["byproduct", "prune"]);

    assert_eq!(
        success(&output),
        json!({ "removed": 1, "kept_live": 1, "freed_bytes": stale_bytes })
    );
    assert!(!fixture.blob_file(B1000_SHA).exists());
    assert!(fixture.blob_file(BSMALL_SHA).exists());
}

#[test]
fn it_should_report_zeros_and_exit_zero_when_nothing_is_stored() {
    let fixture = Fixture::new();

    let output = fixture.omhl(&["byproduct", "prune"]);

    assert_eq!(
        success(&output),
        json!({ "removed": 0, "kept_live": 0, "freed_bytes": 0 })
    );
}

#[test]
fn it_should_prune_again_even_with_a_fresh_daily_stamp() {
    let fixture = Fixture::new();
    fixture.omhl(&["byproduct", "prune"]);
    put_b1000(&fixture);
    age_file(&fixture.blob_file(B1000_SHA), EIGHT_DAYS);

    let output = fixture.omhl(&["byproduct", "prune"]);

    assert_eq!(success(&output)["removed"], json!(1));
}

#[test]
fn it_should_answer_byproduct_prune_failed_when_the_folder_cannot_be_read() {
    let fixture = Fixture::new();
    put_b1000(&fixture);
    let shard = fixture
        .blob_file(B1000_SHA)
        .parent()
        .expect("the blob has a shard")
        .to_path_buf();
    age_file(&fixture.blob_file(B1000_SHA), EIGHT_DAYS);
    fs::set_permissions(&shard, fs::Permissions::from_mode(0o500)).expect("the shard is locked");

    let output = fixture.omhl(&["byproduct", "prune"]);

    fs::set_permissions(&shard, fs::Permissions::from_mode(0o700)).expect("the shard is unlocked");
    assert_eq!(
        failure(&output)["error"]["code"],
        json!("byproduct-prune-failed")
    );
}

#[test]
fn it_should_refuse_byproduct_without_arguments_as_a_usage_error() {
    let fixture = Fixture::new();

    let output = fixture.omhl(&["byproduct"]);

    assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
}

#[test]
fn it_should_not_prune_when_an_option_comes_before_the_prune_word() {
    let fixture = Fixture::new();
    put_b1000(&fixture);
    age_file(&fixture.blob_file(B1000_SHA), EIGHT_DAYS);

    let output = fixture.omhl(&["byproduct", "--tail", "3", "prune"]);

    assert_eq!(output.code, Some(2), "stderr: {}", output.stderr);
    assert!(fixture.blob_file(B1000_SHA).exists());
}
