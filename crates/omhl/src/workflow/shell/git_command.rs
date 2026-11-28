//! Reconhece criação de branch, `git push` e o merge do provedor; só lê o texto, nunca inicia o programa.

use super::{SimpleCommand, read_command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchName {
    Literal(String),
    /// Substituição, variável ou crase: só o shell sabe o nome.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushTarget {
    Branch(String),
    Delete(String),
    All,
    Mirror,
    /// Push sem refspec: o destino é a branch atual, que o chamador lê.
    Current,
}

impl PushTarget {
    pub fn hits(&self, branch: &str) -> bool {
        match self {
            PushTarget::Branch(name) | PushTarget::Delete(name) => name == branch,
            PushTarget::All | PushTarget::Mirror => true,
            PushTarget::Current => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCommand {
    CreateBranch(BranchName),
    Push(Vec<PushTarget>),
    ProviderMerge,
}

const GIT: &str = "git";
const GH: &str = "gh";

/// Todos os comandos reconhecidos de uma linha composta, na ordem; uma linha ilegível não reconhece nada.
pub fn recognize(command: &str) -> Vec<GitCommand> {
    let Ok(line) = read_command(command) else {
        return Vec::new();
    };
    line.commands.iter().filter_map(recognize_simple).collect()
}

fn recognize_simple(command: &SimpleCommand) -> Option<GitCommand> {
    let (_, program, args) = command.split();
    match program? {
        GIT => recognize_git(git_subcommand(args)?),
        GH => recognize_gh(args),
        _ => None,
    }
}

/// Só `-C <caminho>` e `-c <chave=valor>` passam; outra opção global fica sem reconhecer.
fn git_subcommand(args: &[String]) -> Option<&[String]> {
    let mut at = 0;
    while let Some(word) = args.get(at) {
        match word.as_str() {
            "-C" | "-c" => at += 2,
            option if option.starts_with('-') => return None,
            _ => return Some(&args[at..]),
        }
    }
    None
}

fn recognize_git(words: &[String]) -> Option<GitCommand> {
    let (subcommand, args) = words.split_first()?;
    match subcommand.as_str() {
        "switch" => value_after(args, &["-c", "-C", "--create", "--force-create"]).map(create),
        "checkout" => value_after(args, &["-b", "-B"]).map(create),
        "branch" => branch_creation(args).map(create),
        "worktree" => match args.split_first() {
            Some((add, rest)) if add == "add" => value_after(rest, &["-b", "-B"]).map(create),
            _ => None,
        },
        "push" => push_targets(args).map(GitCommand::Push),
        _ => None,
    }
}

fn recognize_gh(args: &[String]) -> Option<GitCommand> {
    match args {
        [pr, merge, ..] if pr == "pr" && merge == "merge" => Some(GitCommand::ProviderMerge),
        _ => None,
    }
}

fn create(name: &str) -> GitCommand {
    let name = if name.contains('$') || name.contains('`') {
        BranchName::Unknown
    } else {
        BranchName::Literal(name.to_owned())
    };
    GitCommand::CreateBranch(name)
}

fn value_after<'a>(args: &'a [String], options: &[&str]) -> Option<&'a str> {
    let at = args
        .iter()
        .position(|word| options.contains(&word.as_str()))?;
    args.get(at + 1).map(String::as_str)
}

/// Opções que não tiram o `git branch <nome>` da criação; qualquer outra lista, apaga, renomeia ou mostra.
const BRANCH_CREATE_OPTIONS: [&str; 6] = ["-f", "--force", "-t", "--track", "--no-track", "-q"];

fn branch_creation(args: &[String]) -> Option<&str> {
    let mut positionals = Vec::new();
    for word in args {
        if word.starts_with('-') {
            if !BRANCH_CREATE_OPTIONS.contains(&word.as_str()) {
                return None;
            }
        } else {
            positionals.push(word.as_str());
        }
    }
    match positionals.as_slice() {
        [name] | [name, _] => Some(*name),
        _ => None,
    }
}

const PUSH_VALUED_OPTIONS: [&str; 5] =
    ["-o", "--push-option", "--repo", "--receive-pack", "--exec"];

fn push_targets(args: &[String]) -> Option<Vec<PushTarget>> {
    let mut delete = false;
    let mut targets = Vec::new();
    let mut positionals = Vec::new();
    let mut at = 0;
    while let Some(word) = args.get(at) {
        at += 1;
        match word.as_str() {
            "--dry-run" | "-n" => return None,
            "--delete" | "-d" => delete = true,
            "--all" | "--branches" => targets.push(PushTarget::All),
            "--mirror" => targets.push(PushTarget::Mirror),
            option if PUSH_VALUED_OPTIONS.contains(&option) => at += 1,
            option if option.starts_with('-') => {}
            _ => positionals.push(word.as_str()),
        }
    }
    targets.extend(
        positionals
            .iter()
            .skip(1)
            .map(|spec| refspec_target(spec, delete)),
    );
    if targets.is_empty() {
        targets.push(PushTarget::Current);
    }
    Some(targets)
}

fn refspec_target(spec: &str, delete: bool) -> PushTarget {
    let spec = spec.strip_prefix('+').unwrap_or(spec);
    if delete {
        return PushTarget::Delete(short_ref(spec).to_owned());
    }
    match spec.split_once(':') {
        Some(("", destination)) => PushTarget::Delete(short_ref(destination).to_owned()),
        Some((_, destination)) => PushTarget::Branch(short_ref(destination).to_owned()),
        None if spec == "HEAD" => PushTarget::Current,
        None => PushTarget::Branch(short_ref(spec).to_owned()),
    }
}

fn short_ref(name: &str) -> &str {
    name.strip_prefix("refs/heads/").unwrap_or(name)
}

#[cfg(test)]
mod tests;
