use std::path::PathBuf;

use super::repo_checks::repo_root;

pub fn agent_file(role: &str) -> PathBuf {
    repo_root().join(format!("plugin/agents/{role}.md"))
}
