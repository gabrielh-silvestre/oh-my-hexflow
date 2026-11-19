use crate::support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::child::{Output as Child, omhl, omhl_json};
use support::git::git_in;
use support::omhl::work_item_data;
use support::project::Project;
use tempfile::TempDir;

const WORK_ITEM: &str = "work-items.w1";
const SLUG: &str = "login";
const BRANCH: &str = "feat/wi-w1__login";
const DEFAULT_SPECS: &str = ".omhl/specs";
const PART_DIR: &str = "wi-w1__login";
const SECTIONS: [&str; 5] = [
    "## Objetivo",
    "## Diretivas e premissas aplicáveis",
    "## Comportamentos",
    "## Critérios de aceitação",
    "## Fora de escopo",
];
const CUSTOM_WORKFLOW: &str = "version = 1\n\n[specs]\ndir = \"docs/team-specs\"\n";
const WORKFLOW_PATH: &str = ".omhl/workflow.toml";

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
        let appended = fixture.append_work_item();
        assert_eq!(appended.code, Some(0), "stderr: {}", appended.stderr);

        fixture
    }

    fn append_work_item(&self) -> Child {
        let request = json!({
            "items": [{ "body": {
                "kind": "node", "id": WORK_ITEM, "type": "work-item",
                "data": work_item_data("t"),
            } }]
        });

        omhl(&["append"])
            .cwd(self.project.dir())
            .xdg(self.project.xdg())
            .stdin(request.to_string())
            .run()
    }

    fn close_work_item(&self) {
        let closed = self.run_in(self.project.dir(), &["close", WORK_ITEM]);

        assert_eq!(closed.code, Some(0), "stderr: {}", closed.stderr);
    }

    fn run_in(&self, cwd: &Path, args: &[&str]) -> Child {
        omhl_json(args).cwd(cwd).xdg(self.project.xdg()).run()
    }

    fn init_from_linked(&self, work_item: &str, slug: &str) -> Child {
        self.run_in(&self.linked, &["spec", "init", work_item, slug])
    }

    fn main_dir(&self) -> PathBuf {
        fs::canonicalize(self.project.dir()).expect("the main directory exists")
    }

    fn part_path(&self, specs_dir: &str) -> PathBuf {
        self.main_dir()
            .join(specs_dir)
            .join(PART_DIR)
            .join("parte-01.md")
    }

    /// Pasta de spec de outro work item, já com uma parte, direto no diretório principal.
    fn seed_spec_dir(&self, name: &str) {
        let dir = self.project.dir().join(DEFAULT_SPECS).join(name);
        fs::create_dir_all(&dir).expect("spec directory is created");
        fs::write(dir.join("parte-01.md"), "seed\n").expect("part is written");
    }

    fn seed_other_work_items(&self, count: usize) {
        for n in 0..count {
            self.seed_spec_dir(&format!("wi-o{n:03}__x"));
        }
    }

    /// `count` arquivos vazios direto no `specs.dir`, que só contam como entradas da pasta.
    fn seed_entries(&self, count: usize) {
        let dir = self.project.dir().join(DEFAULT_SPECS);
        fs::create_dir_all(&dir).expect("specs directory is created");
        for n in 0..count {
            fs::write(dir.join(format!("entry-{n:05}")), "").expect("entry is written");
        }
    }
}

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

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

fn error_code(output: &Child) -> Value {
    json_line(&output.stderr)["error"]["code"].clone()
}

fn assert_spec_store_unusable(output: &Child) {
    assert_eq!(output.code, Some(1), "stdout: {}", output.stdout);
    assert_eq!(error_code(output), json!("spec-store-unusable"));
    assert_eq!(
        json_line(&output.stderr)["error"]["details"][0]["path"],
        json!("/work_item")
    );
}

