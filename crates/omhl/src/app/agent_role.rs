use std::iter::Peekable;
use std::path::Path;

use serde_json::{Value, json};

use crate::guard::{ROLE_PREFIX, ROLE_VARIABLE, known_role};
use crate::ports::files::read_regular_utf8;
use crate::workflow::omhl_leaf::resolve_leaf;
use crate::workflow::role_allow::{RoleAllow, parse_role_allow};
use crate::workflow::shell::{
    CommandLine, SimpleCommand, is_omhl_word, mentions_omhl, read_command,
};

/// O `agent_type` real dos subagentes do plugin é `oh-my-hexflow:<papel>`.
const PLUGIN_PREFIX: &str = "oh-my-hexflow:";
const ASSIGNING_PROGRAMS: [&str; 5] = ["export", "declare", "typeset", "readonly", "local"];
/// Com `-p` e nomes, só estes imprimem o que o nome é; `export -p` e `readonly -p` atribuem (bash 5.2).
const PRINTING_PROGRAMS: [&str; 2] = ["declare", "typeset"];
const AGENT_FILE_MAX_BYTES: u64 = 64 * 1024;
const BLANKS: [char; 2] = [' ', '\t'];

/// O que o hook decide: `Pass` deixa o fluxo normal de permissão; `Allow` troca o `tool_input` pelo reescrito.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleVerdict {
    Pass,
    Deny {
        code: &'static str,
        reason: String,
    },
    Allow {
        reason: String,
        updated_input: Value,
    },
}

/// O que o hook lê fora do evento: a raiz do plugin e os comandos do `omhl` que só existem com subcomando.
pub struct AgentRoleEnv<'e> {
    pub plugin_root: Option<&'e Path>,
    pub groups: &'e [&'e str],
}

/// O hook `agent-role` para o evento `PreToolUse` do Bash, do Monitor e da tool Agent; não abre o banco, só lê o arquivo do agente.
pub fn agent_role(input: &Value, env: &AgentRoleEnv<'_>) -> RoleVerdict {
    if matches!(input["tool_name"].as_str(), Some("Agent" | "Task")) {
        return agent_call_verdict(&input["tool_input"]);
    }
    let Some(command) = input["tool_input"]["command"].as_str() else {
        return RoleVerdict::Pass;
    };
    let line = read_command(command);
    if let Some(change) = line.as_ref().ok().and_then(role_change) {
        return change.denial();
    }
    let Some(role) = input["agent_type"]
        .as_str()
        .and_then(|agent_type| agent_type.strip_prefix(PLUGIN_PREFIX))
    else {
        return RoleVerdict::Pass;
    };

    match line {
        // O bash executa `$(...)` e crase no corpo de um heredoc sem aspas, fora do prefixo e da matriz.
        Ok(line) if line.executes_heredoc && line.mentions_omhl => deny(
            "heredoc-substitution",
            "the heredoc body runs $( ) or backticks; quote the terminator (<<'EOF') so the body stays text",
        ),
        Ok(line) => ported_agent_verdict(role, &line, input, env),
        // O erro de leitura só pesa para o agente portado, e só se o comando fala de `omhl`.
        Err(_) if mentions_omhl(command) => deny(
            "command-unreadable",
            "the command has an unclosed quote or heredoc, is too long or nests too deeply, and mentions omhl; fix the command",
        ),
        Err(_) => RoleVerdict::Pass,
    }
}

/// Com `name` o Claude Code não pré-carrega o `skills:` do agente do plugin; a chamada é negada, nunca reescrita (outro plugin pode reescrever o mesmo `tool_input`).
fn agent_call_verdict(tool_input: &Value) -> RoleVerdict {
    let plugin_agent = tool_input["subagent_type"]
        .as_str()
        .is_some_and(|subagent_type| subagent_type.starts_with(PLUGIN_PREFIX));
    let named = tool_input["name"]
        .as_str()
        .is_some_and(|name| !name.is_empty());
    if !(plugin_agent && named) {
        return RoleVerdict::Pass;
    }

    deny(
        "agent-named",
        "a plugin agent called with `name` does not preload its skills; repeat the call without `name`",
    )
}

/// O que um comando faz com `OMHL_ROLE`: dá um valor a ele ou o tira do ambiente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoleChange {
    Assign,
    Unset,
}

impl RoleChange {
    fn denial(self) -> RoleVerdict {
        match self {
            Self::Assign => deny(
                "role-assignment",
                "OMHL_ROLE is set by the plugin hook only; do not assign it in a command",
            ),
            Self::Unset => deny(
                "role-unset",
                "OMHL_ROLE is set by the plugin hook only; do not unset it in a command",
            ),
        }
    }
}

