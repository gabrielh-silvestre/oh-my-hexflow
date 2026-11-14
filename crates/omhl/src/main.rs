use std::process::ExitCode;

use omhl::adapters::cli;
use omhl::project::Env;

fn main() -> ExitCode {
    #[cfg(unix)]
    let parent_pid = std::os::unix::process::parent_id();
    // Windows (port para o Claude desktop): sem pid do pai; o Job Object do runner mata a árvore.
    #[cfg(not(unix))]
    let parent_pid = 0;
    let xdg_data_home = std::env::var_os("XDG_DATA_HOME");
    // No Windows não há HOME: o banco vai para %LOCALAPPDATA%\omhl.
    #[cfg(windows)]
    let xdg_data_home = xdg_data_home.or_else(|| std::env::var_os("LOCALAPPDATA"));
    let home = std::env::var_os("HOME");
    let git_index_file = std::env::var_os("GIT_INDEX_FILE");
    let env = Env {
        xdg_data_home: xdg_data_home.as_deref(),
        home: home.as_deref(),
        git_index_file: git_index_file.as_deref(),
        parent_pid,
        claude_code: std::env::var_os("CLAUDECODE").is_some_and(|value| value == "1"),
    };
    cli::run(
        std::env::args_os(),
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
        &env,
    )
}
