use crate::support;

use std::fs;

use support::repo_checks::repo_root;

// Documentação

#[test]
fn it_should_document_extensions_in_toml_in_the_glossary() {
    let glossary = fs::read_to_string(repo_root().join("GLOSSARY.md")).expect("GLOSSARY.md exists");
    let guide = repo_root().join("docs/extensions-and-projection.md");

    let entry = glossary
        .split("\n\n")
        .find(|block| block.starts_with("**Extension**"))
        .expect("the glossary has an Extension entry");

    assert!(guide.is_file(), "docs/extensions-and-projection.md exists");
    assert!(entry.contains("TOML"), "the entry cites TOML");
    assert!(!entry.contains("YAML"), "the entry does not cite YAML");
}
