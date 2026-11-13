use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::ports::files::read_regular_utf8;
use crate::ports::git::GitProbe;
use crate::probe::GitTopology;
use crate::project::ProjectError;
use crate::project::layout::{EXT_FILE_MAX_BYTES, OMHL_DIR, WORKFLOW_FILE};
use crate::workflow::config::{WorkflowConfig, parse_workflow};

/// Lê `.omhl/workflow.toml` da raiz do projeto. Arquivo ausente vale os padrões; presente e ilegível ou inválido é recusa.
pub fn read_workflow(root: &Path) -> Result<WorkflowConfig, ProjectError> {
    let path = root.join(OMHL_DIR).join(WORKFLOW_FILE);
    if let Err(error) = fs::symlink_metadata(&path)
        && error.kind() == ErrorKind::NotFound
    {
        return Ok(WorkflowConfig::default());
    }
    let text = read_regular_utf8(&path, EXT_FILE_MAX_BYTES).map_err(|reason| ProjectError {
        code: "invalid-workflow-config",
        message: format!("{}: {reason}", path.display()),
    })?;

    parse_workflow(&path, text.as_bytes())
}

/// De onde vale a política do `workflow.toml`: do diretório principal; da raiz do marcador quando a sonda do git falha (sem repositório).
pub fn policy_dir(root: &Path) -> PathBuf {
    GitProbe::new(root)
        .main_dir()
        .unwrap_or_else(|_| root.to_path_buf())
}

/// Lê a política do `workflow.toml` do diretório de `policy_dir`.
pub fn read_policy(root: &Path) -> Result<WorkflowConfig, ProjectError> {
    read_workflow(&policy_dir(root))
}
