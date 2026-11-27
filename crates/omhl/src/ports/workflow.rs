use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::ports::files::read_regular_utf8;
use crate::ports::git::GitProbe;
use crate::probe::{NativeRefusal, NativeResolver};
use crate::project::ProjectError;
use crate::project::layout::{EXT_FILE_MAX_BYTES, OMHL_DIR, WORKFLOW_FILE};
use crate::workflow::config::{
    INVALID_WORKFLOW_CONFIG, NativeConfig, WorkflowConfig, parse_workflow,
};

pub fn read_workflow(root: &Path) -> Result<WorkflowConfig, ProjectError> {
    let path = root.join(OMHL_DIR).join(WORKFLOW_FILE);
    if let Err(error) = fs::symlink_metadata(&path)
        && error.kind() == ErrorKind::NotFound
    {
        return Ok(WorkflowConfig::default());
    }
    let text = read_regular_utf8(&path, EXT_FILE_MAX_BYTES).map_err(|reason| ProjectError {
        code: INVALID_WORKFLOW_CONFIG,
        message: format!("{}: {reason}", path.display()),
    })?;

    parse_workflow(&path, text.as_bytes())
}

pub fn policy_dir(root: &Path) -> PathBuf {
    GitProbe::new(root)
        .main_dir()
        .unwrap_or_else(|_| root.to_path_buf())
}

pub fn read_policy(root: &Path) -> Result<WorkflowConfig, ProjectError> {
    read_workflow(&policy_dir(root))
}

impl NativeResolver for GitProbe {
    fn resolve(&self) -> Result<NativeConfig, NativeRefusal> {
        let main_dir = self.main_dir().map_err(|_| NativeRefusal::MainDirUnknown)?;

        read_workflow(&main_dir)
            .map(|config| NativeConfig {
                checks: config.native.checks,
                prefixes: config.prefixes,
            })
            .map_err(NativeRefusal::InvalidWorkflow)
    }
}

#[cfg(test)]
mod tests;