/// A primeira mudança de `OMHL_ROLE` que algum comando faz: como prefixo (também depois de embrulho) ou como argumento de `export` e afins.
fn role_change(line: &CommandLine) -> Option<RoleChange> {
    line.commands.iter().find_map(|simple| {
        let (prefix, program, args) = simple.split();

        prefix_role_change(prefix)
            .or_else(|| program.and_then(|program| builtin_role_change(program, args)))
    })
}

/// As palavras antes do programa: o valor de `env -u` / `--unset` tira o papel, `OMHL_ROLE=` em qualquer palavra o atribui.
/// O nome nu em outra palavra (`env -i grep OMHL_ROLE file`) não muda nada.
fn prefix_role_change(prefix: &[String]) -> Option<RoleChange> {
    let mut words = prefix.iter().map(String::as_str).peekable();
    while let Some(word) = words.next() {
        let change = match env_unset_value(word, &mut words) {
            Some(value) if value == ROLE_VARIABLE => Some(RoleChange::Unset),
            Some(value) => assigns_role_variable(value).then_some(RoleChange::Assign),
            None => assigns_role_variable(word).then_some(RoleChange::Assign),
        };
        if change.is_some() {
            return change;
        }
    }

    None
}

/// Os argumentos de `export` e afins: `OMHL_ROLE=` atribui e o nome nu também, salvo em `declare -p NOME` / `typeset -p NOME`.
/// O `-p` só vale entre as opções: depois do primeiro nome ou do `--` ele já é um nome.
fn builtin_role_change(program: &str, args: &[String]) -> Option<RoleChange> {
    if !ASSIGNING_PROGRAMS.contains(&program) {
        return None;
    }
    let option_count = args
        .iter()
        .take_while(|word| word.starts_with('-') && word.as_str() != "--")
        .count();
    let (options, names) = args.split_at(option_count);
    let only_prints = PRINTING_PROGRAMS.contains(&program)
        && options
            .iter()
            .any(|option| !option.starts_with("--") && option.contains('p'));

    names
        .iter()
        .any(|name| assigns_role_variable(name) || (name == ROLE_VARIABLE && !only_prints))
        .then_some(RoleChange::Assign)
}

/// O valor que o getopt do `env` dá à opção `-u`/`--unset` na palavra; `None` se a palavra não é essa opção.
/// Num cluster `-<letras>u<resto>` o valor é `<resto>` ou, se vazio, a palavra seguinte; `--u`..`--unset` são a mesma opção longa.
/// O `u` só abre o valor quando as letras antes dele são flags do `env` sem valor: em `-Cu` o `u` é parte do valor do `-C`.
fn env_unset_value<'w>(
    word: &'w str,
    following: &mut Peekable<impl Iterator<Item = &'w str>>,
) -> Option<&'w str> {
    if let Some(long) = word.strip_prefix("--") {
        let (name, attached) = match long.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (long, None),
        };

        return (!name.is_empty() && "unset".starts_with(name))
            .then(|| attached.or_else(|| following.next()).unwrap_or_default());
    }
    let (flags, tail) = word.strip_prefix('-')?.split_once('u')?;
    if !flags.chars().all(|flag| matches!(flag, 'i' | '0' | 'v')) {
        return None;
    }

    Some(if tail.is_empty() {
        following.peek().copied().unwrap_or_default()
    } else {
        tail
    })
}

/// `OMHL_ROLE=` ou `OMHL_ROLE+=`.
fn assigns_role_variable(word: &str) -> bool {
    word.strip_prefix(ROLE_VARIABLE)
        .is_some_and(|rest| rest.starts_with('=') || rest.starts_with("+="))
}

/// Papel fora da lista, comando sem `omhl`, comando composto e embrulho que o leitor não pula.
fn ported_agent_verdict(
    role: &str,
    line: &CommandLine,
    input: &Value,
    env: &AgentRoleEnv<'_>,
) -> RoleVerdict {
    let reaching: Vec<&SimpleCommand> = line
        .commands
        .iter()
        .filter(|simple| simple.may_run_omhl(line.mentions_omhl))
        .collect();
    if reaching.is_empty() {
        return RoleVerdict::Pass;
    }
    if known_role(&agent_name(role)).is_none() {
        return deny(
            "role-unknown",
            "this plugin agent has no omhl role matrix; it cannot run omhl",
        );
    }

    match reaching.as_slice() {
        [simple] if !line.compound && simple.program().is_some_and(is_omhl_word) => {
            role_matrix_verdict(role, line, simple, input, env)
        }
        [_] if !line.compound => deny(
            "wrapper-unreadable",
            "the omhl call sits behind a wrapper option the hook cannot read; run omhl directly",
        ),
        _ => deny(
            "compound-command",
            "omhl must be the only command of the call: no pipe, chain, substitution or second line",
        ),
    }
}

