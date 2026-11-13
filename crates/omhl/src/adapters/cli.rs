use std::ffi::OsString;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::builder::PossibleValuesParser;
use clap::error::ErrorKind;
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};

use super::{hook, mcp};
use crate::app::RUNNABLE_EVENTS;
use crate::app::action::{Action, Caller, CheckTarget};
use crate::app::error::AppError;
use crate::app::input::{INPUT_MAX_BYTES, read_json_input};
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
        /// Keep the nodes whose `field` equals `value`; repeatable.
        #[arg(long = "where", value_name = "FIELD=VALUE")]
        where_eq: Vec<String>,
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
    /// Decide whether a worktree may be created for a branch; silent without a project marker.
    GateWorktree {
        #[arg(long, allow_hyphen_values = true)]
        branch: String,
    },
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
    /// Manage the specs of work items.
    Spec {
        #[command(subcommand)]
        command: SpecCommand,
    },
    /// Report on the gaps and their answers.
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum CheckCommand {
    /// Run the mechanical checks of an event, or one check, and attest them for the work item of the current branch.
    Run(CheckRunArgs),
    /// Kill the processes that escaped from an earlier check run whose omhl already exited.
    Sweep,
    /// Retire the stored check whose file was deleted from the working tree.
    Retire { check: String },
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("target").required(true).multiple(false).args(["event", "check"])))]
pub struct CheckRunArgs {
    #[arg(long, value_parser = PossibleValuesParser::new(RUNNABLE_EVENTS))]
    pub event: Option<String>,
    #[arg(long)]
    pub check: Option<String>,
    #[arg(long, requires = "event")]
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
pub enum SpecCommand {
    /// Create the spec folder of an open work item, with its first part, in the main directory.
    Init {
        /// Id of the work item, such as work-items.w1.
        work_item: String,
        /// Folder name after the work item: 1 to 64 characters of a-z, 0-9 and -, not starting with -.
        slug: String,
    },
    /// Check the structure of the spec folder of a work item (the graph is not consulted).
    Validate {
        /// Id of the work item, such as work-items.w1.
        work_item: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ReportCommand {
    /// Report the state of each gap, who answered it and whether a directive backs the answer.
    Answers {
        /// Id of the work item, such as work-items.w1; without it, every gap is reported.
        #[arg(long)]
        work_item: Option<String>,
    },
    /// List the premises repeated in enough work items to suggest promoting them to a guideline.
    Recurrences {
        /// Minimum number of distinct work items; without it, `promotion.min_work_items` of workflow.toml applies.
        #[arg(long)]
        min: Option<u32>,
    },
}

#[derive(Debug, Subcommand)]
pub enum HookEvent {
    /// Inject the session context.
    SessionStart,
    /// Deny edits to the spec of a work item that already has a branch; reads the PreToolUse event on stdin.
    SpecLock,
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
    if let Some(usage) = pre_push_misuse(&cli) {
        return report_usage(&usage, out, err);
    }
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
        Command::Hook {
            event: HookEvent::SpecLock,
        } => hook::spec_lock(stdin, out, err),
        Command::Mcp => match start_dir(project_dir.as_deref()) {
            Ok(dir) => mcp::serve(BufReader::new(stdin), out, &dir, db, env),
            Err(error) => report(Err(error), out, err),
        },
        Command::Action(command) => {
            // O `pre-switch` do `wt` roda em qualquer repositório: sem marcador o gate não tem o que decidir.
            let silent_without_marker = matches!(command, ActionCommand::GateWorktree { .. });
            let result = start_dir(project_dir.as_deref())
                .and_then(|dir| execute(command, stdin, &dir, db, env));
            match &result {
                Err(error) if silent_without_marker && error.code() == "no-marker" => {
                    ExitCode::SUCCESS
                }
                _ => report(result, out, err),
            }
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

/// O clap não exprime "só com este valor de `--event`"; o erro sai pelo mesmo caminho do de uso.
fn pre_push_misuse(cli: &Cli) -> Option<clap::Error> {
    let Command::Action(ActionCommand::Check {
        command: CheckCommand::Run(args),
    }) = &cli.command
    else {
        return None;
    };

    (args.pre_push && args.event.as_deref() != Some("before-pr")).then(|| {
        Cli::command().error(
            ErrorKind::ArgumentConflict,
            "--pre-push is only valid with --event before-pr",
        )
    })
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

    resolved.map_err(|_| AppError::invalid_project_dir())
}

fn execute(
    command: ActionCommand,
    stdin: &mut impl Read,
    start: &Path,
    db: Option<&Path>,
    env: &Env<'_>,
) -> Result<Value, AppError> {
    let action = to_action(command, stdin, env)?;

    let caller = if env.claude_code {
        Caller::cli_under_claude_code()
    } else {
        Caller::cli()
    };

    run_action(start, db, env, &caller, &action)
}

/// Divide no primeiro `=`: o valor pode conter `=`.
fn split_where(index: usize, pair: &str) -> Result<(String, String), AppError> {
    match pair.split_once('=') {
        Some((field, value)) if !field.is_empty() => Ok((field.to_owned(), value.to_owned())),
        _ => Err(AppError::invalid_arguments(
            &format!("/where/{index}"),
            "unknown value",
        )),
    }
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
            where_eq,
        } => Action::List {
            type_name,
            id_prefix,
            limit,
            after_seq,
            where_eq: where_eq
                .iter()
                .enumerate()
                .map(|(index, pair)| split_where(index, pair))
                .collect::<Result<_, AppError>>()?,
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
        ActionCommand::GateWorktree { branch } => Action::GateWorktree { branch },
        ActionCommand::Close { work_item } => Action::Close { work_item },
        ActionCommand::Verify => Action::Verify,
        ActionCommand::Load { dry_run } => Action::Load { dry_run },
        ActionCommand::Project => Action::Project,
        ActionCommand::Check {
            command: CheckCommand::Run(args),
        } => check_run(args, stdin)?,
        ActionCommand::Check {
            command: CheckCommand::Sweep,
        } => Action::CheckSweep,
        ActionCommand::Check {
            command: CheckCommand::Retire { check },
        } => Action::CheckRetire { check },
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
        ActionCommand::Spec {
            command: SpecCommand::Init { work_item, slug },
        } => Action::SpecInit { work_item, slug },
        ActionCommand::Spec {
            command: SpecCommand::Validate { work_item },
        } => Action::SpecValidate { work_item },
        ActionCommand::Report {
            command: ReportCommand::Answers { work_item },
        } => Action::ReportAnswers { work_item },
        ActionCommand::Report {
            command: ReportCommand::Recurrences { min },
        } => Action::ReportRecurrences { min },
    })
}

fn check_run(args: CheckRunArgs, stdin: &mut impl Read) -> Result<Action, AppError> {
    let target = match (args.event, args.check) {
        (Some(event), _) => CheckTarget::Event(event),
        (None, Some(check)) => CheckTarget::Check(check),
        (None, None) => unreachable!("clap requires exactly one of --event and --check"),
    };
    let pre_push = match args.pre_push {
        true => Some(read_pre_push_input(stdin)?),
        false => None,
    };

    Ok(Action::CheckRun { target, pre_push })
}

/// Lê um byte além do limite: a entrada grande demais é recusada sem ler o resto.
fn read_pre_push_input(stdin: &mut impl Read) -> Result<String, AppError> {
    let mut bytes = Vec::new();
    stdin
        .take(INPUT_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::input_unreadable())?;
    if bytes.len() > INPUT_MAX_BYTES {
        return Err(push_input_refused("input-too-large"));
    }

    String::from_utf8(bytes).map_err(|_| push_input_refused("input-not-utf8"))
}

fn push_input_refused(kind: &str) -> AppError {
    AppError::push_input_invalid("").with_context(json!({ "kind": kind, "event": "before-pr" }))
}
