use serde_json::Value;

use crate::guard::known_role;
use crate::workflow::lock::Verdict;
use crate::workflow::shell::{CommandLine, SimpleCommand, read_command};

/// O `agent_type` real dos subagentes do plugin é `oh-my-hexflow:<papel>`.
const PLUGIN_PREFIX: &str = "oh-my-hexflow:";
/// O papel gravado em `OMHL_ROLE` e no `agent` dos records é `omhl:<papel>`.
const ROLE_PREFIX: &str = "omhl:";
const ROLE_VARIABLE: &str = "OMHL_ROLE";
const ASSIGNING_PROGRAMS: [&str; 6] = ["export", "env", "declare", "typeset", "readonly", "local"];

/// O hook `agent-role` para o evento `PreToolUse` do Bash; não abre o banco nem lê arquivo.
/// Regras 2 a 6 do plano, na ordem: o que se decide antes de ler a matriz do agente.
pub fn agent_role(input: &Value) -> Verdict {
    let Some(command) = input["tool_input"]["command"].as_str() else {
        return Verdict::Allow;
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
        return Verdict::Allow;
    };

    match line {
        Ok(line) => ported_agent_verdict(role, &line),
        // O erro de leitura só pesa para o agente portado, e só se o comando fala de `omhl`.
        Err(_) if command.contains("omhl") => deny(
            "command-unreadable",
            "the command has an unclosed quote or heredoc and mentions omhl; fix the command",
        ),
        Err(_) => Verdict::Allow,
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
fn ported_agent_verdict(role: &str, line: &CommandLine) -> Verdict {
    let reaching: Vec<&SimpleCommand> = line
        .commands
        .iter()
        .filter(|simple| simple.may_run_omhl())
        .collect();
    if reaching.is_empty() {
        return Verdict::Allow;
    }
    if known_role(&format!("{ROLE_PREFIX}{role}")).is_none() {
        return deny(
            "role-unknown",
            "this plugin agent has no omhl role matrix; it cannot run omhl",
        );
    }

    match reaching.as_slice() {
        [simple] if !line.compound && simple.runs_omhl() => role_matrix_verdict(),
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

/// Regras 7 a 10: a matriz `omhl-allow` do papel decide o único comando `omhl`; sem ela, passa intacto.
fn role_matrix_verdict() -> Verdict {
    Verdict::Allow
}

fn deny(code: &'static str, reason: &str) -> Verdict {
    Verdict::Deny {
        code,
        reason: reason.to_owned(),
    }
}
