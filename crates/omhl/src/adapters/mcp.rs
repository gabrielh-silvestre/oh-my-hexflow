use std::path::Path;
use std::process::ExitCode;

use crate::project::Env;

// provisional: replaced in T8
pub fn serve(
    input: impl std::io::BufRead,
    _out: &mut impl std::io::Write,
    _start: &Path,
    _db: Option<&Path>,
    _env: &Env<'_>,
) -> ExitCode {
    let _ = input.lines().count();
    ExitCode::SUCCESS
}