fn porcelain(dir: &Path) -> String {
    let output = git_in(dir, &["status", "--porcelain"]);
    assert!(output.status.success(), "git status failed");

    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn read_part(fixture: &Fixture) -> String {
    fs::read_to_string(fixture.part_path(DEFAULT_SPECS)).expect("the part exists")
}

// Onde a spec nasce

#[test]
fn it_should_create_the_first_part_in_the_main_dir_when_run_from_a_linked_worktree() {
    let fixture = Fixture::new();

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(fixture.part_path(DEFAULT_SPECS).is_file());
}

#[test]
fn it_should_print_the_path_of_the_part_in_the_main_dir() {
    let fixture = Fixture::new();

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    let printed = json_line(&output.stdout)["path"].clone();
    let printed = printed.as_str().expect("path is a string");
    assert_eq!(
        fs::canonicalize(printed).expect("the printed path exists"),
        fixture.part_path(DEFAULT_SPECS)
    );
}

#[test]
fn it_should_not_create_any_spec_inside_the_linked_worktree() {
    let fixture = Fixture::new();

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(!fixture.linked.join(DEFAULT_SPECS).exists());
}

#[test]
fn it_should_write_a_gitignore_with_a_star_in_the_specs_dir() {
    let fixture = Fixture::new();

    fixture.init_from_linked(WORK_ITEM, SLUG);

    let gitignore = fs::read_to_string(fixture.main_dir().join(DEFAULT_SPECS).join(".gitignore"))
        .expect("the specs dir has a .gitignore");
    assert!(gitignore.lines().any(|line| line.trim() == "*"));
}

// Conteúdo da parte

#[test]
fn it_should_write_the_five_spec_sections_in_order() {
    let fixture = Fixture::new();

    fixture.init_from_linked(WORK_ITEM, SLUG);

    let part = read_part(&fixture);
    let headings: Vec<&str> = part.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(headings, SECTIONS);
}

#[test]
fn it_should_leave_every_section_empty() {
    let fixture = Fixture::new();

    fixture.init_from_linked(WORK_ITEM, SLUG);

    let part = read_part(&fixture);
    let body: Vec<&str> = part
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with("## "))
        .collect();
    assert_eq!(body, Vec::<&str>::new());
}

// Árvore limpa e load

#[test]
fn it_should_leave_the_main_dir_and_the_linked_worktree_clean_in_git() {
    let fixture = Fixture::new();

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(porcelain(fixture.project.dir()), "");
    assert_eq!(porcelain(&fixture.linked), "");
}

#[test]
fn it_should_not_report_unknown_entry_on_a_load_dry_run_after_init() {
    let fixture = Fixture::new();
    let init = fixture.init_from_linked(WORK_ITEM, SLUG);

    let load = fixture.run_in(fixture.project.dir(), &["load", "--dry-run"]);

    let warnings = json_line(&load.stdout)["warnings"].clone();
    assert_eq!(init.code, Some(0), "stderr: {}", init.stderr);
    assert_eq!(load.code, Some(0), "stderr: {}", load.stderr);
    assert_eq!(warnings, json!([]));
}

// Recusas

#[test]
fn it_should_refuse_a_second_init_of_the_same_work_item_with_spec_exists() {
    let fixture = Fixture::new();
    fixture.init_from_linked(WORK_ITEM, SLUG);

    let second = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(second.code, Some(1));
    assert!(second.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(&second), json!("spec-exists"));
}

#[test]
fn it_should_refuse_a_second_init_of_the_same_work_item_under_a_different_slug() {
    let fixture = Fixture::new();
    fixture.init_from_linked(WORK_ITEM, SLUG);

    let second = fixture.init_from_linked(WORK_ITEM, "signup");

    assert_eq!(error_code(&second), json!("spec-exists"));
}

#[test]
fn it_should_keep_the_existing_part_untouched_when_the_second_init_is_refused() {
    let fixture = Fixture::new();
    fixture.init_from_linked(WORK_ITEM, SLUG);
    fs::write(fixture.part_path(DEFAULT_SPECS), "written by the agent\n").expect("part edited");

    fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(read_part(&fixture), "written by the agent\n");
}

#[test]
fn it_should_refuse_a_work_item_that_does_not_exist() {
    let fixture = Fixture::new();

    let output = fixture.init_from_linked("work-items.ghost", SLUG);

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(&output), json!("work-item-not-found"));
}

#[test]
fn it_should_create_nothing_when_the_work_item_does_not_exist() {
    let fixture = Fixture::new();

    let output = fixture.init_from_linked("work-items.ghost", SLUG);

    assert_eq!(output.code, Some(1));
    assert!(!fixture.main_dir().join(DEFAULT_SPECS).exists());
}

#[test]
fn it_should_refuse_a_closed_work_item() {
    let fixture = Fixture::new();
    fixture.close_work_item();

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(1));
    assert!(output.stdout.is_empty(), "stdout stays empty");
    assert_eq!(error_code(&output), json!("work-item-closed"));
}

// Slug: vira nome de pasta, então é fronteira de confiança

