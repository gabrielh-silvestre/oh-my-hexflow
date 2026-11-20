use std::fs;
use std::path::PathBuf;

use super::{
    Cycle, agent_skills, check, cycle_prose, parse_cycle, prove_both_sides, session_checks,
};
use crate::workflow::role_allow::{RoleAllow, parse_role_allow};

const ROLES: [&str; 7] = [
    "analyst",
    "planner",
    "architect",
    "critic",
    "code-reviewer",
    "security-reviewer",
    "verifier",
];
const WORKFLOW_SKILLS: [&str; 3] = ["interview", "plan-consensus", "review"];
const QUALIFIED_SKILL: &str = "oh-my-hexflow:role-scribe";

const AGENT: &str = "---\nname: scribe\nomhl-allow:\n  commands: [append, list, get, help]\n  append:\n    nodes: [premise]\n    relations: [answers, contradicts]\n    retire: [contradicts]\n---\nbody\n";
const AGENT_WITHOUT_APPEND: &str =
    "---\nname: scribe\nomhl-allow:\n  commands: [list, get, help]\n---\nbody\n";

const CYCLE_MD: &str = r#"# Ciclo

1. **Work item.** A sessão grava.
2. **Scribe.** O `oh-my-hexflow:scribe` grava.
3. **Confirmação.** O `oh-my-hexflow:scribe` confirma e a sessão revê.

```yaml omhl-cycle
etapas:
  - {n: 1, nome: work-item, papeis: [sessao]}
  - {n: 2, nome: scribe, papeis: [scribe]}
  - {n: 3, nome: confirmacao, papeis: [scribe, sessao]}
```
"#;

const VIEW: &str = r#"role: scribe
etapas:
  - etapa: 2
    comandos:
      - "omhl list --type premise --where work_item=work-items.<id>"
      - "omhl get premises.<nome>"
      - "omhl append"
    grava: {nodes: [premise], relations: [answers, contradicts], retire: [contradicts]}
  - etapa: 3
    repete: 2
nao_usados: [help]
"#;

const MALFORMED_VIEW: &str = r#"role: other
etapas:
  - etapa: 2
    repete: 3
    comandos:
      - "list"
  - etapa: 3
  - etapa: 4
    comandos:
      - "omhl list --where kind=<kind>"
      - "omhl get a | b"
      - "omhl get a && b"
      - "omhl get a; b"
    grava: {nodes: [], relations: [], retire: []}
  - etapa: 5
    comandos:
      - "omhl append"
  - etapa: 6
    repete: 9
  - etapa: 10
    repete: 2
    grava: {nodes: [], relations: [], retire: []}
nao_usados: [help]
"#;

const VIEW_RECORDING_WITHOUT_APPEND_IN_ALLOW: &str = r#"role: scribe
etapas:
  - etapa: 2
    comandos:
      - "omhl append"
    grava: {nodes: [premise], relations: [], retire: []}
nao_usados: [list, get, help]
"#;

fn plugin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugin")
}

fn read_plugin(relative: &str) -> String {
    fs::read_to_string(plugin_dir().join(relative))
        .unwrap_or_else(|error| panic!("{relative} must exist and be readable: {error}"))
}

fn allow() -> RoleAllow {
    parse_role_allow(AGENT).expect("valid synthetic agent")
}

fn cycle() -> Cycle {
    parse_cycle(CYCLE_MD).expect("valid synthetic cycle")
}

fn skill_with(yaml: &str) -> String {
    format!("---\nname: role-scribe\n---\n\n```yaml omhl-role-view\n{yaml}```\n")
}

fn check_text(skill_text: &str) -> Vec<String> {
    check(
        "scribe",
        skill_text,
        &allow(),
        &cycle(),
        Some(&[QUALIFIED_SKILL.to_owned()]),
        &["role-scribe"],
    )
}

fn findings(yaml: &str) -> Vec<String> {
    check_text(&skill_with(yaml))
}

/// Confere que `VIEW` passa e que `mutated` não, e devolve as linhas que `mutated` provoca.
fn rejected(mutated: &str) -> Vec<String> {
    let failures = prove_both_sides(|yaml| Ok(findings(yaml).len()), &[mutated], &[VIEW]);
    assert!(failures.is_empty(), "{failures:?}");

    findings(mutated)
}

