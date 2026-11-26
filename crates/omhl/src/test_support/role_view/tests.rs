use std::fs;
use std::path::PathBuf;

use super::phrase_markers::{MARKERS, Marker, phrase_markers};
use super::who_uses::{BASE_LEAVES, WHO_USES_ANCHOR, WhoUsesClass, anchor_findings, who_uses};
use super::{
    Cycle, agent_skills, check, cli_leaves, cycle_prose, parse_cycle, prove_both_sides,
    session_checks,
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

const CYCLE_MD: &str = r#"# Cycle

1. **Work item.** The session records.
2. **Scribe.** The `oh-my-hexflow:scribe` records.
3. **Confirmation.** The `oh-my-hexflow:scribe` confirms and the session reviews.

```yaml omhl-cycle
stages:
  - {n: 1, name: work-item, roles: [session]}
  - {n: 2, name: scribe, roles: [scribe]}
  - {n: 3, name: confirmation, roles: [scribe, session]}
```
"#;

const VIEW: &str = r#"role: scribe
stages:
  - stage: 2
    commands:
      - "omhl list --type premise --where work_item=work-items.<id>"
      - "omhl get premises.<name>"
      - "omhl append"
    records: {nodes: [premise], relations: [answers, contradicts], retire: [contradicts]}
  - stage: 3
    repeats: 2
unused: [help]
"#;

const MALFORMED_VIEW: &str = r#"role: other
stages:
  - stage: 2
    repeats: 3
    commands:
      - "list"
  - stage: 3
  - stage: 4
    commands:
      - "omhl list --where kind=<kind>"
      - "omhl get a | b"
      - "omhl get a && b"
      - "omhl get a; b"
    records: {nodes: [], relations: [], retire: []}
  - stage: 5
    commands:
      - "omhl append"
  - stage: 6
    repeats: 9
  - stage: 10
    repeats: 2
    records: {nodes: [], relations: [], retire: []}
unused: [help]
"#;

const WORKFLOW_SKILL: &str = "---\nname: plan-consensus\n---\n# X\n\n1. Run. The session checks with `omhl list --type premise --where work_item=work-items.<id>`.\n\n## Command reference\n\n| Command | Reference |\n|---|---|\n| `omhl get` | [get.md](get.md) |\n";
const WORKFLOW_SKILL_WITHOUT_CITATION: &str = "---\nname: plan-consensus\n---\n# X\n\n1. Run.\n";

const WHO_USES_PAGE: &str = "# omhl list\n\n## Who uses it, at which stage and with which filter\n\n| Role | Stage | Command |\n|---|---|---|\n| scribe | 2, 3 | `omhl list --type premise --where work_item=work-items.<id>` |\n| session | plan-consensus 1 | `omhl list --type premise --where work_item=work-items.<id>` |\n\n## Quando usar\n";
const SCRIBE_ROW: &str =
    "| scribe | 2, 3 | `omhl list --type premise --where work_item=work-items.<id>` |\n";

const VIEW_RECORDING_WITHOUT_APPEND_IN_ALLOW: &str = r#"role: scribe
stages:
  - stage: 2
    commands:
      - "omhl append"
    records: {nodes: [premise], relations: [], retire: []}
unused: [list, get, help]
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
fn it_should_list_the_guidelines_in_every_shipped_role_view_and_allow_list() {
    for role in ROLES {
        let view = read_plugin(&format!("skills/role-{role}/SKILL.md"));
        let allow = parse_role_allow(&read_plugin(&format!("agents/{role}.md")))
            .expect("the shipped omhl-allow");

        assert!(view.contains("omhl list --type guideline"), "{role}: view");
        assert!(
            allow.commands.iter().any(|command| command == "list"),
            "{role}: allow"
        );
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
    let found = rejected(&VIEW.replace("  - stage: 3\n    repeats: 2\n", ""));

    assert_eq!(found, ["stages without a line: [3]"]);
}

#[test]
fn it_should_reject_a_stage_the_role_does_not_act_in() {
    let found = rejected(&VIEW.replace("unused", "  - stage: 1\n    repeats: 2\nunused"));

    assert_eq!(found, ["stages outside the role: [1]"]);
}

#[test]
fn it_should_reject_a_command_outside_the_omhl_allow() {
    let found = rejected(&VIEW.replace(
        "      - \"omhl append\"\n",
        "      - \"omhl append\"\n      - \"omhl render\"\n",
    ));

    assert_eq!(found, ["commands outside the omhl-allow: [render]"]);
}

#[test]
fn it_should_reject_an_unused_entry_outside_the_omhl_allow() {
    let found = rejected(&VIEW.replace("unused: [help]", "unused: [help, sweep]"));

    assert_eq!(found, ["unused outside the omhl-allow: [sweep]"]);
}

#[test]
fn it_should_sort_the_commands_outside_the_omhl_allow_together_with_the_unresolved_ones() {
    let found = rejected(&VIEW.replace(
        "      - \"omhl append\"\n",
        "      - \"omhl append\"\n      - \"omhl render\"\n      - \"omhl --help\"\n",
    ));

    assert_eq!(
        found,
        ["commands outside the omhl-allow: [omhl --help, render]"]
    );
}

#[test]
fn it_should_reject_an_omhl_allow_command_with_no_line() {
    let found = rejected(&VIEW.replace("unused: [help]", "unused: []"));

    assert_eq!(found, ["omhl-allow commands without a line: [help]"]);
}

#[test]
fn it_should_reject_a_command_both_used_and_unused() {
    let found = rejected(&VIEW.replace("unused: [help]", "unused: [help, list]"));

    assert_eq!(found, ["omhl-allow commands without a line: [list]"]);
}

#[test]
fn it_should_reject_a_recorded_item_no_stage_declares() {
    let found = rejected(&VIEW.replace(
        "relations: [answers, contradicts]",
        "relations: [contradicts]",
    ));

    assert_eq!(found, ["records without a line: [relation:answers]"]);
}

#[test]
fn it_should_tell_a_retired_item_from_a_recorded_relation_of_the_same_name() {
    let found = rejected(&VIEW.replace("retire: [contradicts]", "retire: []"));

    assert_eq!(found, ["records without a line: [retire:contradicts]"]);
}

#[test]
fn it_should_reject_a_recorded_item_outside_the_omhl_allow() {
    let found = rejected(&VIEW.replace("nodes: [premise]", "nodes: [premise, gap]"));

    assert_eq!(found, ["records outside the omhl-allow: [node:gap]"]);
}

#[test]
fn it_should_reject_a_malformed_role_view_block() {
    let found = findings(MALFORMED_VIEW);

    assert_eq!(
        found,
        [
            "block shape: [role other ≠ scribe]",
            "block shape: [stage 2: command without the omhl prefix: list]",
            "block shape: [stage 2: repeats with commands]",
            "block shape: [stage 3: no repeats and no commands]",
            "block shape: [stage 4: operator && in omhl get a && b]",
            "block shape: [stage 4: operator ; in omhl get a; b]",
            "block shape: [stage 4: operator | in omhl get a | b]",
            "block shape: [stage 4: placeholder <kind> outside <id>, <name> and <slug>]",
            "block shape: [stage 4: records without omhl append in the commands]",
            "block shape: [stage 5: omhl append without records]",
            "block shape: [stage 6: repeats stage 9, which the role does not have]",
            "block shape: [stage 10: repeats with records]",
        ]
    );
}

#[test]
fn it_should_reject_a_stage_declared_twice_or_repeating_itself() {
    let found = findings(&VIEW.replace(
        "  - stage: 3\n    repeats: 2\n",
        "  - stage: 3\n    repeats: 3\n  - stage: 2\n    repeats: 3\n",
    ));

    assert_eq!(
        found,
        [
            "block shape: [stage 2: stage repeated]",
            "block shape: [stage 3: repeats its own stage]",
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
        ["block shape: [stage 2: records, but the omhl-allow has no append]"]
    );
}

#[test]
fn it_should_reject_a_role_view_file_without_exactly_one_block() {
    let without_block = "---\nname: role-scribe\n---\n\nno fence\n";
    let two_blocks = format!("{}{}", skill_with(VIEW), skill_with(VIEW));
    let unknown_field = skill_with(&format!("{VIEW}extra: 1\n"));

    let none = check_text(without_block);
    let twice = check_text(&two_blocks);
    let unknown = check_text(&unknown_field);

    assert_eq!(
        none,
        ["block shape: [omhl-role-view fences: expected 1, found 0]"]
    );
    assert_eq!(
        twice,
        ["block shape: [omhl-role-view fences: expected 1, found 2]"]
    );
    assert_eq!(unknown.len(), 1, "{unknown:?}");
    assert!(
        unknown[0].starts_with("block shape: [yaml: ") && unknown[0].contains("extra"),
        "{unknown:?}"
    );
}

#[test]
fn it_should_refuse_a_role_view_written_with_the_old_portuguese_keys() {
    let portuguese_view = VIEW
        .replace("stages:", "etapas:")
        .replace("  - stage:", "  - etapa:")
        .replace("    commands:", "    comandos:")
        .replace("    records:", "    grava:")
        .replace("    repeats:", "    repete:")
        .replace("unused:", "nao_usados:");

    let found = findings(&portuguese_view);

    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("block shape: [yaml: ") && found[0].contains("etapas"),
        "{found:?}"
    );
}

#[test]
fn it_should_refuse_a_cycle_block_written_with_the_old_portuguese_keys() {
    let portuguese_cycle = CYCLE_MD
        .replace("stages:", "etapas:")
        .replace("name:", "nome:")
        .replace("roles:", "papeis:");

    let parsed = parse_cycle(&portuguese_cycle);

    assert!(parsed.is_err(), "{parsed:?}");
}

#[test]
fn it_should_refuse_the_old_portuguese_session_phrase() {
    let skill =
        "---\nname: x\n---\n# X\n\n1. Rode. A sessão confere com `omhl get work-items.<id>`.\n";

    let found = session_checks(skill);

    assert_eq!(
        found,
        ["steps without a session check: [1: missing the phrase \"The session checks\"]"]
    );
}

#[test]
fn it_should_reject_a_skills_list_other_than_the_role_view() {
    let bare = ["role-scribe".to_owned()];

    let spelled_bare = skill_findings(Some(&bare), &["role-scribe"]);
    let absent = skill_findings(None, &["role-scribe"]);

    assert_eq!(
        spelled_bare,
        ["agent skills: [expected oh-my-hexflow:role-scribe, found [\"role-scribe\"]]"]
    );
    assert_eq!(
        absent,
        ["agent skills: [expected oh-my-hexflow:role-scribe, found absent]"]
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
        ["agent skills: [expected oh-my-hexflow:role-scribe, found []]"]
    );
    assert_eq!(
        multiple,
        [
            "agent skills: [expected oh-my-hexflow:role-scribe, found [\"oh-my-hexflow:role-scribe\", \"oh-my-hexflow:role-other\"]]"
        ]
    );
}

#[test]
fn it_should_reject_a_cited_skill_without_a_skill_file() {
    let skills = [QUALIFIED_SKILL.to_owned()];

    let found = skill_findings(Some(&skills), &[]);

    assert_eq!(
        found,
        ["agent skill without SKILL.md: [oh-my-hexflow:role-scribe]"]
    );
}

#[test]
fn it_should_read_the_cycle_key_n_but_reject_an_unquoted_number_or_boolean_as_text() {
    let number_as_name = CYCLE_MD.replace("name: scribe", "name: 2");
    let boolean_as_role = CYCLE_MD.replace("roles: [scribe]", "roles: [true]");

    assert!(parse_cycle(CYCLE_MD).is_ok());
    assert!(parse_cycle(&number_as_name).is_err());
    assert!(parse_cycle(&boolean_as_role).is_err());
}

#[test]
fn it_should_reject_a_cycle_role_the_prose_does_not_name() {
    let prose = CYCLE_MD.replace("The `oh-my-hexflow:scribe` records.", "The agent records.");

    let found = cycle_prose(&cycle(), &prose);

    assert_eq!(found, ["cycle roles: [2: roles [] ≠ [scribe]]"]);
}

#[test]
fn it_should_reject_a_cycle_number_or_name_the_prose_does_not_have() {
    let prose = CYCLE_MD
        .replace("3. **Confirmation.**", "4. **Confirmation.**")
        .replace("2. **Scribe.**", "2. **Scribbler.**");

    let found = cycle_prose(&cycle(), &prose);

    assert_eq!(
        found,
        ["cycle roles: [2: name scribbler ≠ scribe, 3: no numbered line]"]
    );
}

#[test]
fn it_should_reject_a_workflow_step_without_a_session_check() {
    let skill = "---\nname: x\n---\n# X\n\n1. Run. The session checks with `omhl get work-items.<id>`.\n2. Call the agent.\n";

    let found = session_checks(skill);

    assert_eq!(
        found,
        [
            "steps without a session check: [2: missing the phrase \"The session checks\", 2: no omhl command]"
        ]
    );
}

#[test]
fn it_should_reject_a_workflow_skill_whose_steps_cannot_be_read() {
    let without_frontmatter =
        "# X\n\n1. Run. The session checks with `omhl get work-items.<id>`.\n";

    let found = session_checks(without_frontmatter);

    assert_eq!(
        found,
        ["steps without a session check: [no numbered step after the frontmatter]"]
    );
}

#[test]
fn it_should_reject_a_session_check_that_cites_a_leaf_the_cli_lacks() {
    let skill =
        "---\nname: x\n---\n# X\n\n1. Run. The session checks with `omhl frobnicate --now`.\n";

    let found = session_checks(skill);

    assert_eq!(
        found,
        ["steps without a session check: [1: unknown leaf frobnicate]"]
    );
}

fn who_uses_of_list(page: &str, workflow_skill: &str) -> Vec<String> {
    let view = skill_with(VIEW);

    who_uses(
        "list",
        page,
        &[("scribe", view.as_str())],
        &[("plan-consensus", workflow_skill)],
    )
}

/// Confere que `WHO_USES_PAGE` passa e que `mutated` não, e devolve as linhas que `mutated` provoca.
fn rejected_page(mutated: &str) -> Vec<String> {
    let failures = prove_both_sides(
        |page| Ok(who_uses_of_list(page, WORKFLOW_SKILL).len()),
        &[mutated],
        &[WHO_USES_PAGE],
    );
    assert!(failures.is_empty(), "{failures:?}");

    who_uses_of_list(mutated, WORKFLOW_SKILL)
}

fn synthetic_anchor_findings(cli: &[&str], anchor: &[(&str, WhoUsesClass)]) -> Vec<String> {
    let view = skill_with(VIEW);
    let leaves: Vec<String> = cli.iter().map(|leaf| (*leaf).to_owned()).collect();

    anchor_findings(
        &leaves,
        &["append"],
        anchor,
        &[("scribe", view.as_str())],
        &[("plan-consensus", WORKFLOW_SKILL)],
    )
}

fn migrated_page(leaf: &str) -> String {
    let file = format!("{}.md", leaf.replace(' ', "-"));
    let found: Vec<String> = skill_dirs_with_a_file()
        .iter()
        .map(|dir| format!("skills/{dir}/references/{file}"))
        .filter(|relative| plugin_dir().join(relative).is_file())
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{leaf} must have exactly one page: {found:?}"
    );

    read_plugin(&found[0])
}

fn shipped_texts(names: &[&str], file: impl Fn(&str) -> String) -> Vec<(String, String)> {
    names
        .iter()
        .map(|name| ((*name).to_owned(), read_plugin(&file(name))))
        .collect()
}

fn borrowed(pairs: &[(String, String)]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect()
}

#[test]
fn it_should_reject_a_who_uses_row_the_views_do_not_project() {
    let found = rejected_page(
        &WHO_USES_PAGE.replace("| session |", "| critic | 3 | `omhl list` |\n| session |"),
    );

    assert_eq!(
        found,
        ["who-uses of page list: [extra: critic | 3 | omhl list]"]
    );
}

#[test]
fn it_should_reject_a_projected_row_missing_from_the_page() {
    let found = rejected_page(&WHO_USES_PAGE.replace(SCRIBE_ROW, ""));

    assert_eq!(
        found,
        [
            "who-uses of page list: [missing: scribe | 2, 3 | omhl list --type premise --where work_item=work-items.<id>]"
        ]
    );
}

#[test]
fn it_should_reject_a_session_row_the_workflow_skills_do_not_cite() {
    let found = who_uses_of_list(WHO_USES_PAGE, WORKFLOW_SKILL_WITHOUT_CITATION);

    assert_eq!(
        found,
        [
            "who-uses of page list: [extra: session | plan-consensus 1 | omhl list --type premise --where work_item=work-items.<id>]"
        ]
    );
}

#[test]
fn it_should_reject_a_projection_class_the_anchor_does_not_expect() {
    let found = synthetic_anchor_findings(
        &["append", "get", "list"],
        &[
            ("get", WhoUsesClass::RolesOnly),
            ("list", WhoUsesClass::RolesOnly),
        ],
    );

    assert_eq!(
        found,
        ["class diverging from the anchor: [list: expected RolesOnly, projected Session]"]
    );
}

#[test]
fn it_should_reject_a_migrated_page_missing_from_the_anchor() {
    let found = synthetic_anchor_findings(
        &["append", "get", "list", "verify"],
        &[
            ("get", WhoUsesClass::RolesOnly),
            ("list", WhoUsesClass::Session),
        ],
    );

    assert_eq!(found, ["leaves outside the anchor: [verify]"]);
}

#[test]
fn it_should_reject_an_anchor_entry_with_no_cli_leaf() {
    let found = synthetic_anchor_findings(
        &["append", "get", "list"],
        &[
            ("get", WhoUsesClass::RolesOnly),
            ("list", WhoUsesClass::Session),
            ("render", WhoUsesClass::NoRole),
        ],
    );

    assert_eq!(found, ["anchor without a CLI leaf: [render]"]);
}

#[test]
fn it_should_accept_the_shipped_who_uses_tables() {
    let views = shipped_texts(&ROLES, |role| format!("skills/role-{role}/SKILL.md"));
    let workflow = shipped_texts(&WORKFLOW_SKILLS, |skill| format!("skills/{skill}/SKILL.md"));

    for (leaf, _) in WHO_USES_ANCHOR {
        let found = who_uses(
            leaf,
            &migrated_page(leaf),
            &borrowed(&views),
            &borrowed(&workflow),
        );

        assert_eq!(found, Vec::<String>::new(), "{leaf}");
    }
}

#[test]
fn it_should_keep_the_who_uses_classes_equal_to_the_anchor() {
    let views = shipped_texts(&ROLES, |role| format!("skills/role-{role}/SKILL.md"));
    let workflow = shipped_texts(&WORKFLOW_SKILLS, |skill| format!("skills/{skill}/SKILL.md"));

    let found = anchor_findings(
        &cli_leaves(),
        &BASE_LEAVES,
        &WHO_USES_ANCHOR,
        &borrowed(&views),
        &borrowed(&workflow),
    );

    assert_eq!(found, Vec::<String>::new());
}

fn marker_files() -> Vec<(String, String)> {
    let mut paths: Vec<&str> = MARKERS.iter().map(|marker| marker.file).collect();
    paths.sort_unstable();
    paths.dedup();

    paths
        .into_iter()
        .map(|path| (path.to_owned(), read_plugin(path)))
        .collect()
}

fn marker_item(marker: &Marker) -> String {
    format!(
        "{} in {}: {}",
        marker.rule, marker.file, marker.fragments[0]
    )
}

#[test]
fn it_should_accept_the_shipped_phrase_markers() {
    let files = marker_files();

    let found = phrase_markers(&MARKERS, &borrowed(&files));

    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn it_should_reject_each_shipped_marker_whose_line_leaves_the_text() {
    for marker in &MARKERS {
        let files: Vec<(String, String)> = marker_files()
            .into_iter()
            .map(|(path, text)| {
                let kept: Vec<&str> = text
                    .lines()
                    .filter(|line| {
                        path != marker.file
                            || !marker
                                .fragments
                                .iter()
                                .all(|fragment| line.contains(fragment))
                    })
                    .collect();
                (path, kept.join("\n"))
            })
            .collect();

        let found = phrase_markers(&MARKERS, &borrowed(&files));

        let expected = marker_item(marker);
        assert!(
            found.iter().any(|item| item.contains(&expected)),
            "{expected}: {found:?}"
        );
    }
}

#[test]
fn it_should_require_the_marker_fragments_on_one_line() {
    let marker = Marker {
        rule: "AC-X",
        file: "skills/x/SKILL.md",
        fragments: &["alpha", "beta"],
    };
    let together = [("skills/x/SKILL.md", "an alpha and beta together\n")];
    let apart = [("skills/x/SKILL.md", "only alpha\nonly beta\n")];

    let accepted = phrase_markers(std::slice::from_ref(&marker), &together);
    let rejected = phrase_markers(std::slice::from_ref(&marker), &apart);

    assert_eq!(accepted, Vec::<String>::new());
    assert_eq!(
        rejected,
        ["marker phrases missing: [AC-X in skills/x/SKILL.md: alpha]"]
    );
}

#[test]
fn it_should_reject_a_marker_whose_file_is_not_given() {
    let marker = Marker {
        rule: "AC-X",
        file: "skills/x/SKILL.md",
        fragments: &["alpha"],
    };

    let found = phrase_markers(std::slice::from_ref(&marker), &[]);

    assert_eq!(
        found,
        ["marker phrases missing: [AC-X in skills/x/SKILL.md: alpha]"]
    );
}
