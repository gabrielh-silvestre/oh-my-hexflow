use crate::support;

use std::fs;

use serde_json::{Value, json};
use support::child::{Output, omhl_json};
use support::git::{commit_marker, git_ok, write_executable};
use support::omhl::work_item_data;
use support::project::Project;

const W1: &str = "work-items.w1";
const REVIEW: &str = "checks.config-loosening-review";
const RUN_TESTS: &str = "checks.run-tests";
const CI_GREEN: &str = "checks.ci-green-before-merge";
const COMMANDS: [&str; 2] = ["gate-delivery", "close"];
const WORKFLOW_FILE: &str = ".omhl/workflow.toml";
const DELIVERY_NOTE: &str = "authorized";
const ITEM_BRANCH: &str = "feat/wi-w1__login";
const WORK_FORMAT: &str = "work/<id>/<type>/<slug>";
const WORK_ITEM_BRANCH: &str = "work/w1/fix/login";
const NO_SUCH_REF: &str = "no-such-ref";

// CLI sobre repositório git real: a base da revisão e o `--failed-delivery` vêm da integração declarada

fn run(project: &Project, args: &[&str]) -> Output {
    omhl_json(args).cwd(project.dir()).xdg(project.xdg()).run()
}

fn open_work_item(project: &Project) {
    let mut data = work_item_data("t");
    data["type"] = json!("chore");
    let request = json!({ "items": [{ "body": {
        "kind": "node", "id": W1, "type": "work-item", "data": data,
    } }] });

    let appended = omhl_json(&["append"])
        .cwd(project.dir())
        .xdg(project.xdg())
        .stdin(request.to_string())
        .run();

    assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);
}

fn error_of(output: &Output) -> Value {
    serde_json::from_str::<Value>(output.stderr.trim_end()).expect("stderr is JSON")["error"]
        .clone()
}

fn error_text(output: &Output) -> String {
    error_of(output).to_string()
}

fn text_of(output: &Output) -> String {
    format!("{}{}", output.stdout, output.stderr)
}

fn assert_refused(output: &Output, code: &str) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(error_of(output)["code"], json!(code), "{}", output.stderr);
}

fn work_item_data_of(project: &Project) -> Value {
    let fetched = run(project, &["get", W1]);

    serde_json::from_str::<Value>(fetched.stdout.trim_end()).expect("stdout is JSON")["node"]
        ["data"]
        .clone()
}

fn chain(project: &Project) -> Value {
    let verified = run(project, &["verify"]);
    assert_eq!(verified.code, Some(0), "stderr: {}", verified.stderr);
    let report: Value = serde_json::from_str(verified.stdout.trim_end()).expect("stdout is JSON");

    json!({ "total_records": report["total_records"], "chain_head": report["chain_head"] })
}

fn head(project: &Project) -> String {
    git_ok(project.dir(), &["rev-parse", "HEAD"])
        .trim()
        .to_owned()
}

fn rev(project: &Project, name: &str) -> String {
    git_ok(project.dir(), &["rev-parse", name])
        .trim()
        .to_owned()
}

/// Commita a mudança em `path`; um comentário mantém o `workflow.toml` válido.
fn commit_change(project: &Project, path: &str) -> String {
    let target = project.dir().join(path);
    fs::create_dir_all(target.parent().expect("a parent")).expect("the folder is created");
    let previous = fs::read_to_string(&target).unwrap_or_default();
    fs::write(&target, format!("{previous}# changed\n")).expect("the file is written");
    git_ok(project.dir(), &["add", "-A"]);
    git_ok(project.dir(), &["commit", "-q", "-m", "change"]);

    head(project)
}

