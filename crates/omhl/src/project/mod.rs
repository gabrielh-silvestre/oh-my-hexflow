pub mod layout;
pub mod marker;
pub mod paths;
pub(crate) mod text;

/// Variáveis de ambiente que o `omhl` lê, por valor: ninguém toca o ambiente do processo.
pub struct Env<'e> {
    pub xdg_data_home: Option<&'e std::ffi::OsStr>,
    pub home: Option<&'e std::ffi::OsStr>,
    pub git_index_file: Option<&'e std::ffi::OsStr>,
    pub parent_pid: u32,
    pub claude_code: bool,
    /// Valor cru do `OMHL_ROLE`: vazio é um valor (recusado), não ausência.
    pub role: Option<String>,
    /// Raiz do plugin (`CLAUDE_PLUGIN_ROOT`); ausente vale `None`.
    pub plugin_root: Option<&'e std::ffi::OsStr>,
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
