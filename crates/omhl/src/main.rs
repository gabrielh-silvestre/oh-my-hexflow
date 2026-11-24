use std::process::ExitCode;

use omhl::adapters::cli;
use omhl::guard::ROLE_VARIABLE;
use omhl::project::Env;

fn main() -> ExitCode {
    let parent_pid = std::os::unix::process::parent_id();
    let xdg_data_home = std::env::var_os("XDG_DATA_HOME");
    let home = std::env::var_os("HOME");
    let git_index_file = std::env::var_os("GIT_INDEX_FILE");
    let plugin_root = std::env::var_os("CLAUDE_PLUGIN_ROOT");
    let env = Env {
        xdg_data_home: xdg_data_home.as_deref(),
        home: home.as_deref(),
        git_index_file: git_index_file.as_deref(),
        parent_pid,
        claude_code: std::env::var_os("CLAUDECODE").is_some_and(|value| value == "1"),
        role: std::env::var_os(ROLE_VARIABLE).map(|value| value.to_string_lossy().into_owned()),
        plugin_root: plugin_root.as_deref(),
        session: std::env::var_os("CLAUDE_CODE_SESSION_ID"),
    };
    cli::run(
        std::env::args_os(),
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
        &env,
    )
}