/// A matriz `omhl-allow` do papel decide o único comando `omhl`; o `allow` só sai quando o prefixo chega ao `omhl`.
fn role_matrix_verdict(
    role: &str,
    line: &CommandLine,
    simple: &SimpleCommand,
    input: &Value,
    env: &AgentRoleEnv<'_>,
) -> RoleVerdict {
    if let Some(denial) = matrix_denial(role, simple, env) {
        return denial;
    }
    if !line.clean_start || simple.follows_keyword() {
        return deny(
            "role-prefix-unreachable",
            "start the command with omhl (after spaces or env/timeout/time only): no comment, blank line, redirection, `!` or control keyword before it",
        );
    }

    RoleVerdict::Allow {
        reason: format!("the command is inside the {} matrix", agent_name(role)),
        updated_input: with_role_prefix(input, role),
    }
}

/// O motivo de negar quando a matriz do papel não cobre o comando; `None` quando cobre.
fn matrix_denial(
    role: &str,
    simple: &SimpleCommand,
    env: &AgentRoleEnv<'_>,
) -> Option<RoleVerdict> {
    let allow = match read_role_allow(role, env.plugin_root) {
        Ok(allow) => allow,
        Err(problem) => {
            return Some(deny(
                "role-matrix-unreadable",
                &format!("a ported role without a valid omhl-allow runs no omhl: {problem}"),
            ));
        }
    };
    let leaf = resolve_leaf(simple.split().2, env.groups);

    (!leaf.as_ref().is_some_and(|leaf| allow.permits(leaf))).then(|| {
        deny(
            "command-outside-matrix",
            &format!(
                "{} may run only these omhl commands (plus -h/--help, -V/--version and <group> help): {}",
                agent_name(role),
                allow.commands.join(", ")
            ),
        )
    })
}

/// Lê `${CLAUDE_PLUGIN_ROOT}/agents/<papel>.md`; o papel já passou pela lista fechada, então não vira caminho solto.
fn read_role_allow(role: &str, plugin_root: Option<&Path>) -> Result<RoleAllow, String> {
    let root = plugin_root.ok_or("CLAUDE_PLUGIN_ROOT is not set")?;
    let path = root.join("agents").join(format!("{role}.md"));
    let agent_file = read_regular_utf8(&path, AGENT_FILE_MAX_BYTES)
        .map_err(|code| format!("cannot read the agent file of {role}: {code}"))?;

    parse_role_allow(&agent_file)
}

/// O `updatedInput` substitui o objeto inteiro, então leva `description`, `timeout` e o resto como vieram.
fn with_role_prefix(input: &Value, role: &str) -> Value {
    let mut tool_input = input["tool_input"].clone();
    if let Some(command) = input["tool_input"]["command"].as_str() {
        let (before, after) = command.split_at(prefix_position(command));
        tool_input["command"] = json!(format!(
            "{before}{ROLE_VARIABLE}={ROLE_PREFIX}{role} {after}"
        ));
    }

    tool_input
}

/// Onde o prefixo entra: na frente, salvo depois da palavra reservada `time` (e do `-p` e do `--`), que só vale no começo do comando.
/// Com o prefixo antes dela o bash chamaria o `/usr/bin/time`, que pode nem existir.
fn prefix_position(command: &str) -> usize {
    let Some(rest) = after_blank_word(command.trim_start_matches(BLANKS), "time") else {
        return 0;
    };
    let rest = ["-p", "--"].iter().fold(rest, |rest, option| {
        after_blank_word(rest, option).unwrap_or(rest)
    });

    command.len() - rest.len()
}

/// O que sobra depois da palavra `word` e dos espaços que a seguem; `None` se o texto não começa por ela.
fn after_blank_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    text.strip_prefix(word)
        .filter(|rest| rest.starts_with(BLANKS))
        .map(|rest| rest.trim_start_matches(BLANKS))
}

/// O nome do papel como o banco o grava no `agent` dos records.
fn agent_name(role: &str) -> String {
    format!("{ROLE_PREFIX}{role}")
}

fn deny(code: &'static str, reason: &str) -> RoleVerdict {
    RoleVerdict::Deny {
        code,
        reason: format!("{code}: {reason}"),
    }
}

#[cfg(test)]
mod tests;
