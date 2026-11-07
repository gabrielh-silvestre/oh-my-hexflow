use std::fs::{DirBuilder, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{Env, ProjectError};

pub fn default_db_path(env: &Env<'_>) -> Result<PathBuf, ProjectError> {
    let xdg_base = env
        .xdg_data_home
        .map(Path::new)
        .filter(|base| base.is_absolute());
    let base = match xdg_base {
        Some(base) => base.to_path_buf(),
        None => home_data_dir(env)?,
    };

    Ok(base.join("omhl").join("omhl.db"))
}

fn home_data_dir(env: &Env<'_>) -> Result<PathBuf, ProjectError> {
    match env.home.filter(|home| !home.is_empty()) {
        Some(home) => Ok(Path::new(home).join(".local").join("share")),
        None => Err(ProjectError {
            code: "no-database-path",
            message: "cannot locate the database: set HOME or XDG_DATA_HOME, or pass --db".into(),
        }),
    }
}

fn file_error(path: &Path, error: &std::io::Error) -> ProjectError {
    ProjectError {
        code: "database-file",
        message: format!("{}: {error}", path.display()),
    }
}

/// Cria o diretório (`0700`) e o arquivo vazio (`0600`) só quando faltam; o `hexlog` instala o esquema.
/// Fora do unix o modo não é aplicado: o arquivo nasce com o padrão do sistema.
pub fn prepare_db_file(path: &Path) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent).map_err(|error| file_error(parent, &error))?;
    }
    match create_private_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(file_error(path, &error)),
    }
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);

    builder.create(dir)
}

fn create_private_file(path: &Path) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);

    options.open(path).map(drop)
}
