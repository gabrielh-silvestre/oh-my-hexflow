use std::ffi::OsString;
use std::path::Path;

use ::omhl::adapters::cli;
use ::omhl::project::Env;

use super::project::Project;
use super::run::Run;

/// Roda o `omhl` pela CLI real no mesmo processo, em `project` (`--project-dir` e dados XDG dele).
pub fn run_inproc(project: &Project, args: &[&str], stdin: &str) -> Run {
    run_inproc_at(project.dir(), project.xdg(), args, stdin)
}

/// Como `run_inproc`, para quem monta a pasta do projeto à mão (sem marcador, inexistente).
pub fn run_inproc_at(project_dir: &Path, xdg: &Path, args: &[&str], stdin: &str) -> Run {
    let env = Env {
        xdg_data_home: Some(xdg.as_os_str()),
        home: None,
        git_index_file: None,
        parent_pid: 1,
        claude_code: false,
        role: None,
        plugin_root: None,
        session: None,
    };
    let argv = ["omhl", "--project-dir"]
        .into_iter()
        .map(OsString::from)
        .chain([project_dir.as_os_str().to_owned()])
        .chain(args.iter().map(OsString::from));
    let (mut out, mut err) = (Vec::new(), Vec::new());

    cli::run(argv, &mut stdin.as_bytes(), &mut out, &mut err, &env);

    Run {
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
    }
}
