use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::builder::PossibleValuesParser;
use clap::error::ErrorKind;
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};

use super::hook;
use super::text::{CommandKey, Layout, deferred, render_failure, render_success, renderer_for};
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
    /// Output format: `agent` (text for agents), `human` (text for people) or `json` (one JSON line). The default is `agent` for the commands that print text and `json` for the others; the hook commands, `hooks post-commit` and `gate-worktree` only print JSON.
    #[arg(long, global = true, value_enum)]
    pub format: Option<OutputFormat>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(flatten)]
    Action(ActionCommand),
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
    /// Manage the guidelines of the repository.
    Guideline {
        #[command(subcommand)]
        command: GuidelineCommand,
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
    /// Read-only reports: gap answers, premise recurrences and the retro of a work item.
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

#[derive(Debug, Subcommand)]
pub enum GuidelineCommand {
    /// Retire the stored guideline whose file was deleted from the working tree.
    Retire {
        id: String,
        /// The guideline that supersedes the retired one.
        #[arg(long)]
        by: Option<String>,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Agent,
    Human,
    Json,
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
    /// Check the structure and the graph of the spec folder of a work item.
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
    /// Print the retro of a work item (open or closed): divergences, amendments and unanswered gaps.
    Retro {
        /// Id of the work item, such as work-items.w1.
        work_item: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum HookEvent {
    /// Inject the session context.
    SessionStart,
    /// Deny edits to the spec of a work item that already has a branch; reads the PreToolUse event on stdin.
    SpecLock,
    /// Impose the role matrix on the Bash tool of plugin agents; reads the PreToolUse event on stdin.
    AgentRole,
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
    let key = command_key(&cli.command);
    if let Some(usage) = pre_push_misuse(&cli).or_else(|| format_misuse(&cli, key)) {
        return report_usage(&usage, out, err);
    }
    let Cli {
        db,
        project_dir,
        format,
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
        Command::Hook {
            event: HookEvent::AgentRole,
        } => {
            let cli = Cli::command();
            // Os comandos com subcomando vêm do clap: o hook não mantém uma segunda lista de folhas.
            let groups: Vec<&str> = cli
                .get_subcommands()
                .filter(|command| command.has_subcommands())
                .map(clap::Command::get_name)
                .collect();
            hook::agent_role(stdin, out, err, env, &groups)
        }
        Command::Action(command) => {
            // O `pre-switch` do `wt` roda em qualquer repositório: sem marcador o gate não tem o que decidir.
            let silent_without_marker = matches!(key, CommandKey::GateWorktree);
            let format = format.unwrap_or_else(|| default_format(key));
            let result = start_dir(project_dir.as_deref())
                .and_then(|dir| execute(command, stdin, &dir, db, env));
            match &result {
                Err(error) if silent_without_marker && error.code() == "no-marker" => {
                    ExitCode::SUCCESS
                }
                _ => report(result, format, key, out, err),
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

/// `agent` e `human` só existem onde o comando tem renderizador; a recusa sai pelo mesmo caminho do erro de uso.
fn format_misuse(cli: &Cli, key: CommandKey) -> Option<clap::Error> {
    if !matches!(cli.format, Some(OutputFormat::Agent | OutputFormat::Human))
        || renderer_for(key).is_some()
    {
        return None;
    }
    let message = match deferred(key) {
        true => "this command only prints JSON; use --format json",
        false => "not available for this command yet; use --format json",
    };

    Some(Cli::command().error(ErrorKind::InvalidValue, message))
}

fn default_format(key: CommandKey) -> OutputFormat {
    match renderer_for(key).is_some() {
        true => OutputFormat::Agent,
        false => OutputFormat::Json,
    }
}

/// Sem braço `_`: comando novo falha na compilação até ganhar uma chave.
fn command_key(command: &Command) -> CommandKey {
    match command {
        Command::Hook {
            event: HookEvent::SessionStart,
        } => CommandKey::HookSessionStart,
        Command::Hook {
            event: HookEvent::SpecLock,
        } => CommandKey::HookSpecLock,
        Command::Hook {
            event: HookEvent::AgentRole,
        } => CommandKey::HookAgentRole,
        Command::Action(ActionCommand::List { .. }) => CommandKey::List,
        Command::Action(ActionCommand::Get { .. }) => CommandKey::Get,
        Command::Action(ActionCommand::Append) => CommandKey::Append,
        Command::Action(ActionCommand::Attest { .. }) => CommandKey::Attest,
        Command::Action(ActionCommand::Render) => CommandKey::Render,
        Command::Action(ActionCommand::GatePr { .. }) => CommandKey::GatePr,
        Command::Action(ActionCommand::GateWorktree { .. }) => CommandKey::GateWorktree,
        Command::Action(ActionCommand::Close { .. }) => CommandKey::Close,
        Command::Action(ActionCommand::Verify) => CommandKey::Verify,
        Command::Action(ActionCommand::Load { .. }) => CommandKey::Load,
        Command::Action(ActionCommand::Project) => CommandKey::Project,
        Command::Action(ActionCommand::Check {
            command: CheckCommand::Run(_),
        }) => CommandKey::CheckRun,
        Command::Action(ActionCommand::Check {
            command: CheckCommand::Sweep,
        }) => CommandKey::CheckSweep,
        Command::Action(ActionCommand::Check {
            command: CheckCommand::Retire { .. },
        }) => CommandKey::CheckRetire,
        Command::Action(ActionCommand::Guideline {
            command: GuidelineCommand::Retire { .. },
        }) => CommandKey::GuidelineRetire,
        Command::Action(ActionCommand::Byproduct { .. }) => CommandKey::Byproduct,
        Command::Action(ActionCommand::Hooks {
            command: HooksCommand::Install { .. },
        }) => CommandKey::HooksInstall,
        Command::Action(ActionCommand::Hooks {
            command: HooksCommand::PostCommit,
        }) => CommandKey::HooksPostCommit,
        Command::Action(ActionCommand::Spec {
            command: SpecCommand::Init { .. },
        }) => CommandKey::SpecInit,
        Command::Action(ActionCommand::Spec {
            command: SpecCommand::Validate { .. },
        }) => CommandKey::SpecValidate,
        Command::Action(ActionCommand::Report {
            command: ReportCommand::Answers { .. },
        }) => CommandKey::ReportAnswers,
        Command::Action(ActionCommand::Report {
            command: ReportCommand::Recurrences { .. },
        }) => CommandKey::ReportRecurrences,
        Command::Action(ActionCommand::Report {
            command: ReportCommand::Retro { .. },
        }) => CommandKey::ReportRetro,
    }
}

fn report(
    result: Result<Value, AppError>,
    format: OutputFormat,
    key: CommandKey,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
) -> ExitCode {
    match (result, format) {
        (Ok(value), OutputFormat::Json) => {
            let _ = writeln!(out, "{value}");
            ExitCode::SUCCESS
        }
        (Ok(value), OutputFormat::Agent | OutputFormat::Human) => {
            let _ = writeln!(out, "{}", render_success(key, &value, layout_of(format)));
            ExitCode::SUCCESS
        }
        (Err(error), OutputFormat::Json) => {
            let _ = writeln!(err, "{}", error.to_json());
            ExitCode::from(1)
        }
        (Err(error), OutputFormat::Agent | OutputFormat::Human) => {
            let _ = writeln!(
                err,
                "{}",
                render_failure(key, &error.to_json(), layout_of(format))
            );
            ExitCode::from(1)
        }
    }
}

/// O `Json` não passa por aqui: o `report` só chama isto nos braços `Agent` e `Human`.
fn layout_of(format: OutputFormat) -> Layout {
    match format {
        OutputFormat::Human => Layout::Human,
        OutputFormat::Agent | OutputFormat::Json => Layout::Agent,
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

    let caller = Caller::cli_with_role(env.claude_code, env.role.as_deref())?;

    run_action(start, db, env, &caller, &action)
}

/// Divide no primeiro `=`: o valor pode conter `=`.
fn split_where(index: usize, pair: &str) -> Result<(String, String), AppError> {
    match pair.split_once('=') {
        Some((field, value)) if !field.is_empty() => Ok((field.to_owned(), value.to_owned())),
        _ => Err(AppError::invalid_arguments(
            &format!("/where/{index}"),
            "where must be FIELD=VALUE with a non-empty FIELD",
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
        ActionCommand::Guideline {
            command: GuidelineCommand::Retire { id, by },
        } => Action::GuidelineRetire { id, by },
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
        ActionCommand::Report {
            command: ReportCommand::Retro { work_item },
        } => Action::ReportRetro { work_item },
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
