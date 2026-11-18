use super::git::GitRepo;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

pub const W1: &str = "work-items.w1";

pub const BRANCH: &str = "feat/wi-w1__x";

pub fn tree_json(id: &str) -> Value {
    json!({ "kind": "git-tree", "id": id })
}

pub fn node_wire(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "items": [{ "body": { "kind": "node", "id": id, "type": type_name, "data": data } }] })
}

pub fn copy_index(repo: &GitRepo, to: &Path) {
    fs::copy(repo.path().join(".git").join("index"), to).expect("the index is copied");
}
