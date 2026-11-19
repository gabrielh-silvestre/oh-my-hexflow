use std::path::{Path, PathBuf};

pub fn agent_file(role: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    root.join(format!("plugin/agents/{role}.md"))
}
