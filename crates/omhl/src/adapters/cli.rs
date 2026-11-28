use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::builder::PossibleValuesParser;
use clap::error::ErrorKind;
use clap::{ArgGroup, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use hexlog::SemanticId;
use serde_json::{Value, json};

use super::hook;
use super::text::{CommandKey, Layout, render_failure, render_success, renderer_for};
use crate::app::action::{Action, Caller, CheckTarget};
use crate::app::error::AppError;
use crate::app::input::{INPUT_MAX_BYTES, read_json_input};
use crate::app::runtime::run_action;
use crate::app::{Group, RUNNABLE_EVENTS};
use crate::project::Env;
use crate::provider::Strategy;

#[derive(Debug, Parser)]
#[command(name = "omhl", version, arg_required_else_help = true)]
pub struct Cli {
    /// Path of the database file.
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,
    /// Directory where the project marker lookup starts.
    #[arg(long, global = true)]
    pub project_dir: Option<PathBuf>,
    /// Output format: `agent` (text for agents), `human` (text for people) or `json` (one JSON line). The default of each command is in `omhl --help` and in the help of the command; a command whose default is `json` only prints JSON.
    #[arg(long, global = true, value_enum)]
    pub format: Option<OutputFormat>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(flatten)]
    Action(ActionCommand),
    /// Run one hook of the `omhl hook` group: session-start, spec-lock, agent-role, subagent-start, branch-birth, command-guard, mcp-merge-guard, mcp-write-warning and branch-created.
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
    Append {
        /// Owner only: let through the refusals that loosen a check or a work item type.
        #[arg(long)]
        owner_override: bool,
    },
    /// Record a check outcome for a work item.
    Attest {
        check: String,
        work_item: String,
        #[arg(long)]
        outcome: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Print the session context.
    Render,
    /// Evaluate the delivery gate for a work item.
    GateDelivery {
        work_item: String,
        /// The integration branch the pull request targets; it decides whether the config review is required.
        #[arg(long, allow_hyphen_values = true)]
        base: Option<String>,
    },
    /// Decide whether a worktree may be created for a branch; silent without a project marker.
    GateWorktree {
        #[arg(long, allow_hyphen_values = true)]
        branch: String,
    },
    /// Close a work item once the gate is evaluated; `--force --note` closes it past warn questions, `--abandon` closes it without the gate and `--failed-delivery` closes a merged one that still fails.
    #[command(group(clap::ArgGroup::new("mode").args(["force", "abandon", "failed_delivery"]).multiple(true)))]
    Close {
        work_item: String,
        #[arg(long, requires = "note", conflicts_with = "abandon")]
        force: bool,
        #[arg(long, requires_all = ["reason", "note"])]
        abandon: bool,
        #[arg(long, requires = "note", conflicts_with_all = ["force", "abandon"])]
        failed_delivery: bool,
        #[arg(long, requires = "abandon", conflicts_with = "failed_delivery")]
        reason: Option<String>,
        #[arg(long, requires = "mode")]
        note: Option<String>,
        #[arg(long, allow_hyphen_values = true, conflicts_with = "abandon")]
        base: Option<String>,
    },
    /// Merge the pull request of a work item through the provider and close the work item; refuses before the merge when the CI or the delivery gate does not pass.
    Merge {
        work_item: String,
        /// Squash the commits instead of a merge commit.
        #[arg(long, conflicts_with = "rebase")]
        squash: bool,
        /// Rebase the commits instead of a merge commit.
        #[arg(long)]
        rebase: bool,
        /// Pass a red CI check that branch protection does not require, or warn questions of the gate, with the owner's authorization in --note.
        #[arg(long, requires = "note")]
        force: bool,
        #[arg(long, requires = "force")]
        note: Option<String>,
        /// The base of the loosening review, as in close.
        #[arg(long, allow_hyphen_values = true)]
        base: Option<String>,
    },
    /// Verify the hash chain.
    Verify,
    /// Load the project extensions into the tenant.
    Load {
        /// Report the plan without writing anything.
        #[arg(long)]
        dry_run: bool,
        /// Owner only: let a revision that loosens a stored check through.
        #[arg(long)]
        owner_override: bool,
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
    /// Print the last lines of the stored output of a check for a work item, or prune the stored outputs with `prune`.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Byproduct {
        #[command(subcommand)]
        command: Option<ByproductCommand>,
        // `Option` com `required`: com `String` o clap falha ao montar o subcomando; o `None` só existe fora do clap e o adaptador o recusa.
        #[arg(required = true)]
        check: Option<String>,
        #[arg(required = true)]
        work_item: Option<String>,
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
    /// Read-only reports over the records.
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum CheckCommand {
    /// Run the mechanical checks of an event, or one check, and attest them.
    ///
    /// Events: before-commit (git pre-commit and pre-merge-commit hooks), before-push (git pre-push hook), before-merge (by hand, no hook), before-publishing-review-comments (an instruction to the agent; `--event` does not take it).
    Run(CheckRunArgs),
    /// Kill the processes that escaped from an earlier check run whose omhl already exited.
    Sweep,
    /// Retire the stored check whose file was deleted from the working tree.
    Retire {
        check: String,
        /// Owner only: retiring a check loosens it; this lets the retirement through.
        #[arg(long)]
        owner_override: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ByproductCommand {
    /// Delete the stored outputs older than 7 days that no live attestation points to, now and without looking at the daily stamp; prints `removed`, `kept_live` and `freed_bytes`. The `check run` already prunes by itself, at most once a day per database and project.
    Prune,
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
    /// Install the git hooks (pre-commit, pre-merge-commit, pre-push, post-commit, post-merge) of this clone; these are git hooks, unrelated to the `omhl hook` group (Claude Code events and the `branch-created` hook).
    Install {
        #[arg(long)]
        force: bool,
    },
    /// Consume the commit mark after a commit on a dry-run-with-mark exception branch.
    PostCommit,
    /// Consume the commit mark after a merge on a dry-run-with-mark exception branch; the squash flag 1 keeps it.
    PostMerge {
        /// The flag git passes to post-merge: 1 for a squash merge.
        squash_flag: String,
    },
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
    /// List the real Claude Code sessions that wrote records: first and last record, nodes, relations and work items touched.
    Sessions {
        /// Id of a work item, such as work-items.w1; repeatable. Keeps the sessions that wrote a record of any of them; without it, every session is listed.
        #[arg(long)]
        work_item: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
// Os eventos do Claude Code leem o evento no stdin; o `branch-created` não lê. Quem decide o código de saída que bloqueia é o `omhl-hook.sh`, cujo ramo sem o binário sai com `exit 0` fora de projeto marcado (`docs/directives-flow.md`).
pub enum HookEvent {
    /// Inject the session context.
    SessionStart,
    /// Deny edits to the spec of a work item that already has a branch, and deny them when git or the spec folder cannot be read; also deny a change of `specs.dir` in the main workflow.toml while a spec is frozen, any edit under a `.git` folder of an omhl project, and an edit that would leave the project marker invalid; reads the PreToolUse event on stdin.
    SpecLock,
    /// Impose the role matrix on the Bash and Monitor tools of plugin agents and deny a plugin agent call that carries `name`; reads the PreToolUse event on stdin.
    AgentRole,
    /// Print the conversation language of the person as context for a plugin subagent; reads the SubagentStart event on stdin, never blocks, and prints nothing without a language.
    SubagentStart,
    /// Write the freeze mark of every work item with a local branch after a Bash command that created a branch; reads the PostToolUse event on stdin and prints nothing.
    BranchBirth,
    /// Run the creation gate on a Bash branch creation and warn on a push that reaches main; reads the PreToolUse event on stdin.
    CommandGuard,
    /// Guard the merge of a pull request through an MCP tool; reads the PreToolUse event on stdin.
    McpMergeGuard,
    /// Warn when an MCP write tool targets main or the default branch; reads the PreToolUse event on stdin.
    McpWriteWarning,
    /// Write the freeze mark in every spec folder of the work item of a new branch and keep the worktree folder ignored; does not read stdin, prints JSON, and stays silent without a project marker.
    BranchCreated {
        /// Name of the new branch.
        #[arg(long)]
        branch: String,
    },
}

pub fn run(
    args: impl Iterator<Item = OsString>,
    stdin: &mut impl Read,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
    env: &Env<'_>,
) -> ExitCode {
    let parsed = command()
        .try_get_matches_from(args)
        .and_then(|matches| Cli::from_arg_matches(&matches));
    let cli = match parsed {
        Ok(cli) => cli,
        Err(usage) => return report_usage(&usage, out, err),
    };
    let key = command_key(&cli.command);
    if let Some(usage) = pre_push_misuse(&cli)
        .or_else(|| byproduct_misuse(&cli))
        .or_else(|| format_misuse(&cli, key))
    {
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
            // Os grupos vêm do clap: o hook não mantém uma segunda lista de folhas.
            hook::agent_role(stdin, out, err, env, &command_groups(&Cli::command()))
        }
        Command::Hook {
            event: HookEvent::SubagentStart,
        } => hook::subagent_start(stdin, out, env),
        Command::Hook {
            event: HookEvent::BranchBirth,
        } => hook::branch_birth(stdin, err),
        Command::Hook {
            event: HookEvent::CommandGuard,
        } => hook::command_guard(stdin, out, err),
        Command::Hook {
            event: HookEvent::McpMergeGuard,
        } => hook::mcp_merge_guard(stdin, out, err),
        Command::Hook {
            event: HookEvent::McpWriteWarning,
        } => hook::mcp_write_warning(stdin, out, err),
        Command::Hook {
            event: HookEvent::BranchCreated { branch },
        } => {
            let format = format.unwrap_or_else(|| default_format(key));
            let result = start_dir(project_dir.as_deref())
                .and_then(|dir| crate::app::branch_created(&dir, &branch));

            report_silent_without_marker(result, format, key, out, err)
        }
        Command::Action(command) => {
            let format = format.unwrap_or_else(|| default_format(key));
            let result = start_dir(project_dir.as_deref())
                .and_then(|dir| execute(command, stdin, &dir, db, env));

            report_silent_without_marker(result, format, key, out, err)
        }
    }
}

/// O `gate-worktree` e o `branch-created` rodam em qualquer repositório: sem marcador não há o que decidir nem marcar.
fn report_silent_without_marker(
    result: Result<Value, AppError>,
    format: OutputFormat,
    key: CommandKey,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
) -> ExitCode {
    let silent_without_marker = matches!(
        key,
        CommandKey::GateWorktree | CommandKey::HookBranchCreated
    );
    match &result {
        Err(error) if silent_without_marker && error.code() == "no-marker" => ExitCode::SUCCESS,
        _ => report(result, format, key, out, err),
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

    (args.pre_push && args.event.as_deref() != Some("before-push")).then(|| {
        Cli::command().error(
            ErrorKind::ArgumentConflict,
            "--pre-push is only valid with --event before-push",
        )
    })
}

const BYPRODUCT_NEEDS_TARGET: &str =
    "byproduct needs <CHECK> and <WORK_ITEM>, or the `prune` subcommand";

/// `check` e `work_item` são `Option` por causa do subcomando `prune`; o clap já os exige sem ele, e o `None` que sobra sai pelo erro de uso (código `2`).
fn byproduct_misuse(cli: &Cli) -> Option<clap::Error> {
    let Command::Action(ActionCommand::Byproduct {
        command: None,
        check,
        work_item,
        ..
    }) = &cli.command
    else {
        return None;
    };

    (check.is_none() || work_item.is_none())
        .then(|| Cli::command().error(ErrorKind::MissingRequiredArgument, BYPRODUCT_NEEDS_TARGET))
}

/// Os comandos de primeiro nível com subcomando (o `help` implícito do clap não conta).
pub fn command_groups(root: &clap::Command) -> Vec<Group> {
    root.get_subcommands()
        .filter(|command| real_subcommands(command).next().is_some())
        .map(|command| Group {
            name: command.get_name().to_owned(),
            subcommands: real_subcommands(command)
                .map(|sub| sub.get_name().to_owned())
                .collect(),
            takes_positionals: command.get_positionals().next().is_some(),
        })
        .collect()
}

/// Cada folha como caminho (`check run`): um comando sem subcomando, ou um grupo híbrido (com argumentos próprios), que também desce nos filhos.
pub fn command_leaves(root: &clap::Command) -> Vec<String> {
    let mut leaves = Vec::new();
    for command in real_subcommands(root) {
        collect_leaf_paths(command, "", &mut leaves);
    }

    leaves
}

fn collect_leaf_paths(command: &clap::Command, parent: &str, leaves: &mut Vec<String>) {
    let path = match parent {
        "" => command.get_name().to_owned(),
        _ => format!("{parent} {}", command.get_name()),
    };
    if real_subcommands(command).next().is_none() || command.get_positionals().next().is_some() {
        leaves.push(path.clone());
    }
    for child in real_subcommands(command) {
        collect_leaf_paths(child, &path, leaves);
    }
}

fn real_subcommands(command: &clap::Command) -> impl Iterator<Item = &clap::Command> {
    command
        .get_subcommands()
        .filter(|sub| sub.get_name() != "help")
}

/// `agent` e `human` só existem onde o comando tem renderizador; a recusa sai pelo mesmo caminho do erro de uso.
fn format_misuse(cli: &Cli, key: CommandKey) -> Option<clap::Error> {
    if !matches!(cli.format, Some(OutputFormat::Agent | OutputFormat::Human))
        || renderer_for(key).is_some()
    {
        return None;
    }

    Some(Cli::command().error(
        ErrorKind::InvalidValue,
        "this command only prints JSON; use --format json",
    ))
}

fn default_format(key: CommandKey) -> OutputFormat {
    match renderer_for(key).is_some() {
        true => OutputFormat::Agent,
        false => OutputFormat::Json,
    }
}

/// O clap com o padrão de saída de cada comando no help: o texto sai de `default_format`, o mesmo fato que escolhe o formato na execução.
fn command() -> clap::Command {
    let listing: Vec<String> = OutputFormat::value_variants()
        .iter()
        .filter_map(|format| {
            let paths: Vec<&str> = CommandKey::ALL
                .into_iter()
                .filter(|key| default_format(*key) == *format)
                .map(command_path)
                .collect();
            (!paths.is_empty()).then(|| format!("  {}: {}", format_name(*format), paths.join(", ")))
        })
        .collect();
    let root = Cli::command().after_help(format!(
        "Default output format of each command:\n{}",
        listing.join("\n")
    ));

    CommandKey::ALL.into_iter().fold(root, |command, key| {
        let path: Vec<&str> = command_path(key).split(' ').collect();
        let note = format!(
            "Default output format: {}.",
            format_name(default_format(key))
        );

        annotate(command, &path, note)
    })
}

/// Desce pelos subcomandos do `path` e põe a nota no fim do help do comando folha.
fn annotate(command: clap::Command, path: &[&str], note: String) -> clap::Command {
    match path.split_first() {
        Some((name, rest)) => command.mut_subcommand(*name, |sub| annotate(sub, rest, note)),
        None => command.after_help(note),
    }
}

fn format_name(format: OutputFormat) -> String {
    format
        .to_possible_value()
        .map(|value| value.get_name().to_owned())
        .unwrap_or_default()
}

/// Como o comando se escreve na CLI; sem braço `_`: chave nova falha na compilação até ganhar o caminho.
fn command_path(key: CommandKey) -> &'static str {
    match key {
        CommandKey::List => "list",
        CommandKey::Get => "get",
        CommandKey::Append => "append",
        CommandKey::Attest => "attest",
        CommandKey::Render => "render",
        CommandKey::GateDelivery => "gate-delivery",
        CommandKey::GateWorktree => "gate-worktree",
        CommandKey::Close => "close",
        CommandKey::Merge => "merge",
        CommandKey::Verify => "verify",
        CommandKey::Load => "load",
        CommandKey::Project => "project",
        CommandKey::CheckRun => "check run",
        CommandKey::CheckSweep => "check sweep",
        CommandKey::CheckRetire => "check retire",
        CommandKey::GuidelineRetire => "guideline retire",
        CommandKey::Byproduct => "byproduct",
        CommandKey::ByproductPrune => "byproduct prune",
        CommandKey::HooksInstall => "hooks install",
        CommandKey::HooksPostCommit => "hooks post-commit",
        CommandKey::HooksPostMerge => "hooks post-merge",
        CommandKey::SpecInit => "spec init",
        CommandKey::SpecValidate => "spec validate",
        CommandKey::ReportAnswers => "report answers",
        CommandKey::ReportRecurrences => "report recurrences",
        CommandKey::ReportRetro => "report retro",
        CommandKey::ReportSessions => "report sessions",
        CommandKey::HookSessionStart => "hook session-start",
        CommandKey::HookSpecLock => "hook spec-lock",
        CommandKey::HookAgentRole => "hook agent-role",
        CommandKey::HookSubagentStart => "hook subagent-start",
        CommandKey::HookBranchCreated => "hook branch-created",
        CommandKey::HookBranchBirth => "hook branch-birth",
        CommandKey::HookCommandGuard => "hook command-guard",
        CommandKey::HookMcpMergeGuard => "hook mcp-merge-guard",
        CommandKey::HookMcpWriteWarning => "hook mcp-write-warning",
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
        Command::Hook {
            event: HookEvent::SubagentStart,
        } => CommandKey::HookSubagentStart,
        Command::Hook {
            event: HookEvent::BranchCreated { .. },
        } => CommandKey::HookBranchCreated,
        Command::Hook {
            event: HookEvent::BranchBirth,
        } => CommandKey::HookBranchBirth,
        Command::Hook {
            event: HookEvent::CommandGuard,
        } => CommandKey::HookCommandGuard,
        Command::Hook {
            event: HookEvent::McpMergeGuard,
        } => CommandKey::HookMcpMergeGuard,
        Command::Hook {
            event: HookEvent::McpWriteWarning,
        } => CommandKey::HookMcpWriteWarning,
        Command::Action(ActionCommand::List { .. }) => CommandKey::List,
        Command::Action(ActionCommand::Get { .. }) => CommandKey::Get,
        Command::Action(ActionCommand::Append { .. }) => CommandKey::Append,
        Command::Action(ActionCommand::Attest { .. }) => CommandKey::Attest,
        Command::Action(ActionCommand::Render) => CommandKey::Render,
        Command::Action(ActionCommand::GateDelivery { .. }) => CommandKey::GateDelivery,
        Command::Action(ActionCommand::GateWorktree { .. }) => CommandKey::GateWorktree,
        Command::Action(ActionCommand::Close { .. }) => CommandKey::Close,
        Command::Action(ActionCommand::Merge { .. }) => CommandKey::Merge,
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
        Command::Action(ActionCommand::Byproduct {
            command: Some(ByproductCommand::Prune),
            ..
        }) => CommandKey::ByproductPrune,
        Command::Action(ActionCommand::Byproduct { command: None, .. }) => CommandKey::Byproduct,
        Command::Action(ActionCommand::Hooks {
            command: HooksCommand::Install { .. },
        }) => CommandKey::HooksInstall,
        Command::Action(ActionCommand::Hooks {
            command: HooksCommand::PostCommit,
        }) => CommandKey::HooksPostCommit,
        Command::Action(ActionCommand::Hooks {
            command: HooksCommand::PostMerge { .. },
        }) => CommandKey::HooksPostMerge,
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
        Command::Action(ActionCommand::Report {
            command: ReportCommand::Sessions { .. },
        }) => CommandKey::ReportSessions,
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
            let text = render_success(key, &value, layout_of(format));
            // O `text` do `render` já termina em quebra de linha: sai byte a byte, sem quebra extra.
            let _ = match key {
                CommandKey::Render => write!(out, "{text}"),
                _ => writeln!(out, "{text}"),
            };
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

fn has_owner_override(command: &ActionCommand) -> bool {
    matches!(
        command,
        ActionCommand::Append {
            owner_override: true
        } | ActionCommand::Load {
            owner_override: true,
            ..
        } | ActionCommand::Check {
            command: CheckCommand::Retire {
                owner_override: true,
                ..
            }
        }
    )
}

fn execute(
    command: ActionCommand,
    stdin: &mut impl Read,
    start: &Path,
    db: Option<&Path>,
    env: &Env<'_>,
) -> Result<Value, AppError> {
    // Depois do `git merge --squash` o commit que fecha ainda precisa da marca.
    if let ActionCommand::Hooks {
        command: HooksCommand::PostMerge { squash_flag },
    } = &command
        && squash_flag == "1"
    {
        return Ok(json!({ "consumed": false }));
    }
    let owner_override = has_owner_override(&command);
    let action = to_action(command, stdin, env)?;

    let caller = Caller {
        owner_override,
        ..Caller::cli_with_role(env.claude_code, env.role.as_deref())?
    };

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
        ActionCommand::Append { .. } => Action::Append {
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
        ActionCommand::GateDelivery { work_item, base } => Action::GateDelivery { work_item, base },
        ActionCommand::GateWorktree { branch } => Action::GateWorktree { branch },
        ActionCommand::Close {
            work_item,
            abandon: true,
            reason: Some(reason),
            note: Some(note),
            ..
        } => Action::Abandon {
            work_item,
            reason,
            note,
        },
        ActionCommand::Close {
            work_item,
            failed_delivery: true,
            note: Some(note),
            base,
            ..
        } => Action::CloseFailedDelivery {
            work_item,
            note,
            base,
        },
        ActionCommand::Close {
            work_item,
            note,
            base,
            ..
        } => Action::Close {
            work_item,
            note,
            base,
        },
        ActionCommand::Merge {
            work_item,
            squash,
            rebase,
            note,
            base,
            ..
        } => Action::Merge {
            work_item: SemanticId::parse(&work_item)
                .map_err(|error| AppError::from(error.at("/work_item")))?,
            strategy: match (squash, rebase) {
                (true, _) => Strategy::Squash,
                (_, true) => Strategy::Rebase,
                _ => Strategy::Merge,
            },
            force_note: note,
            base,
        },
        ActionCommand::Verify => Action::Verify,
        ActionCommand::Load { dry_run, .. } => Action::Load { dry_run },
        ActionCommand::Project => Action::Project,
        ActionCommand::Check {
            command: CheckCommand::Run(args),
        } => check_run(args, stdin)?,
        ActionCommand::Check {
            command: CheckCommand::Sweep,
        } => Action::CheckSweep,
        ActionCommand::Check {
            command: CheckCommand::Retire { check, .. },
        } => Action::CheckRetire { check },
        ActionCommand::Guideline {
            command: GuidelineCommand::Retire { id, by },
        } => Action::GuidelineRetire { id, by },
        ActionCommand::Byproduct {
            command: Some(ByproductCommand::Prune),
            ..
        } => Action::ByproductPrune,
        ActionCommand::Byproduct {
            command: None,
            check: Some(check),
            work_item: Some(work_item),
            tail,
        } => Action::Byproduct {
            check,
            work_item,
            tail,
        },
        ActionCommand::Byproduct { command: None, .. } => {
            return Err(AppError::invalid_arguments("", BYPRODUCT_NEEDS_TARGET));
        }
        ActionCommand::Hooks {
            command: HooksCommand::Install { force },
        } => Action::HooksInstall { force },
        ActionCommand::Hooks {
            command: HooksCommand::PostCommit,
        } => Action::ConsumeCommitMark,
        ActionCommand::Hooks {
            command: HooksCommand::PostMerge { .. },
        } => Action::ConsumeCommitMark,
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
        ActionCommand::Report {
            command: ReportCommand::Sessions { work_item },
        } => Action::ReportSessions {
            work_items: work_item,
        },
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
    AppError::push_input_invalid("").with_context(json!({ "kind": kind, "event": "before-push" }))
}

#[cfg(test)]
mod tests;
