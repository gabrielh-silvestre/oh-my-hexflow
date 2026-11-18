use std::ffi::OsString;

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
    let env = Env {
        xdg_data_home: Some(project.xdg().as_os_str()),
        home: None,
        git_index_file: None,
        parent_pid: 1,
        claude_code,
        role: role.map(str::to_owned),
        plugin_root: None,
    };
    let argv = ["omhl", "--project-dir"]
        .into_iter()
        .map(OsString::from)
        .chain([project.dir().as_os_str().to_owned()])
        .chain(args.iter().map(OsString::from));
    let (mut out, mut err) = (Vec::new(), Vec::new());

    cli::run(argv, &mut stdin.as_bytes(), &mut out, &mut err, &env);

    Run {
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
    }
}