/// O `git` do PATH sai com 128 quando um argumento é `word`: o git falhou de verdade, sem ser o exit 1 da pergunta.
fn run_with_git_failing_on(project: &Project, word: &str, args: &[&str]) -> Output {
    let fakebin = project.dir().with_extension("fakebin");
    fs::create_dir_all(&fakebin).expect("fakebin is created");
    let inherited = std::env::var_os("PATH").expect("the test process has a PATH");
    let real_git = std::env::split_paths(&inherited)
        .map(|dir| dir.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git is on the PATH");
    let script = format!(
        "#!/bin/sh\nfor arg in \"$@\"; do [ \"$arg\" = {word} ] && exit 128; done\nexec {} \"$@\"\n",
        real_git.display()
    );
    write_executable(&fakebin.join("git"), &script);
    let path =
        std::env::join_paths(std::iter::once(fakebin).chain(std::env::split_paths(&inherited)))
            .expect("PATH is joined");

    omhl_json(args)
        .cwd(project.dir())
        .xdg(project.xdg())
        .env("PATH", path)
        .run()
}

// Base do `gate-delivery` e do `close`

/// `main` com o check de revisão ligado, o work item `w1` aberto, a integração declarada (se houver) criada no ponto de partida e a `topic` em uso.
struct Repo {
    project: Project,
    fork_point: String,
}

impl Repo {
    fn new(integration: Option<&str>) -> Repo {
        let project = Project::new("acme");
        let table = integration
            .map(|name| format!("\n[branch]\nintegration = \"{name}\"\n"))
            .unwrap_or_default();
        project.write_workflow(&format!(
            "version = 1\n\n[native]\nchecks = [\"{REVIEW}\"]\n{table}"
        ));
        commit_marker(project.dir());
        open_work_item(&project);
        let fork_point = head(&project);
        if let Some(name) = integration {
            git_ok(project.dir(), &["branch", name]);
        }
        git_ok(project.dir(), &["switch", "-q", "-c", "topic"]);

        Repo {
            project,
            fork_point,
        }
    }

    fn git(&self, args: &[&str]) {
        git_ok(self.project.dir(), args);
    }

    fn run(&self, args: &[&str]) -> Output {
        run(&self.project, args)
    }

    fn merge_topic_into(&self, target: &str) {
        self.git(&["switch", "-q", target]);
        self.git(&["merge", "-q", "--no-ff", "-m", "merge topic", "topic"]);
        self.git(&["switch", "-q", "topic"]);
    }

    fn squash_topic_into(&self, target: &str) {
        self.git(&["switch", "-q", target]);
        self.git(&["merge", "-q", "--squash", "topic"]);
        self.git(&["commit", "-q", "-m", "squash topic"]);
        self.git(&["switch", "-q", "topic"]);
    }

    /// Um commit que não está em `topic` nem na integração.
    fn commit_on_side_branch(&self) -> String {
        self.git(&["switch", "-q", "-c", "side", &self.fork_point]);
        let side = commit_change(&self.project, "side.txt");
        self.git(&["switch", "-q", "topic"]);

        side
    }

    fn assert_nothing_recorded(&self) {
        assert_eq!(work_item_data_of(&self.project)["status"], json!("open"));
    }
}

#[test]
fn it_should_print_the_integration_branch_as_the_origin_of_the_base_with_the_merge_base_and_the_trigger()
 {
    for command in COMMANDS {
        let repo = Repo::new(Some("develop"));
        commit_change(&repo.project, WORKFLOW_FILE);

        let text = text_of(&repo.run(&[command, W1]));

        for expected in [
            "integration branch develop",
            repo.fork_point.as_str(),
            WORKFLOW_FILE,
        ] {
            assert!(text.contains(expected), "{command}: {expected} in {text}");
        }
    }
}

#[test]
fn it_should_pass_without_a_base_when_the_integration_range_touches_only_source() {
    for command in COMMANDS {
        let repo = Repo::new(Some("develop"));
        commit_change(&repo.project, "src/a.rs");

        let output = repo.run(&[command, W1]);

        assert_eq!(output.code, Some(0), "{command} stderr: {}", output.stderr);
    }
}

#[test]
fn it_should_pass_a_base_that_gives_the_same_merge_base_as_the_integration() {
    for command in COMMANDS {
        let repo = Repo::new(Some("develop"));
        commit_change(&repo.project, "src/a.rs");

        let output = repo.run(&[command, W1, "--base", &repo.fork_point]);

        assert_eq!(output.code, Some(0), "{command} stderr: {}", output.stderr);
    }
}

#[test]
fn it_should_refuse_a_base_that_gives_another_merge_base_naming_the_option_and_record_nothing() {
    for command in COMMANDS {
        let repo = Repo::new(Some("develop"));
        let first = commit_change(&repo.project, "src/a.rs");
        commit_change(&repo.project, "src/b.rs");
        let before = chain(&repo.project);

        let output = repo.run(&[command, W1, "--base", &first]);

        assert_refused(&output, "review-base-invalid");
        assert!(error_text(&output).contains("--base"), "{command}");
        repo.assert_nothing_recorded();
        assert_eq!(chain(&repo.project), before);
    }
}

#[test]
fn it_should_not_let_a_tag_named_like_the_integration_win_over_the_branch() {
    for command in COMMANDS {
        let repo = Repo::new(Some("develop"));
        let first = commit_change(&repo.project, "src/a.rs");
        commit_change(&repo.project, WORKFLOW_FILE);
        repo.git(&["tag", "develop", &first]);

        let text = text_of(&repo.run(&[command, W1]));

        assert!(text.contains(&repo.fork_point), "{command}: {text}");
        assert!(!text.contains(&first), "{command}: {text}");
    }
}

#[test]
fn it_should_refuse_with_the_declaration_key_when_the_local_ref_is_missing_even_with_the_remote_one()
 {
    for command in COMMANDS {
        let repo = Repo::new(Some("develop"));
        repo.git(&[
            "update-ref",
            "refs/remotes/origin/develop",
            &repo.fork_point,
        ]);
        repo.git(&["branch", "-D", "develop"]);
        commit_change(&repo.project, "src/a.rs");

        let output = repo.run(&[command, W1]);

        assert_refused(&output, "review-base-invalid");
        assert!(
            error_text(&output).contains("branch.integration"),
            "{command}"
        );
        repo.assert_nothing_recorded();
    }
}

#[test]
fn it_should_keep_the_caller_base_rules_without_a_declared_integration() {
    for command in COMMANDS {
        let repo = Repo::new(None);

        let unresolved = repo.run(&[command, W1, "--base", NO_SUCH_REF]);
        let without_base = text_of(&repo.run(&[command, W1]));

        assert_refused(&unresolved, "review-base-invalid");
        assert!(
            without_base.contains("no base"),
            "{command}: {without_base}"
        );
    }
}

#[test]
fn it_should_look_only_at_the_declared_integration_and_ignore_a_local_develop_for_the_base() {
    for command in COMMANDS {
        let repo = Repo::new(Some("trunk-int"));
        let first = commit_change(&repo.project, "src/a.rs");
        commit_change(&repo.project, WORKFLOW_FILE);
        repo.git(&["branch", "develop", &first]);

        let text = text_of(&repo.run(&[command, W1]));

        for expected in ["integration branch trunk-int", repo.fork_point.as_str()] {
            assert!(text.contains(expected), "{command}: {expected} in {text}");
        }
        assert!(!text.contains(&first), "{command}: {text}");
    }
}

#[test]
fn it_should_name_the_declaration_key_and_not_develop_when_the_declared_ref_is_missing() {
    let repo = Repo::new(Some("trunk-int"));
    repo.git(&["branch", "-D", "trunk-int"]);

    let output = repo.run(&["gate-delivery", W1]);

    assert_refused(&output, "review-base-invalid");
    let text = error_text(&output);
    assert!(text.contains("branch.integration"), "{text}");
    assert!(!text.contains("develop"), "{text}");
}

#[test]
fn it_should_take_the_integration_from_the_main_directory_and_never_from_the_branch_copy() {
    let repo = Repo::new(Some("develop"));
    let linked = repo.project.dir().with_extension("linked");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        ITEM_BRANCH,
        linked.to_str().expect("utf-8"),
    ]);
    fs::write(
        linked.join(WORKFLOW_FILE),
        format!(
            "version = 1\n\n[native]\nchecks = [\"{REVIEW}\"]\n\n[branch]\nintegration = \"trunk-int\"\n"
        ),
    )
    .expect("the branch copy is written");
    git_ok(&linked, &["add", "-A"]);
    git_ok(
        &linked,
        &["commit", "-q", "-m", "declare another integration"],
    );
    fs::write(
        repo.project.dir().join(WORKFLOW_FILE),
        "version = 1\ncolour = true\n",
    )
    .expect("the main file is broken");

    let gate = omhl_json(&["gate-delivery", W1, "--base", "main"])
        .cwd(&linked)
        .xdg(repo.project.xdg())
        .run();

    let text = text_of(&gate);
    assert!(text.contains("integration branch develop"), "{text}");
    fs::remove_dir_all(linked).expect("the worktree is removed");
}

