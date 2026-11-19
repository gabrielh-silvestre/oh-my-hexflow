use super::inproc::run_inproc;
use super::project::Project;
use super::report::WORK_ITEM;
use serde_json::Value;

/// O stdout do comando em processo; o stderr tem de estar vazio.
pub fn stdout_of(project: &Project, args: &[&str]) -> String {
    let run = run_inproc(project, args, "");

    assert_eq!(run.stderr, "", "unexpected stderr for {args:?}");

    run.stdout
}

pub const ANSWERS: &[&str] = &["report", "answers"];

pub const RECURRENCES: &[&str] = &["report", "recurrences"];

pub fn retro_args() -> Vec<&'static str> {
    vec!["report", "retro", WORK_ITEM]
}

pub fn with_format<'a>(format: Option<&'a str>, args: &[&'a str]) -> Vec<&'a str> {
    let flag = format.into_iter().flat_map(|format| ["--format", format]);

    flag.chain(args.iter().copied()).collect()
}

// ---------------------------------------------------------------- `render`: os bytes do `text`

/// O `text` do contexto no `--format json`.
pub fn context_text(project: &Project) -> String {
    let json = stdout_of(project, &["--format", "json", "render"]);
    let value: Value = serde_json::from_str(json.trim_end()).expect("the output is JSON");

    value["text"]
        .as_str()
        .expect("the value has the text")
        .to_owned()
}
