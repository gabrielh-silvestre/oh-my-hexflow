use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind;

use crate::project::Env;

#[derive(Debug, Parser)]
#[command(name = "omhl", version, arg_required_else_help = true)]
pub struct Cli {
    /// Path of the database file.
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,
    /// Directory where the project marker lookup starts.
    #[arg(long, global = true)]
    pub project_dir: Option<PathBuf>,
}

pub fn run(
    args: impl Iterator<Item = OsString>,
    _stdin: &mut impl std::io::Read,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
    _env: &Env<'_>,
) -> ExitCode {
    match Cli::try_parse_from(args) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            let _ = write!(out, "{}", e.render());
            ExitCode::SUCCESS
        }
        Err(e) => {
            let _ = write!(err, "{}", e.render());
            ExitCode::from(2)
        }
    }
}
