use std::process::ExitCode;

use omhl::adapters::cli;
use omhl::project::Env;

fn main() -> ExitCode {
    let xdg_data_home = std::env::var_os("XDG_DATA_HOME");
    let home = std::env::var_os("HOME");
    let env = Env {
        xdg_data_home: xdg_data_home.as_deref(),
        home: home.as_deref(),
    };
    cli::run(
        std::env::args_os(),
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
        &env,
    )
}
