use std::process::ExitCode;

use crate::project::Env;

// provisional: replaced in T9
pub fn session_start(
    _stdin: &mut impl std::io::Read,
    _out: &mut impl std::io::Write,
    _err: &mut impl std::io::Write,
    _env: &Env<'_>,
) -> ExitCode {
    ExitCode::SUCCESS
}
