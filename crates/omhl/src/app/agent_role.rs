use std::path::Path;

use serde_json::{Value, json};

use crate::guard::known_role;
use crate::ports::files::read_regular_utf8;
use crate::workflow::omhl_leaf::resolve_leaf;
use crate::workflow::role_allow::{RoleAllow, parse_role_allow};
use crate::workflow::shell::{CommandLine, SimpleCommand, read_command};

/// O `agent_type` real dos subagentes do plugin é `oh-my-hexflow:<papel>`.
const PLUGIN_PREFIX: &str = "oh-my-hexflow:";
/// O papel gravado em `OMHL_ROLE` e no `agent` dos records é `omhl:<papel>`.
const ROLE_PREFIX: &str = "omhl:";
const ROLE_VARIABLE: &str = "OMHL_ROLE";
const ASSIGNING_PROGRAMS: [&str; 6] = ["export", "env", "declare", "typeset", "readonly", "local"];
const AGENT_FILE_MAX_BYTES: u64 = 64 * 1024;

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
/// Regras 2 a 10 do plano, na ordem.
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
        Ok(line) => ported_agent_verdict(role, &line, input, env),
        // O erro de leitura só pesa para o agente portado, e só se o comando fala de `omhl`.
        Err(_) if command.contains("omhl") => deny(
            "command-unreadable",
            "the command has an unclosed quote or heredoc and mentions omhl; fix the command",
        ),
        Err(_) => RoleVerdict::Pass,
    }
}

/// Atribuição de `OMHL_ROLE` como prefixo (também depois de embrulho) ou como argumento de `export` e afins.
fn assigns_role_variable(line: &CommandLine) -> bool {
    line.commands.iter().any(|simple| {
        let assigns_in_prefix = simple.prefix().iter().any(|word| names_role_variable(word));
        let assigns_by_builtin = simple
            .program()
            .is_some_and(|program| ASSIGNING_PROGRAMS.contains(&program))
            && simple.args().iter().any(|word| names_role_variable(word));

        assigns_in_prefix || assigns_by_builtin
    })
}

fn names_role_variable(word: &str) -> bool {
    word.strip_prefix(ROLE_VARIABLE)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
}

/// Regras 4 a 6: papel fora da lista, comando sem `omhl`, comando composto e embrulho que o leitor não pula.
fn ported_agent_verdict(
    role: &str,
    line: &CommandLine,
    input: &Value,
    env: &AgentRoleEnv<'_>,
) -> RoleVerdict {
    let reaching: Vec<&SimpleCommand> = line
        .commands
        .iter()
        .filter(|simple| simple.may_run_omhl())
        .collect();
    if reaching.is_empty() {
        return RoleVerdict::Pass;
    }
    if known_role(&format!("{ROLE_PREFIX}{role}")).is_none() {
        return deny(
            "role-unknown",
            "this plugin agent has no omhl role matrix; it cannot run omhl",
        );
    }

    match reaching.as_slice() {
        [simple] if !line.compound && simple.runs_omhl() => {
            role_matrix_verdict(role, simple, input, env)
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

/// Regras 7 a 10: a matriz `omhl-allow` do papel decide o único comando `omhl`.
fn role_matrix_verdict(
    role: &str,
    simple: &SimpleCommand,
    input: &Value,
    env: &AgentRoleEnv<'_>,
) -> RoleVerdict {
    let allow = match read_role_allow(role, env.plugin_root) {
        Ok(allow) => allow,
        Err(problem) => {
            return deny(
                "role-matrix-unreadable",
                &format!("a ported role without a valid omhl-allow runs no omhl: {problem}"),
            );
        }
    };
    let leaf = resolve_leaf(simple.args(), env.groups);
    if !leaf.as_ref().is_some_and(|leaf| allow.permits(leaf)) {
        return deny(
            "command-outside-matrix",
            &format!(
                "omhl:{role} may run only these omhl commands (plus --help, -V and help): {}",
                allow.commands.join(", ")
            ),
        );
    }

    RoleVerdict::Allow {
        reason: format!("the command is inside the omhl:{role} matrix"),
        updated_input: with_role_prefix(input, role),
    }
}

/// Regra 8: `${CLAUDE_PLUGIN_ROOT}/agents/<papel>.md`; o papel já passou pela lista fechada, então não vira caminho solto.
fn read_role_allow(role: &str, plugin_root: Option<&Path>) -> Result<RoleAllow, String> {
    let root = plugin_root.ok_or("CLAUDE_PLUGIN_ROOT is not set")?;
    let path = root.join("agents").join(format!("{role}.md"));
    let agent_file = read_regular_utf8(&path, AGENT_FILE_MAX_BYTES)
        .map_err(|code| format!("cannot read the agent file of {role}: {code}"))?;

    parse_role_allow(&agent_file)
}

/// Regra 9: o `updatedInput` substitui o objeto inteiro, então leva `description`, `timeout` e o resto como vieram.
fn with_role_prefix(input: &Value, role: &str) -> Value {
    let mut tool_input = input["tool_input"].clone();
    if let Some(command) = input["tool_input"]["command"].as_str() {
        tool_input["command"] = json!(format!("{ROLE_VARIABLE}={ROLE_PREFIX}{role} {command}"));
    }

    tool_input
}

fn deny(code: &'static str, reason: &str) -> RoleVerdict {
    RoleVerdict::Deny {
        code,
        reason: reason.to_owned(),
    }
}
