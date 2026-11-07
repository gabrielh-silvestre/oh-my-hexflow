use std::fs;

use serde_json::Value;

use super::compile;

const SUITE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/json-schema-test-suite"
);

fn suite_files() -> Vec<(String, Vec<Value>)> {
    let mut files: Vec<(String, Vec<Value>)> = fs::read_dir(SUITE_DIR)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let groups: Vec<Value> =
                serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            (name, groups)
        })
        .collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

#[test]
fn it_should_pass_the_filtered_official_suite_without_skips() {
    let files = suite_files();
    let group_count: usize = files.iter().map(|(_, groups)| groups.len()).sum();
    let case_count: usize = files
        .iter()
        .flat_map(|(_, groups)| groups)
        .map(|group| group["tests"].as_array().unwrap().len())
        .sum();
    assert_eq!((files.len(), group_count, case_count), (21, 124, 392));

    let mut failures = Vec::new();
    for (file, groups) in &files {
        for group in groups {
            let description = group["description"].as_str().unwrap();
            let compiled = match compile(&group["schema"]) {
                Ok(compiled) => compiled,
                Err(details) => {
                    failures.push(format!("{file} / {description}: refused {details:?}"));
                    continue;
                }
            };
            for case in group["tests"].as_array().unwrap() {
                let expected = case["valid"].as_bool().unwrap();
                let actual = compiled.validate(&case["data"]).is_ok();
                if actual != expected {
                    failures.push(format!(
                        "{file} / {description} / {}: expected valid={expected}",
                        case["description"].as_str().unwrap()
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