const INVALID_SLUGS: [&str; 8] = [
    "",
    "Login",
    "a/b",
    "a\\b",
    "..",
    "my slug",
    "-login",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
];

#[test]
fn it_should_refuse_an_invalid_slug_with_spec_slug_invalid() {
    let fixture = Fixture::new();

    let outputs: Vec<Child> = INVALID_SLUGS
        .iter()
        .map(|slug| fixture.run_in(&fixture.linked, &["spec", "init", WORK_ITEM, "--", slug]))
        .collect();

    let refusals: Vec<(Option<i32>, bool, Value)> = outputs
        .iter()
        .map(|output| (output.code, output.stdout.is_empty(), error_code(output)))
        .collect();
    assert_eq!(
        refusals,
        vec![(Some(1), true, json!("spec-slug-invalid")); INVALID_SLUGS.len()]
    );
}

#[test]
fn it_should_create_nothing_when_the_slug_is_invalid() {
    let fixture = Fixture::new();

    let output = fixture.init_from_linked(WORK_ITEM, "Bad/../Slug");

    assert_eq!(output.code, Some(1));
    assert!(!fixture.main_dir().join(DEFAULT_SPECS).exists());
}

#[test]
fn it_should_not_echo_an_invalid_slug_in_the_error() {
    let fixture = Fixture::new();

    let output = fixture.init_from_linked(WORK_ITEM, "Canary/../Slug");

    assert_eq!(output.code, Some(1));
    assert!(!output.stderr.contains("Canary"));
}

#[test]
fn it_should_accept_a_slug_of_64_characters() {
    let fixture = Fixture::new();
    let slug = format!("a{}", "-".repeat(63));

    let output = fixture.init_from_linked(WORK_ITEM, &slug);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

// Teto próprio da busca por prefixo (não é o DIR_ENTRIES_MAX = 100 das extensions)

#[test]
fn it_should_init_when_150_specs_of_other_work_items_already_exist() {
    let fixture = Fixture::new();
    fixture.seed_other_work_items(150);

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(fixture.part_path(DEFAULT_SPECS).is_file());
}

#[test]
fn it_should_find_an_existing_spec_of_the_work_item_among_150_other_specs() {
    let fixture = Fixture::new();
    fixture.seed_other_work_items(150);
    fixture.seed_spec_dir("wi-w1__old");

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(1));
    assert_eq!(error_code(&output), json!("spec-exists"));
}

#[test]
fn it_should_init_when_the_specs_dir_has_exactly_10000_entries() {
    let fixture = Fixture::new();
    fixture.seed_entries(10_000);

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
}

#[test]
fn it_should_refuse_with_spec_store_unusable_when_the_specs_dir_has_10001_entries() {
    let fixture = Fixture::new();
    fixture.seed_entries(10_001);

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_spec_store_unusable(&output);
}

// Pasta de spec inutilizável

#[test]
fn it_should_refuse_with_spec_store_unusable_when_the_work_item_has_two_spec_folders() {
    let fixture = Fixture::new();
    fixture.seed_spec_dir("wi-w1__first");
    fixture.seed_spec_dir("wi-w1__second");

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_spec_store_unusable(&output);
}

#[test]
fn it_should_refuse_with_spec_store_unusable_when_the_specs_dir_cannot_be_read() {
    let fixture = Fixture::new();
    fixture.seed_entries(1);
    let specs = fixture.main_dir().join(DEFAULT_SPECS);
    let Some(_unreadable) = Unreadable::new(&specs) else {
        return;
    };

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_spec_store_unusable(&output);
}

// specs.dir customizado

#[test]
fn it_should_create_the_part_in_the_specs_dir_set_in_workflow_toml() {
    let fixture = Fixture::with_files(&[(WORKFLOW_PATH, CUSTOM_WORKFLOW)]);

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(fixture.part_path("docs/team-specs").is_file());
}

#[test]
fn it_should_not_create_the_default_specs_dir_when_workflow_toml_sets_another() {
    let fixture = Fixture::with_files(&[(WORKFLOW_PATH, CUSTOM_WORKFLOW)]);

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert!(!fixture.main_dir().join(DEFAULT_SPECS).exists());
}

#[test]
fn it_should_keep_the_tree_clean_when_the_specs_dir_is_custom() {
    let fixture = Fixture::with_files(&[(WORKFLOW_PATH, CUSTOM_WORKFLOW)]);

    let output = fixture.init_from_linked(WORK_ITEM, SLUG);

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(porcelain(fixture.project.dir()), "");
}