fn skill_findings(agent_skills: Option<&[String]>, skill_dirs: &[&str]) -> Vec<String> {
    check(
        "scribe",
        &skill_with(VIEW),
        &allow(),
        &cycle(),
        agent_skills,
        skill_dirs,
    )
}

fn skill_dirs_with_a_file() -> Vec<String> {
    let mut dirs: Vec<String> = fs::read_dir(plugin_dir().join("skills"))
        .expect("plugin/skills must exist")
        .flatten()
        .filter(|entry| entry.path().join("SKILL.md").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    dirs.sort();

    dirs
}

#[test]
fn it_should_accept_the_shipped_role_views() {
    let cycle = parse_cycle(&read_plugin("skills/directives-flow/references/cycle.md"))
        .expect("the shipped cycle block");
    let dirs = skill_dirs_with_a_file();
    let dir_refs: Vec<&str> = dirs.iter().map(String::as_str).collect();

    for role in ROLES {
        let agent_file = read_plugin(&format!("agents/{role}.md"));
        let allow = parse_role_allow(&agent_file).expect("the shipped omhl-allow");
        let skills = agent_skills(&agent_file).expect("the shipped frontmatter");

        let found = check(
            role,
            &read_plugin(&format!("skills/role-{role}/SKILL.md")),
            &allow,
            &cycle,
            skills.as_deref(),
            &dir_refs,
        );

        assert_eq!(found, Vec::<String>::new(), "{role}");
    }
}

#[test]
fn it_should_accept_the_shipped_cycle_prose() {
    let cycle_md = read_plugin("skills/directives-flow/references/cycle.md");
    let cycle = parse_cycle(&cycle_md).expect("the shipped cycle block");

    let found = cycle_prose(&cycle, &cycle_md);

    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn it_should_accept_the_session_checks_of_the_shipped_workflows() {
    for skill in WORKFLOW_SKILLS {
        let found = session_checks(&read_plugin(&format!("skills/{skill}/SKILL.md")));

        assert_eq!(found, Vec::<String>::new(), "{skill}");
    }
}

#[test]
fn it_should_list_the_stages_a_role_view_lacks() {
    let found = rejected(&VIEW.replace("  - etapa: 3\n    repete: 2\n", ""));

    assert_eq!(found, ["etapas sem linha: [3]"]);
}

#[test]
fn it_should_reject_a_stage_the_role_does_not_act_in() {
    let found = rejected(&VIEW.replace("nao_usados", "  - etapa: 1\n    repete: 2\nnao_usados"));

    assert_eq!(found, ["etapas fora do papel: [1]"]);
}

#[test]
fn it_should_reject_a_command_outside_the_omhl_allow() {
    let found = rejected(&VIEW.replace(
        "      - \"omhl append\"\n",
        "      - \"omhl append\"\n      - \"omhl render\"\n",
    ));

    assert_eq!(found, ["comandos fora do omhl-allow: [render]"]);
}

#[test]
fn it_should_reject_an_unused_entry_outside_the_omhl_allow() {
    let found = rejected(&VIEW.replace("nao_usados: [help]", "nao_usados: [help, sweep]"));

    assert_eq!(found, ["nao_usados fora do omhl-allow: [sweep]"]);
}

#[test]
fn it_should_sort_the_commands_outside_the_omhl_allow_together_with_the_unresolved_ones() {
    let found = rejected(&VIEW.replace(
        "      - \"omhl append\"\n",
        "      - \"omhl append\"\n      - \"omhl render\"\n      - \"omhl --help\"\n",
    ));

    assert_eq!(
        found,
        ["comandos fora do omhl-allow: [omhl --help, render]"]
    );
}

#[test]
fn it_should_reject_an_omhl_allow_command_with_no_line() {
    let found = rejected(&VIEW.replace("nao_usados: [help]", "nao_usados: []"));

    assert_eq!(found, ["comandos do omhl-allow sem linha: [help]"]);
}

#[test]
fn it_should_reject_a_command_both_used_and_unused() {
    let found = rejected(&VIEW.replace("nao_usados: [help]", "nao_usados: [help, list]"));

    assert_eq!(found, ["comandos do omhl-allow sem linha: [list]"]);
}

#[test]
fn it_should_reject_a_recorded_item_no_stage_declares() {
    let found = rejected(&VIEW.replace(
        "relations: [answers, contradicts]",
        "relations: [contradicts]",
    ));

    assert_eq!(found, ["grava sem linha: [relação:answers]"]);
}

#[test]
fn it_should_tell_a_retired_item_from_a_recorded_relation_of_the_same_name() {
    let found = rejected(&VIEW.replace("retire: [contradicts]", "retire: []"));

    assert_eq!(found, ["grava sem linha: [retirada:contradicts]"]);
}

#[test]
fn it_should_reject_a_recorded_item_outside_the_omhl_allow() {
    let found = rejected(&VIEW.replace("nodes: [premise]", "nodes: [premise, gap]"));

    assert_eq!(found, ["grava fora do omhl-allow: [nó:gap]"]);
}

#[test]
fn it_should_reject_a_malformed_role_view_block() {
    let found = findings(MALFORMED_VIEW);

    assert_eq!(
        found,
        [
            "forma do bloco: [role other ≠ scribe]",
            "forma do bloco: [etapa 2: comando sem o prefixo omhl: list]",
            "forma do bloco: [etapa 2: repete com comandos]",
            "forma do bloco: [etapa 3: sem repete e sem comandos]",
            "forma do bloco: [etapa 4: grava sem omhl append nos comandos]",
            "forma do bloco: [etapa 4: operador && em omhl get a && b]",
            "forma do bloco: [etapa 4: operador ; em omhl get a; b]",
            "forma do bloco: [etapa 4: operador | em omhl get a | b]",
            "forma do bloco: [etapa 4: placeholder <kind> fora de <id>, <nome> e <slug>]",
            "forma do bloco: [etapa 5: omhl append sem grava]",
            "forma do bloco: [etapa 6: repete a etapa 9, que o papel não tem]",
            "forma do bloco: [etapa 10: repete com grava]",
        ]
    );
}

#[test]
fn it_should_reject_a_stage_declared_twice_or_repeating_itself() {
    let found = findings(&VIEW.replace(
        "  - etapa: 3\n    repete: 2\n",
        "  - etapa: 3\n    repete: 3\n  - etapa: 2\n    repete: 3\n",
    ));

    assert_eq!(
        found,
        [
            "forma do bloco: [etapa 2: etapa repetida]",
            "forma do bloco: [etapa 3: repete a própria etapa]",
        ]
    );
}

#[test]
fn it_should_reject_a_recorded_item_when_the_omhl_allow_has_no_append() {
    let allow = parse_role_allow(AGENT_WITHOUT_APPEND).expect("valid synthetic agent");

    let found = check(
        "scribe",
        &skill_with(VIEW_RECORDING_WITHOUT_APPEND_IN_ALLOW),
        &allow,
        &cycle(),
        Some(&[QUALIFIED_SKILL.to_owned()]),
        &["role-scribe"],
    );

    assert_eq!(
        found,
        ["forma do bloco: [etapa 2: grava, mas o omhl-allow não tem append]"]
    );
}

#[test]
fn it_should_reject_a_role_view_file_without_exactly_one_block() {
    let without_block = "---\nname: role-scribe\n---\n\nsem cerca\n";
    let two_blocks = format!("{}{}", skill_with(VIEW), skill_with(VIEW));
    let unknown_field = skill_with(&format!("{VIEW}extra: 1\n"));

    let none = check_text(without_block);
    let twice = check_text(&two_blocks);
    let unknown = check_text(&unknown_field);

    assert_eq!(
        none,
        ["forma do bloco: [cercas omhl-role-view: esperada 1, achadas 0]"]
    );
    assert_eq!(
        twice,
        ["forma do bloco: [cercas omhl-role-view: esperada 1, achadas 2]"]
    );
    assert_eq!(unknown.len(), 1, "{unknown:?}");
    assert!(
        unknown[0].starts_with("forma do bloco: [yaml: ") && unknown[0].contains("extra"),
        "{unknown:?}"
    );
}

#[test]
fn it_should_reject_a_skills_list_other_than_the_role_view() {
    let bare = ["role-scribe".to_owned()];

    let spelled_bare = skill_findings(Some(&bare), &["role-scribe"]);
    let absent = skill_findings(None, &["role-scribe"]);

    assert_eq!(
        spelled_bare,
        ["skills do agente: [esperado oh-my-hexflow:role-scribe, achado [\"role-scribe\"]]"]
    );
    assert_eq!(
        absent,
        ["skills do agente: [esperado oh-my-hexflow:role-scribe, achado ausente]"]
    );
}

#[test]
fn it_should_reject_an_empty_or_a_multiple_skills_list() {
    let none: [String; 0] = [];
    let two = [
        QUALIFIED_SKILL.to_owned(),
        "oh-my-hexflow:role-other".to_owned(),
    ];

    let empty = skill_findings(Some(&none), &["role-scribe"]);
    let multiple = skill_findings(Some(&two), &["role-scribe", "role-other"]);

    assert_eq!(
        empty,
        ["skills do agente: [esperado oh-my-hexflow:role-scribe, achado []]"]
    );
    assert_eq!(
        multiple,
        [
            "skills do agente: [esperado oh-my-hexflow:role-scribe, achado [\"oh-my-hexflow:role-scribe\", \"oh-my-hexflow:role-other\"]]"
        ]
    );
}

#[test]
fn it_should_reject_a_cited_skill_without_a_skill_file() {
    let skills = [QUALIFIED_SKILL.to_owned()];

    let found = skill_findings(Some(&skills), &[]);

    assert_eq!(
        found,
        ["skill do agente sem SKILL.md: [oh-my-hexflow:role-scribe]"]
    );
}

#[test]
fn it_should_read_the_cycle_key_n_but_reject_an_unquoted_number_or_boolean_as_text() {
    let number_as_name = CYCLE_MD.replace("nome: scribe", "nome: 2");
    let boolean_as_role = CYCLE_MD.replace("papeis: [scribe]", "papeis: [true]");

    assert!(parse_cycle(CYCLE_MD).is_ok());
    assert!(parse_cycle(&number_as_name).is_err());
    assert!(parse_cycle(&boolean_as_role).is_err());
}

#[test]
fn it_should_reject_a_cycle_role_the_prose_does_not_name() {
    let prose = CYCLE_MD.replace("O `oh-my-hexflow:scribe` grava.", "O agente grava.");

    let found = cycle_prose(&cycle(), &prose);

    assert_eq!(found, ["papeis do ciclo: [2: papeis [] ≠ [scribe]]"]);
}

#[test]
fn it_should_reject_a_cycle_number_or_name_the_prose_does_not_have() {
    let prose = CYCLE_MD
        .replace("3. **Confirmação.**", "4. **Confirmação.**")
        .replace("2. **Scribe.**", "2. **Escriba.**");

    let found = cycle_prose(&cycle(), &prose);

    assert_eq!(
        found,
        ["papeis do ciclo: [2: nome escriba ≠ scribe, 3: sem linha numerada]"]
    );
}

#[test]
fn it_should_reject_a_workflow_step_without_a_session_check() {
    let skill = "---\nname: x\n---\n# X\n\n1. Rode. A sessão confere com `omhl get work-items.<id>`.\n2. Chame o agente.\n";

    let found = session_checks(skill);

    assert_eq!(
        found,
        ["passos sem conferência: [2: sem a frase \"A sessão confere\", 2: sem comando omhl]"]
    );
}

#[test]
fn it_should_reject_a_workflow_skill_whose_steps_cannot_be_read() {
    let without_frontmatter = "# X\n\n1. Rode. A sessão confere com `omhl get work-items.<id>`.\n";

    let found = session_checks(without_frontmatter);

    assert_eq!(
        found,
        ["passos sem conferência: [sem passo numerado após o frontmatter]"]
    );
}

#[test]
fn it_should_reject_a_session_check_that_cites_a_leaf_the_cli_lacks() {
    let skill =
        "---\nname: x\n---\n# X\n\n1. Rode. A sessão confere com `omhl frobnicate --now`.\n";

    let found = session_checks(skill);

    assert_eq!(
        found,
        ["passos sem conferência: [1: folha inexistente frobnicate]"]
    );
}
