use std::ffi::OsString;
use std::process::ExitCode;

use ::omhl::adapters::cli;
use ::omhl::project::Env;

use super::project::Project;

/// Saída de um `omhl` rodado no mesmo processo.
pub struct Run {
    pub stdout: String,
    pub stderr: String,
}

/// Roda o `omhl` pela CLI real no mesmo processo, com o ambiente dado por valor.
pub fn run_omhl(
    project: &Project,
    args: &[&str],
    stdin: &str,
    claude_code: bool,
    role: Option<&str>,
) -> Run {
    run_omhl_with_session(project, args, stdin, claude_code, role, None).1
}

/// Como `run_omhl`, com o valor cru de `CLAUDE_CODE_SESSION_ID` (`None` é a variável ausente) e o código de saída.
pub fn run_omhl_with_session(
    project: &Project,
    args: &[&str],
    stdin: &str,
    claude_code: bool,
    role: Option<&str>,
    session: Option<OsString>,
) -> (ExitCode, Run) {
    let env = Env {
        xdg_data_home: Some(project.xdg().as_os_str()),
        home: None,
        git_index_file: None,
        parent_pid: 1,
        claude_code,
        role: role.map(str::to_owned),
        plugin_root: None,
        claude: Default::default(),
        session,
    };
    let argv = ["omhl", "--project-dir"]
        .into_iter()
        .map(OsString::from)
        .chain([project.dir().as_os_str().to_owned()])
        .chain(args.iter().map(OsString::from));
    let (mut out, mut err) = (Vec::new(), Vec::new());

    let exit = cli::run(argv, &mut stdin.as_bytes(), &mut out, &mut err, &env);

    let run = Run {
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
    };

    (exit, run)
}