// Depois do merge: o `HEAD` já está na integração

#[test]
fn it_should_print_the_caller_base_with_the_note_that_the_integration_holds_the_head_after_the_merge()
 {
    let repo = Repo::new(Some("develop"));
    commit_change(&repo.project, WORKFLOW_FILE);
    repo.merge_topic_into("develop");

    let closed = repo.run(&["close", W1, "--base", &repo.fork_point]);

    assert_refused(&closed, "close-refused");
    let text = text_of(&closed);
    for expected in [
        "caller --base",
        "integration branch develop",
        repo.fork_point.as_str(),
        WORKFLOW_FILE,
    ] {
        assert!(text.contains(expected), "{expected} in {text}");
    }
    repo.assert_nothing_recorded();
}

#[test]
fn it_should_close_after_the_merge_when_the_caller_base_range_touches_only_source() {
    let repo = Repo::new(Some("develop"));
    commit_change(&repo.project, "src/a.rs");
    repo.merge_topic_into("develop");

    let closed = repo.run(&["close", W1, "--base", &repo.fork_point]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
}

#[test]
fn it_should_refuse_after_the_merge_a_base_that_does_not_resolve_is_not_in_the_integration_or_is_the_head()
 {
    let repo = Repo::new(Some("develop"));
    commit_change(&repo.project, "src/a.rs");
    let side = repo.commit_on_side_branch();
    repo.merge_topic_into("develop");
    let before = chain(&repo.project);

    for base in [NO_SUCH_REF, side.as_str(), "HEAD"] {
        let output = repo.run(&["close", W1, "--base", base]);

        assert_refused(&output, "review-base-invalid");
        assert!(error_text(&output).contains("--base"), "{base}");
    }
    repo.assert_nothing_recorded();
    assert_eq!(chain(&repo.project), before);
}

#[test]
fn it_should_require_the_review_after_the_merge_when_no_base_is_given_and_say_so() {
    let repo = Repo::new(Some("develop"));
    commit_change(&repo.project, "src/a.rs");
    repo.merge_topic_into("develop");

    let closed = repo.run(&["close", W1]);

    assert_refused(&closed, "close-refused");
    let text = text_of(&closed);
    for expected in ["no base", "develop"] {
        assert!(text.contains(expected), "{expected} in {text}");
    }
}

#[test]
fn it_should_refuse_after_the_merge_naming_the_declaration_key_when_the_local_ref_is_missing() {
    let repo = Repo::new(Some("develop"));
    commit_change(&repo.project, "src/a.rs");
    repo.merge_topic_into("develop");
    repo.git(&["branch", "-D", "develop"]);

    let output = repo.run(&["close", W1, "--base", &repo.fork_point]);

    assert_refused(&output, "review-base-invalid");
    assert!(error_text(&output).contains("branch.integration"));
}

#[test]
fn it_should_keep_the_merge_base_path_after_a_squash_merge() {
    let repo = Repo::new(Some("develop"));
    commit_change(&repo.project, WORKFLOW_FILE);
    repo.squash_topic_into("develop");

    let closed = repo.run(&["close", W1]);

    assert_refused(&closed, "close-refused");
    let text = text_of(&closed);
    for expected in ["integration branch develop", repo.fork_point.as_str()] {
        assert!(text.contains(expected), "{expected} in {text}");
    }
}

#[test]
fn it_should_exit_probe_failed_when_the_ancestor_probe_fails_with_another_exit() {
    let repo = Repo::new(Some("develop"));
    commit_change(&repo.project, "src/a.rs");
    repo.merge_topic_into("develop");

    let output = run_with_git_failing_on(
        &repo.project,
        "--is-ancestor",
        &["close", W1, "--base", &repo.fork_point],
    );

    assert_refused(&output, "probe-failed");
    repo.assert_nothing_recorded();
}

// `close --failed-delivery`: a integração é a declarada

/// Item `w1` com um commit próprio na branch dada e o gate reprovado pelos checks ligados.
struct Delivery {
    project: Project,
}

impl Delivery {
    fn new(branch_table: &str, branch: &str) -> Delivery {
        let project = Project::new("acme");
        project.write_workflow(&format!(
            "version = 1\n\n[native]\nchecks = [\"{RUN_TESTS}\", \"{CI_GREEN}\", \"{REVIEW}\"]\n{branch_table}"
        ));
        commit_marker(project.dir());
        open_work_item(&project);
        git_ok(project.dir(), &["switch", "-q", "-c", branch]);
        fs::write(project.dir().join("feature.txt"), "feature\n").expect("the feature is written");
        git_ok(project.dir(), &["add", "-A"]);
        git_ok(project.dir(), &["commit", "-q", "-m", "the item"]);

        Delivery { project }
    }

    fn branch(&self) -> String {
        git_ok(self.project.dir(), &["branch", "--show-current"])
            .trim()
            .to_owned()
    }

    /// Cria `target` a partir da `main` (se falta) e junta nela a branch do item com merge commit.
    fn integrate_into(&self, target: &str) {
        let item = self.branch();
        let dir = self.project.dir();
        if git_ok(dir, &["branch", "--list", target]).trim().is_empty() {
            git_ok(dir, &["branch", target, "main"]);
        }
        git_ok(dir, &["switch", "-q", target]);
        git_ok(
            dir,
            &["merge", "-q", "--no-ff", "-m", "merge the item", &item],
        );
        git_ok(dir, &["switch", "-q", &item]);
    }

    fn side_commit(&self) -> String {
        let item = self.branch();
        git_ok(self.project.dir(), &["switch", "-q", "-c", "side", "main"]);
        let side = commit_change(&self.project, "side.txt");
        git_ok(self.project.dir(), &["switch", "-q", &item]);

        side
    }

    fn failed(&self, extra: &[&str]) -> Output {
        let mut args = vec!["close", W1, "--failed-delivery", "--note", DELIVERY_NOTE];
        args.extend_from_slice(extra);

        run(&self.project, &args)
    }

    fn assert_refused_without_recording(&self, extra: &[&str], code: &str) -> Output {
        let before = chain(&self.project);

        let output = self.failed(extra);

        assert_refused(&output, code);
        assert_eq!(chain(&self.project), before);

        output
    }
}

const DEVELOP_TABLE: &str = "\n[branch]\nintegration = \"develop\"\n";

#[test]
fn it_should_recognize_the_work_item_branch_of_a_declared_format_in_the_failed_delivery() {
    let table = format!("\n[branch]\nformat = \"{WORK_FORMAT}\"\nintegration = \"develop\"\n");
    let delivery = Delivery::new(&table, WORK_ITEM_BRANCH);
    delivery.integrate_into("develop");

    let closed = delivery.failed(&[]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(
        work_item_data_of(&delivery.project)["delivery_failure"]["integration"],
        json!({ "ref": "develop", "commit": rev(&delivery.project, "refs/heads/develop") })
    );
}

#[test]
fn it_should_record_the_declared_name_as_the_integration_ref() {
    let delivery = Delivery::new("\n[branch]\nintegration = \"trunk-int\"\n", ITEM_BRANCH);
    delivery.integrate_into("trunk-int");

    let closed = delivery.failed(&[]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(
        work_item_data_of(&delivery.project)["delivery_failure"]["integration"],
        json!({ "ref": "trunk-int", "commit": rev(&delivery.project, "refs/heads/trunk-int") })
    );
}

#[test]
fn it_should_refuse_when_only_a_local_develop_holds_the_item_and_another_integration_is_declared() {
    let delivery = Delivery::new("\n[branch]\nintegration = \"trunk-int\"\n", ITEM_BRANCH);
    delivery.integrate_into("develop");
    git_ok(delivery.project.dir(), &["branch", "trunk-int", "main"]);

    delivery.assert_refused_without_recording(&[], "delivery-not-integrated");
}

#[test]
fn it_should_refuse_with_the_integration_refusals_before_judging_the_base() {
    let delivery = Delivery::new(DEVELOP_TABLE, ITEM_BRANCH);
    git_ok(delivery.project.dir(), &["branch", "develop", "main"]);

    delivery.assert_refused_without_recording(&["--base", NO_SUCH_REF], "delivery-not-integrated");
}

#[test]
fn it_should_refuse_with_delivery_not_integrated_before_judging_the_base_when_nothing_is_declared()
{
    let delivery = Delivery::new("", ITEM_BRANCH);
    delivery.integrate_into("develop");

    delivery.assert_refused_without_recording(&["--base", NO_SUCH_REF], "delivery-not-integrated");
}

#[test]
fn it_should_refuse_with_the_work_item_branch_before_judging_the_base() {
    let delivery = Delivery::new(DEVELOP_TABLE, ITEM_BRANCH);
    delivery.integrate_into("develop");
    git_ok(delivery.project.dir(), &["switch", "-q", "develop"]);

    delivery.assert_refused_without_recording(
        &["--base", NO_SUCH_REF],
        "delivery-not-on-work-item-branch",
    );
}

#[test]
fn it_should_refuse_a_base_that_does_not_resolve_after_the_integration_passes_naming_the_option() {
    let delivery = Delivery::new(DEVELOP_TABLE, ITEM_BRANCH);
    delivery.integrate_into("develop");

    let output =
        delivery.assert_refused_without_recording(&["--base", NO_SUCH_REF], "review-base-invalid");

    assert!(error_text(&output).contains("--base"));
}

#[test]
fn it_should_refuse_a_base_outside_the_integration_after_it_passes_naming_the_option() {
    let delivery = Delivery::new(DEVELOP_TABLE, ITEM_BRANCH);
    delivery.integrate_into("develop");
    let side = delivery.side_commit();

    let output =
        delivery.assert_refused_without_recording(&["--base", &side], "review-base-invalid");

    assert!(error_text(&output).contains("--base"));
}

#[test]
fn it_should_record_the_failure_with_a_caller_base_that_is_an_ancestor_of_the_integration() {
    let delivery = Delivery::new(DEVELOP_TABLE, ITEM_BRANCH);
    delivery.integrate_into("develop");

    let closed = delivery.failed(&["--base", "main"]);

    assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    assert_eq!(
        work_item_data_of(&delivery.project)["status"],
        json!("closed")
    );
}
