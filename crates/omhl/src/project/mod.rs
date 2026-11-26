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
    pub plugin_root: Option<&'e std::ffi::OsStr>,
    pub session: Option<std::ffi::OsString>,
    pub claude: ClaudeEnv,
}

/// O ambiente do processo é lido uma vez, na borda (`from_process`); os hooks nunca leem as variáveis.
#[derive(Default)]
pub struct ClaudeEnv {
    pub plugin_option_language: Option<std::ffi::OsString>,
    pub project_dir: Option<std::ffi::OsString>,
    pub config_dir: Option<std::ffi::OsString>,
}

impl ClaudeEnv {
    pub fn from_process() -> ClaudeEnv {
        ClaudeEnv {
            plugin_option_language: std::env::var_os("CLAUDE_PLUGIN_OPTION_LANGUAGE"),
            project_dir: std::env::var_os("CLAUDE_PROJECT_DIR"),
            config_dir: std::env::var_os("CLAUDE_CONFIG_DIR"),
        }
    }
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

#[cfg(test)]
mod tests;
