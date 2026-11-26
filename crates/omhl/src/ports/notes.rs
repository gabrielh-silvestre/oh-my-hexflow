use std::fs;
use std::io::{ErrorKind, Result};
use std::path::Path;

use super::specs::write_new;
use crate::project::layout::{GITIGNORE_NAME, NOTES_DIR, NOTES_GITIGNORE_CONTENT, OMHL_DIR};

/// Quem cria `.omhl/notes` é quem grava o `.gitignore`: sem um "existe?" antes, um evento concorrente não o recria.
pub fn birth_notes_folder(root: &Path) -> Result<()> {
    let omhl_dir = root.join(OMHL_DIR);
    fs::create_dir_all(&omhl_dir)?;
    let notes_dir = omhl_dir.join(NOTES_DIR);

    match fs::create_dir(&notes_dir) {
        Ok(()) => write_new(&notes_dir.join(GITIGNORE_NAME), NOTES_GITIGNORE_CONTENT),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests;
