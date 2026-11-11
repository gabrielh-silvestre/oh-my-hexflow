use std::ffi::OsString;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand};
use serde_json::Value;

use super::{hook, mcp};
use crate::app::action::{Action, Caller, CheckTarget};
use crate::app::error::AppError;
use crate::app::input::read_json_input;
use crate::app::runtime::run_action;
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
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(flatten)]
    Action(ActionCommand),
    /// Serve the MCP tools over stdio.
    Mcp,
    /// Run a Claude Code hook.
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
}

#[derive(Debug, Subcommand)]
pub enum ActionCommand {
    /// List the current nodes.
    List {
        #[arg(long = "type")]
        type_name: Option<String>,
        #[arg(long)]
        id_prefix: Option<String>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        after_seq: Option<u64>,
    },
    /// Print one node, or null when it does not exist.
    Get { id: String },
    /// Append the records of a JSON request read from stdin.
    Append,
    /// Record a check outcome for a work item.
    Attest {
        check: String,
        work_item: String,
        #[arg(long)]
        outcome: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Print the session context as JSON.
    Render,
    /// Evaluate the pull-request gate for a work item.
    GatePr { work_item: String },
    /// Close a work item once the gate is evaluated.
    Close { work_item: String },
    /// Verify the hash chain.
    Verify,
    /// Load the project extensions into the tenant.
    Load {
        /// Report the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Write the projection of the tenant under `.omhl/projection/`.
    Project,
    /// Run mechanical checks.
    Check {
        #[command(subcommand)]
        command: CheckCommand,
    },
    /// Print the last lines of the stored output of a check for a work item.
    Byproduct {
        check: String,
        work_item: String,
        #[arg(long, default_value_t = 50)]
        tail: u32,
    },
    /// Manage the git hooks of omhl.
    Hooks {
        #[command(subcommand)]
        command: HooksCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum CheckCommand {
    /// Run the mechanical checks of an event, or one check, and attest them for the work item of the current branch.
    Run(CheckRunArgs),
    /// Kill the processes that escaped from an earlier check run whose omhl already exited.
    Sweep,
}

#[derive(Debug, Args)]
pub struct CheckRunArgs {
    #[arg(long)]
    pub event: Option<String>,
    #[arg(long)]
    pub check: Option<String>,
    #[arg(long)]
    pub pre_push: bool,
}

#[derive(Debug, Subcommand)]
pub enum HooksCommand {
    /// Install the git hooks (pre-commit, pre-merge-commit, pre-push, post-commit) of this clone; these are git hooks, unrelated to the Claude Code hook `omhl hook session-start`.
    Install {
        #[arg(long)]
        force: bool,
    },
    /// Consume the develop mark after a commit on develop; the git post-commit hook calls it.
    PostCommit,
}

#[derive(Debug, Subcommand)]
pub enum HookEvent {
    /// Inject the session context.
    SessionStart,
}

pub fn run(
    args: impl Iterator<Item = OsString>,
    stdin: &mut impl Read,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
    env: &Env<'_>,
) -> ExitCode {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(usage) => return report_usage(&usage, out, err),
    };
    let Cli {
        db,
        project_dir,
        command,
    } = cli;
    let db = db.as_deref();

    match command {
        Command::Hook {
            event: HookEvent::SessionStart,
        } => hook::session_start(stdin, out, err, env),
        Command::Mcp => match start_dir(project_dir.as_deref()) {
            Ok(dir) => mcp::serve(BufReader::new(stdin), out, &dir, db, env),
            Err(error) => report(Err(error), out, err),
        },
        Command::Action(command) => {
            let result = start_dir(project_dir.as_deref())
                .and_then(|dir| execute(command, stdin, &dir, db, env));
            report(result, out, err)
        }
    }
}

fn report_usage(
    usage: &clap::Error,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
) -> ExitCode {
    if matches!(
        usage.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        let _ = write!(out, "{}", usage.render());
        return ExitCode::SUCCESS;
    }
    let _ = write!(err, "{}", usage.render());

    ExitCode::from(2)
}

fn report(
    result: Result<Value, AppError>,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
) -> ExitCode {
    match result {
        Ok(value) => {
            let _ = writeln!(out, "{value}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let _ = writeln!(err, "{}", error.to_json());
            ExitCode::from(1)
        }
    }
}

fn start_dir(project_dir: Option<&Path>) -> Result<PathBuf, AppError> {
    let resolved = match project_dir {
        Some(path) => std::path::absolute(path),
        None => std::env::current_dir(),
    };

    resolved.map_err(|_| {
        AppError::plain(
            "invalid-project-dir",
            "the project directory cannot be resolved",
        )
    })
}

fn execute(
    command: ActionCommand,
    stdin: &mut impl Read,
    start: &Path,
    db: Option<&Path>,
    env: &Env<'_>,
) -> Result<Value, AppError> {
    let action = to_action(command, stdin, env)?;

    run_action(start, db, env, &Caller::cli(), &action)
}

fn to_action(
    command: ActionCommand,
    stdin: &mut impl Read,
    _env: &Env<'_>,
) -> Result<Action, AppError> {
    Ok(match command {
        ActionCommand::List {
            type_name,
            id_prefix,
            limit,
            after_seq,
        } => Action::List {
            type_name,
            id_prefix,
            limit,
            after_seq,
        },
        ActionCommand::Get { id } => Action::Get { id },
        ActionCommand::Append => Action::Append {
            request: read_json_input(stdin)?,
        },
        ActionCommand::Attest {
            check,
            work_item,
            outcome,
            note,
        } => Action::Attest {
            check,
            work_item,
            outcome,
            note,
        },
        ActionCommand::Render => Action::Render,
        ActionCommand::GatePr { work_item } => Action::GatePr { work_item },
        ActionCommand::Close { work_item } => Action::Close { work_item },
        ActionCommand::Verify => Action::Verify,
        ActionCommand::Load { dry_run } => Action::Load { dry_run },
        ActionCommand::Project => Action::Project,
        ActionCommand::Check {
            command: CheckCommand::Run(args),
        } => check_run(args),
        ActionCommand::Check {
            command: CheckCommand::Sweep,
        } => Action::CheckSweep,
        ActionCommand::Byproduct {
            check,
            work_item,
            tail,
        } => Action::Byproduct {
            check,
            work_item,
            tail,
        },
        ActionCommand::Hooks {
            command: HooksCommand::Install { force },
        } => Action::HooksInstall { force },
        ActionCommand::Hooks {
            command: HooksCommand::PostCommit,
        } => Action::ConsumeDevelopMark,
    })
}

fn check_run(args: CheckRunArgs) -> Action {
    let target = match (args.event, args.check) {
        (Some(event), _) => CheckTarget::Event(event),
        (None, Some(check)) => CheckTarget::Check(check),
        (None, None) => unreachable!("clap requires exactly one of --event and --check"),
    };
    let pre_push = match args.pre_push {
        true => unimplemented!("the pre-push input is read from stdin"),
        false => None,
    };

    Action::CheckRun { target, pre_push }
}
