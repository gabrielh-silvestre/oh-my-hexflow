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

/// O hook `agent-role` para o evento `PreToolUse` do Bash; não abre o banco, só lê o arquivo do agente.
pub fn agent_role(input: &Value, env: &AgentRoleEnv<'_>) -> RoleVerdict {
    let Some(command) = input["tool_input"]["command"].as_str() else {
        return RoleVerdict::Pass;
    };
    let line = read_command(command);
    if line.as_ref().is_ok_and(assigns_role_variable) {
        return deny(
            "role-assignment",
            "OMHL_ROLE is set by the plugin hook only; do not assign it in a command",
        );
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

/// Atribuição de `OMHL_ROLE` como prefixo (também depois de embrulho) ou como argumento de `export` e afins.
fn assigns_role_variable(line: &CommandLine) -> bool {
    line.commands.iter().any(|simple| {
        let (prefix, program, args) = simple.split();
        let assigns_in_prefix = prefix.iter().any(|word| names_role_variable(word));
        let assigns_by_builtin = program
            .is_some_and(|program| ASSIGNING_PROGRAMS.contains(&program))
            && args.iter().any(|word| names_role_variable(word));

        assigns_in_prefix || assigns_by_builtin
    })
}

/// `OMHL_ROLE`, `OMHL_ROLE=`, `OMHL_ROLE+=` e o nome atrás de `env -u` / `env --unset=` (inclusive grudado).
fn names_role_variable(word: &str) -> bool {
    let name = word
        .strip_prefix("--unset=")
        .or_else(|| word.strip_prefix("-u"))
        .unwrap_or(word);

    name.strip_prefix(ROLE_VARIABLE)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('=') || rest.starts_with("+="))
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
                "{} may run only these omhl commands (plus --help, -V and help): {}",
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
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests;
