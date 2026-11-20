use std::path::Path;

use serde_json::{Value, json};

use super::{AgentRoleEnv, RoleVerdict, agent_role};

const GROUPS: [&str; 2] = ["spec", "report"];
const PORTED_AGENT: &str = "oh-my-hexflow:verifier";
const ATTEST: &str = "omhl attest checks.run-tests work-items.w1 --outcome pass";
/// Os falsos positivos que a S5 libera: o nome nu só é argumento de `grep` ou do `-p` de `declare`/`typeset`.
const RELEASED_FALSE_POSITIVES: [&str; 3] = [
    "env -i grep OMHL_ROLE file",
    "declare -p OMHL_ROLE",
    "typeset -p OMHL_ROLE",
];

/// O que o corpus afirma de cada veredito: o `Allow` e o `Deny` sem o texto da razão.
#[derive(Debug, PartialEq)]
enum Expected {
    Pass,
    Allow,
    Deny(&'static str),
}

/// Um comando do corpus com o veredito de cada origem: a sessão principal (sem `agent_type`) e o agente portado.
struct CorpusEntry {
    command: &'static str,
    main_session: Expected,
    ported_agent: Expected,
}

fn corpus_entry(
    command: &'static str,
    main_session: Expected,
    ported_agent: Expected,
) -> CorpusEntry {
    CorpusEntry {
        command,
        main_session,
        ported_agent,
    }
}

/// Negado com `role-assignment` nas duas origens.
fn denied_from_both_origins(command: &'static str) -> CorpusEntry {
    corpus_entry(
        command,
        Expected::Deny("role-assignment"),
        Expected::Deny("role-assignment"),
    )
}

/// Negado com `role-unset` nas duas origens.
fn unset_denied_from_both_origins(command: &'static str) -> CorpusEntry {
    corpus_entry(
        command,
        Expected::Deny("role-unset"),
        Expected::Deny("role-unset"),
    )
}

/// Liberado nas duas origens.
fn passed_from_both_origins(command: &'static str) -> CorpusEntry {
    corpus_entry(command, Expected::Pass, Expected::Pass)
}

/// A raiz de fixtures tem `agents/<papel>.md` legível para os sete papéis: a negação vem da regra mirada.
fn plugin_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

fn tool_call(command: &str, agent_type: Option<&str>) -> Value {
    let mut event = json!({ "tool_input": { "command": command } });
    if let Some(agent_type) = agent_type {
        event["agent_type"] = json!(agent_type);
    }

    event
}

fn verdict_of(command: &str, agent_type: Option<&str>) -> RoleVerdict {
    let env = AgentRoleEnv {
        plugin_root: Some(plugin_root()),
        groups: &GROUPS,
    };

    agent_role(&tool_call(command, agent_type), &env)
}

fn ported_verdict_of(command: &str) -> RoleVerdict {
    verdict_of(command, Some(PORTED_AGENT))
}

fn assert_verdict_denies_with_code(verdict: RoleVerdict, code: &str) {
    match verdict {
        RoleVerdict::Deny {
            code: actual,
            reason,
        } => {
            assert_eq!(actual, code);
            assert!(
                reason.starts_with(&format!("{code}: ")),
                "the reason starts with `{code}: `: {reason}"
            );
        }
        other => panic!("expected a deny with code {code}, got {other:?}"),
    }
}

fn expected_of(verdict: &RoleVerdict) -> Expected {
    match verdict {
        RoleVerdict::Pass => Expected::Pass,
        RoleVerdict::Allow { .. } => Expected::Allow,
        RoleVerdict::Deny { code, .. } => Expected::Deny(code),
    }
}

// Regras do hook, uma por teste

#[test]
fn it_should_deny_role_assignment_in_the_prefix() {
    let verdict = verdict_of("OMHL_ROLE=x omhl list", None);

    assert_verdict_denies_with_code(verdict, "role-assignment");
}

#[test]
fn it_should_deny_env_unset_with_its_own_code() {
    let commands = [
        "env -u OMHL_ROLE omhl list",
        "env -uOMHL_ROLE omhl list",
        "env --unset=OMHL_ROLE omhl list",
        "env --unset OMHL_ROLE omhl list",
    ];

    for command in commands {
        assert_verdict_denies_with_code(verdict_of(command, None), "role-unset");
    }
}

#[test]
fn it_should_read_the_value_of_u_like_getopt() {
    let commands = [
        "env -iu OMHL_ROLE omhl list",
        "env -iuOMHL_ROLE omhl list",
        "env -i -u OMHL_ROLE omhl list",
        "env --uns=OMHL_ROLE omhl list",
        "env --un OMHL_ROLE omhl list",
    ];

    for command in commands {
        assert_verdict_denies_with_code(verdict_of(command, None), "role-unset");
        assert_verdict_denies_with_code(ported_verdict_of(command), "role-unset");
    }
}

#[test]
fn it_should_not_deny_env_unset_of_another_variable() {
    let verdict = ported_verdict_of("env -u HOME omhl list");

    assert_eq!(
        expected_of(&verdict),
        Expected::Allow,
        "the unset value is `HOME`: {verdict:?}"
    );
}

#[test]
fn it_should_not_deny_a_bare_role_name_as_an_argument_of_grep() {
    let command = "env -i grep OMHL_ROLE file";

    assert_eq!(verdict_of(command, None), RoleVerdict::Pass);
    assert_eq!(ported_verdict_of(command), RoleVerdict::Pass);
}

#[test]
fn it_should_not_deny_declare_or_typeset_p_of_the_bare_role_name() {
    for command in ["declare -p OMHL_ROLE", "typeset -p OMHL_ROLE"] {
        assert_eq!(verdict_of(command, None), RoleVerdict::Pass, "{command}");
        assert_eq!(ported_verdict_of(command), RoleVerdict::Pass, "{command}");
    }
}

#[test]
fn it_should_deny_declare_with_p_after_the_double_dash() {
    let commands = [
        "declare -- -p OMHL_ROLE=x",
        "declare -- -p OMHL_ROLE",
        "typeset -- -p OMHL_ROLE",
    ];

    for command in commands {
        assert_verdict_denies_with_code(verdict_of(command, None), "role-assignment");
    }
}

#[test]
fn it_should_deny_a_p_that_comes_after_a_name() {
    let verdict = verdict_of("declare -i foo -p OMHL_ROLE", None);

    assert_verdict_denies_with_code(verdict, "role-assignment");
}

#[test]
fn it_should_deny_heredoc_substitution() {
    let verdict = ported_verdict_of("omhl append <<EOF\n$(id)\nEOF\n");

    assert_verdict_denies_with_code(verdict, "heredoc-substitution");
}

#[test]
fn it_should_deny_command_unreadable_when_the_command_mentions_omhl() {
    let verdict = ported_verdict_of("omhl list \"unclosed");

    assert_verdict_denies_with_code(verdict, "command-unreadable");
}

#[test]
fn it_should_deny_a_compound_command() {
    let verdict = ported_verdict_of("omhl list | head");

    assert_verdict_denies_with_code(verdict, "compound-command");
}

#[test]
fn it_should_deny_a_command_outside_the_matrix() {
    let verdict = ported_verdict_of(ATTEST);

    assert_verdict_denies_with_code(verdict, "command-outside-matrix");
}

#[test]
fn it_should_name_every_read_only_form_in_the_outside_matrix_message() {
    let RoleVerdict::Deny { reason, .. } = ported_verdict_of(ATTEST) else {
        panic!("expected the command outside the matrix to be denied");
    };

    for form in ["-h/--help", "-V/--version", "<group> help"] {
        assert!(reason.contains(form), "the reason names `{form}`: {reason}");
    }
}

#[test]
fn it_should_allow_with_the_role_prefix() {
    let verdict = ported_verdict_of("omhl list");

    assert_eq!(
        verdict,
        RoleVerdict::Allow {
            reason: "the command is inside the omhl:verifier matrix".to_owned(),
            updated_input: json!({ "command": "OMHL_ROLE=omhl:verifier omhl list" }),
        }
    );
}

#[test]
fn it_should_put_the_prefix_after_time_and_its_options() {
    let verdict = ported_verdict_of("time -p omhl list");

    assert_eq!(
        verdict,
        RoleVerdict::Allow {
            reason: "the command is inside the omhl:verifier matrix".to_owned(),
            updated_input: json!({ "command": "time -p OMHL_ROLE=omhl:verifier omhl list" }),
        }
    );
}

#[test]
fn it_should_keep_denying_a_bare_reserved_word_with_role_prefix_unreachable() {
    for keyword in ["!", "if", "then", "elif", "else", "while", "until", "do"] {
        let verdict = ported_verdict_of(&format!("{keyword} omhl list"));

        assert_verdict_denies_with_code(verdict, "role-prefix-unreachable");
    }
}

#[test]
fn it_should_put_the_prefix_before_an_assignment_whose_value_ends_in_a_keyword_name() {
    let verdict = ported_verdict_of("FOO=/srv/do omhl list");

    assert_eq!(
        verdict,
        RoleVerdict::Allow {
            reason: "the command is inside the omhl:verifier matrix".to_owned(),
            updated_input: json!({ "command": "OMHL_ROLE=omhl:verifier FOO=/srv/do omhl list" }),
        }
    );
}

#[test]
fn it_should_carry_the_code_of_each_denial() {
    let without_plugin_root = AgentRoleEnv {
        plugin_root: None,
        groups: &GROUPS,
    };
    let denials = [
        ("role-assignment", verdict_of("OMHL_ROLE=x omhl list", None)),
        ("role-unset", verdict_of("env -u OMHL_ROLE omhl list", None)),
        (
            "heredoc-substitution",
            ported_verdict_of("omhl append <<EOF\n$(id)\nEOF\n"),
        ),
        (
            "command-unreadable",
            ported_verdict_of("omhl list \"unclosed"),
        ),
        (
            "role-unknown",
            verdict_of("omhl list", Some("oh-my-hexflow:janitor")),
        ),
        ("wrapper-unreadable", ported_verdict_of("env -i omhl list")),
        ("compound-command", ported_verdict_of("omhl list | head")),
        ("command-outside-matrix", ported_verdict_of(ATTEST)),
        ("role-prefix-unreachable", ported_verdict_of("! omhl list")),
        (
            "role-matrix-unreadable",
            agent_role(
                &tool_call("omhl list", Some(PORTED_AGENT)),
                &without_plugin_root,
            ),
        ),
    ];

    for (code, verdict) in denials {
        assert_verdict_denies_with_code(verdict, code);
    }
}

// Corpus diferencial da regra de `OMHL_ROLE`

/// Os comandos negados com `role-assignment` nas duas origens, os negados com `role-unset`
/// e os falsos positivos que a S5 libera.
fn role_variable_corpus() -> Vec<CorpusEntry> {
    let mut corpus: Vec<CorpusEntry> = [
        "OMHL_ROLE=x omhl list",
        "OMHL_ROLE+=x omhl list",
        "export OMHL_ROLE=x",
        "export OMHL_ROLE",
        "export -p OMHL_ROLE=x",
        "readonly OMHL_ROLE",
        "readonly -p OMHL_ROLE=x",
        "local OMHL_ROLE=x",
        "declare -x OMHL_ROLE=x",
        "declare OMHL_ROLE=x",
        "declare -p OMHL_ROLE=x",
        "declare -- -p OMHL_ROLE=x",
        "typeset -x OMHL_ROLE=x",
        "env OMHL_ROLE=x omhl list",
        "env -i OMHL_ROLE=x omhl list",
    ]
    .into_iter()
    .map(denied_from_both_origins)
    .collect();

    // Tiram o papel com `env -u`/`--unset`: o código é `role-unset`, não mais `role-assignment`.
    corpus.extend(
        [
            "env -i -u OMHL_ROLE omhl list",
            "env -iu OMHL_ROLE omhl list",
            "env -u OMHL_ROLE",
            "env -u OMHL_ROLE omhl list",
            "env -uOMHL_ROLE",
            "env -uOMHL_ROLE omhl list",
            "env --unset=OMHL_ROLE",
            "env --unset=OMHL_ROLE omhl list",
            "env --unset OMHL_ROLE",
            "env --unset OMHL_ROLE omhl list",
            "env --uns OMHL_ROLE omhl list",
            // Endurecimento da S5: o getopt lê o valor de `-u` no cluster `-iu...` e na abreviação `--uns=`;
            // antes escapavam da regra na sessão principal e o agente portado era negado por opacidade.
            "env -iuOMHL_ROLE omhl list",
            "env --uns=OMHL_ROLE omhl list",
        ]
        .into_iter()
        .map(unset_denied_from_both_origins),
    );

    corpus.extend(
        RELEASED_FALSE_POSITIVES
            .into_iter()
            .map(passed_from_both_origins),
    );

    corpus
}

#[test]
fn it_should_keep_the_current_denials_of_the_role_variable_corpus() {
    let mismatches: Vec<String> = role_variable_corpus()
        .into_iter()
        .flat_map(|entry| {
            [
                ("main session", None, entry.main_session),
                ("ported agent", Some(PORTED_AGENT), entry.ported_agent),
            ]
            .into_iter()
            .filter_map(move |(origin, agent_type, expected)| {
                let actual = expected_of(&verdict_of(entry.command, agent_type));

                (actual != expected).then(|| {
                    format!(
                        "`{}` from the {origin}: expected {expected:?}, got {actual:?}",
                        entry.command
                    )
                })
            })
        })
        .collect();

    assert!(
        mismatches.is_empty(),
        "verdicts changed:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn it_should_deny_every_command_of_the_differential_corpus_except_the_approved() {
    let wrongly_decided: Vec<String> = role_variable_corpus()
        .into_iter()
        .flat_map(|entry| {
            [("main session", None), ("ported agent", Some(PORTED_AGENT))]
                .into_iter()
                .filter_map(move |(origin, agent_type)| {
                    let verdict = verdict_of(entry.command, agent_type);
                    let denied = matches!(verdict, RoleVerdict::Deny { .. });
                    let approved = RELEASED_FALSE_POSITIVES.contains(&entry.command);

                    (denied == approved).then(|| {
                    format!(
                        "`{}` from the {origin}: approved to pass = {approved}, got {verdict:?}",
                        entry.command
                    )
                })
                })
        })
        .collect();

    assert!(
        wrongly_decided.is_empty(),
        "wrong verdicts:\n{}",
        wrongly_decided.join("\n")
    );
}
