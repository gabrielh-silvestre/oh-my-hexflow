use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::ports::files::read_regular_utf8;
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
