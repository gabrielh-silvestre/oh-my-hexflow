use crate::support;

use std::fs;

use serde_json::{Value, json};
use support::child::{Cmd, omhl};
use support::ext::ExtTree;
use support::guideline_retire::{BRIEF, TONE};
use support::project::Project;

// Linha de comando

const TONE_TOML: &str = "kind = \"business\"\ntext = \"Keep every answer brief\"\n";
const BRIEF_TOML: &str = "kind = \"business\"\ntext = \"Be concise\"\n";

fn omhl_in(project: &Project, args: &[&str]) -> Cmd {
    omhl(args).cwd(project.dir()).xdg(project.xdg())
}

/// Projeto com as diretivas `tone` e `brief` já carregadas; os arquivos seguem na árvore.
fn loaded_project() -> Project {
    let project = Project::new("acme");
    ExtTree::at(project.dir())
        .guideline("tone", TONE_TOML)
        .guideline("brief", BRIEF_TOML);
    let loaded = omhl_in(&project, &["load"]).run();
    assert_eq!(loaded.code, Some(0), "load stderr: {}", loaded.stderr);

    project
}

fn json_line(text: &str) -> Value {
    serde_json::from_str(text.trim_end()).expect("the line is JSON")
}

#[test]
fn it_should_retire_a_guideline_with_its_successor_from_the_command_line() {
    let project = loaded_project();
    fs::remove_file(project.dir().join(".omhl/guidelines/tone.toml"))
        .expect("the guideline file is removed");

    let output = omhl_in(&project, &["guideline", "retire", TONE, "--by", BRIEF]).run();

    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);
    assert_eq!(
        json_line(&output.stdout),
        json!({ "retired": TONE, "superseded_by": BRIEF })
    );
}

#[test]
fn it_should_refuse_from_the_command_line_while_the_toml_file_exists() {
    let project = loaded_project();

    let output = omhl_in(&project, &["guideline", "retire", TONE]).run();

    assert_eq!(output.code, Some(1), "stderr: {}", output.stderr);
    let error = json_line(&output.stderr)["error"].clone();
    assert_eq!(error["code"], json!("guideline-not-orphaned"));
    assert_eq!(error["details"][0]["path"], json!("/id"));
}
