pub mod layout;
pub mod marker;
pub mod paths;
pub(crate) mod text;

/// Variáveis de ambiente que o `omhl` lê, por valor: ninguém toca o ambiente do processo.
pub struct Env<'e> {
    pub xdg_data_home: Option<&'e std::ffi::OsStr>,
    pub home: Option<&'e std::ffi::OsStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProjectError {}
