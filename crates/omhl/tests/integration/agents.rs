//! Os sete agentes portados do oh-my-claudecode 5.7.19 em `plugin/agents/`, o aviso de terceiros e os
//! créditos do README. Todo teste lê os sete arquivos: com um agente faltando, o teste falha (nunca passa vazio).
//! O frontmatter é lido com o `serde-saphyr` do próprio crate; o `omhl-allow`, pelo parser do hook.

use crate::support;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use clap::CommandFactory;
use omhl::adapters::cli::{Cli, command_groups};
use omhl::guard::{ROLE_REFERENCE_COMMANDS, ROLE_REFERENCE_MATRIX};
use omhl::workflow::omhl_leaf::{Leaf, resolve_leaf};
use omhl::workflow::role_allow::{RoleAllow, parse_role_allow};
use regex::Regex;
use serde::Deserialize;
use support::fences::{FenceLine, lines_by_fence};
use support::repo_checks::repo_root;

const ORIGIN_VERSION: &str = "5.7.19";
const OPUS_ROLES: [&str; 2] = ["architect", "critic"];
const PLANNER: &str = "planner";
const GROUNDED_ROLES: [&str; 3] = ["critic", "code-reviewer", "security-reviewer"];

/// Tags de topo (filhas de `<Agent_Prompt>`) de cada agente do OMC 5.7.19, congeladas: o CI não tem o cache do OMC.
const UPSTREAM_TOP_LEVEL_TAGS: [(&str, &[&str]); 7] = [
    (
        "analyst",
        &[
            "Role",
            "Why_This_Matters",
            "Success_Criteria",
            "Constraints",
            "Investigation_Protocol",
            "Tool_Usage",
            "Execution_Policy",
            "Output_Format",
            "Final_Response_Contract",
            "Failure_Modes_To_Avoid",
            "Examples",
            "Open_Questions",
            "Final_Checklist",
        ],
    ),
    (
        "planner",
        &[
            "Role",
            "Why_This_Matters",
            "Success_Criteria",
            "Constraints",
            "Investigation_Protocol",
            "Consensus_RALPLAN_DR_Protocol",
            "Tool_Usage",
            "Execution_Policy",
            "Output_Format",
            "Failure_Modes_To_Avoid",
            "Examples",
            "Open_Questions",
            "Final_Checklist",
        ],
    ),
    (
        "architect",
        &[
            "Role",
            "Why_This_Matters",
            "Success_Criteria",
            "Constraints",
            "Investigation_Protocol",
            "Tool_Usage",
            "Execution_Policy",
            "Output_Format",
            "Final_Response_Contract",
            "Failure_Modes_To_Avoid",
            "Examples",
            "Final_Checklist",
        ],
    ),
    (
        "critic",
        &[
            "Role",
            "Why_This_Matters",
            "Success_Criteria",
            "Constraints",
            "Investigation_Protocol",
            "Evidence_Requirements",
            "Tool_Usage",
            "Execution_Policy",
            "Output_Format",
            "Final_Response_Contract",
            "Failure_Modes_To_Avoid",
            "Examples",
            "Final_Checklist",
        ],
    ),
    (
        "code-reviewer",
        &[
            "Role",
            "Why_This_Matters",
            "Success_Criteria",
            "Constraints",
            "Investigation_Protocol",
            "Tool_Usage",
            "Execution_Policy",
            "Discovery_Filtering_Separation",
            "Review_Checklist",
            "Output_Format",
            "Final_Response_Contract",
            "Failure_Modes_To_Avoid",
            "Examples",
            "Final_Checklist",
            "API_Contract_Review",
            "Style_Review_Mode",
            "Performance_Review_Mode",
            "Quality_Strategy_Mode",
        ],
    ),
    (
        "security-reviewer",
        &[
            "Role",
            "Why_This_Matters",
            "Success_Criteria",
            "Constraints",
            "Investigation_Protocol",
            "Tool_Usage",
            "Execution_Policy",
            "OWASP_Top_10",
            "Security_Checklists",
            "Severity_Definitions",
            "Output_Format",
            "Final_Response_Contract",
            "Failure_Modes_To_Avoid",
            "Examples",
            "Final_Checklist",
        ],
    ),
    (
        "verifier",
        &[
            "Role",
            "Why_This_Matters",
            "Success_Criteria",
            "Constraints",
            "Investigation_Protocol",
            "Tool_Usage",
            "Execution_Policy",
            "Output_Format",
            "Final_Response_Contract",
            "Failure_Modes_To_Avoid",
            "Examples",
            "Final_Checklist",
        ],
    ),
];

/// Trechos do texto MIT do `LICENSE` do OMC 5.7.19 (espaços normalizados) e o detentor do copyright.
const MIT_FRAGMENTS: [&str; 6] = [
    "MIT License",
    "Copyright (c) 2025 Yeachan Heo",
    "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files",
    "The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.",
    "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND",
    "IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    model: Option<String>,
    disallowed_tools: Option<String>,
    omit_claude_md: Option<bool>,
}

struct Agent {
    role: &'static str,
    file: String,
    frontmatter: String,
    body: String,
}

impl Agent {
    fn parsed(&self) -> Frontmatter {
        serde_saphyr::from_str(&self.frontmatter)
            .unwrap_or_else(|error| panic!("{}: invalid frontmatter: {error}", self.role))
    }

    fn disallowed_tools(&self) -> BTreeSet<String> {
        self.parsed()
            .disallowed_tools
            .unwrap_or_else(|| panic!("{}: no disallowedTools", self.role))
            .split(',')
            .map(|tool| tool.trim().to_owned())
            .collect()
    }

    fn allow(&self) -> RoleAllow {
        parse_role_allow(&self.file)
            .unwrap_or_else(|error| panic!("{}: invalid omhl-allow: {error}", self.role))
    }

    /// Chaves de topo do frontmatter (linhas sem recuo), para provar a ausência de `tools`.
    fn top_level_keys(&self) -> BTreeSet<&str> {
        self.frontmatter
            .lines()
            .filter(|line| !line.starts_with([' ', '\t', '#']))
            .filter_map(|line| line.split_once(':').map(|(key, _)| key.trim()))
            .collect()
    }
}

fn agents_dir() -> PathBuf {
    repo_root().join("plugin/agents")
}

/// `omhl:analyst` -> `analyst`; os sete papéis vêm da constante de referência da guarda.
fn roles() -> Vec<&'static str> {
    ROLE_REFERENCE_COMMANDS
        .iter()
        .map(|(role, _)| {
            role.strip_prefix("omhl:")
                .expect("reference roles carry the omhl: prefix")
        })
        .collect()
}

/// `(frontmatter, corpo)`: o frontmatter fica entre a primeira linha `---` e a seguinte.
fn split_agent(file: &str) -> (String, String) {
    let rest = file
        .strip_prefix("---\n")
        .expect("the agent file opens with a --- line");
    let end = rest
        .find("\n---\n")
        .expect("the frontmatter is closed by a --- line");

    (
        rest[..=end].to_owned(),
        rest[end + "\n---\n".len()..].to_owned(),
    )
}

fn agent(role: &'static str) -> Agent {
    let path = agents_dir().join(format!("{role}.md"));
    let file = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is missing: {error}", path.display()));
    let (frontmatter, body) = split_agent(&file);

    Agent {
        role,
        file,
        frontmatter,
        body,
    }
}

fn agents() -> Vec<Agent> {
    roles().into_iter().map(agent).collect()
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn planner_prompt() -> String {
    squash(&agent(PLANNER).body)
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn strings(items: Vec<String>) -> BTreeSet<String> {
    items.into_iter().collect()
}

/// Tags sozinhas numa linha que são filhas diretas de `<Agent_Prompt>`; blocos de código ficam de fora.
fn top_level_tags(body: &str) -> Vec<String> {
    let tag = Regex::new(r"^<(/?)([A-Za-z][A-Za-z0-9_]*)>$").expect("regex");
    let mut stack: Vec<String> = Vec::new();
    let mut tops = Vec::new();
    for (place, line) in lines_by_fence(body) {
        let Some(captures) = tag
            .captures(line.trim())
            .filter(|_| place == FenceLine::Outside)
        else {
            continue;
        };
        if &captures[1] == "/" {
            stack.pop();
            continue;
        }
        if stack == ["Agent_Prompt"] {
            tops.push(captures[2].to_owned());
        }
        stack.push(captures[2].to_owned());
    }

    tops
}

/// Palavras depois de cada `omhl` citado: entre crases na prosa, ou no começo da linha (com `$ ` opcional) num bloco de código.
fn omhl_citations(body: &str) -> Vec<Vec<String>> {
    let inline = Regex::new(r"`omhl((?: [a-z][a-z0-9-]*)+)").expect("regex");
    let in_fence = Regex::new(r"^\s*(?:\$ )?omhl((?: [a-z][a-z0-9-]*)+)").expect("regex");
    let mut found = Vec::new();
    for (place, line) in lines_by_fence(body) {
        let pattern = match place {
            FenceLine::Open | FenceLine::Close => continue,
            FenceLine::Inside => &in_fence,
            FenceLine::Outside => &inline,
        };
        for captures in pattern.captures_iter(line) {
            found.push(captures[1].split_whitespace().map(str::to_owned).collect());
        }
    }

    found
}

#[test]
fn it_should_ship_exactly_the_seven_ported_agents() {
    let mut shipped: Vec<String> = fs::read_dir(agents_dir())
        .expect("plugin/agents exists")
        .map(|entry| {
            entry
                .expect("readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    shipped.sort();

    let mut expected: Vec<String> = roles().iter().map(|role| format!("{role}.md")).collect();
    expected.sort();

    assert_eq!(shipped, expected);
}

#[test]
fn it_should_name_each_agent_after_its_file() {
    for agent in agents() {
        assert_eq!(
            agent.parsed().name.as_deref(),
            Some(agent.role),
            "{}",
            agent.role
        );
    }
}

#[test]
fn it_should_describe_each_agent() {
    for agent in agents() {
        assert!(
            agent
                .parsed()
                .description
                .is_some_and(|description| !description.trim().is_empty()),
            "{}: description",
            agent.role
        );
    }
}

#[test]
fn it_should_set_model_opus_for_architect_and_critic_and_sonnet_for_the_other_five() {
    for agent in agents() {
        let expected = if OPUS_ROLES.contains(&agent.role) {
            "opus"
        } else {
            "sonnet"
        };

        assert_eq!(
            agent.parsed().model.as_deref(),
            Some(expected),
            "{}",
            agent.role
        );
    }
}

#[test]
fn it_should_open_each_agent_with_an_origin_line_naming_oh_my_claudecode_5_7_19() {
    for agent in agents() {
        let first_line = agent
            .body
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();

        let origin = format!(
            "Ported from oh-my-claudecode {ORIGIN_VERSION} (MIT), agents/{}.md",
            agent.role
        );

        assert!(
            first_line.starts_with(&origin),
            "{}: {first_line}",
            agent.role
        );
    }
}

#[test]
fn it_should_not_mention_oh_my_claudecode_colon_or_dot_omc_slash_in_any_agent() {
    for agent in agents() {
        for forbidden in ["oh-my-claudecode:", ".omc/"] {
            assert!(
                !agent.file.contains(forbidden),
                "{} mentions {forbidden}",
                agent.role
            );
        }
    }
}

#[test]
fn it_should_omit_the_tools_key_in_every_ported_agent() {
    for agent in agents() {
        assert!(
            !agent.top_level_keys().contains("tools"),
            "{} declares tools",
            agent.role
        );
    }
}

#[test]
fn it_should_disallow_agent_and_monitor_in_every_ported_agent() {
    for agent in agents() {
        let disallowed = agent.disallowed_tools();

        assert!(
            disallowed.is_superset(&set(&["Agent", "Monitor"])),
            "{}: {disallowed:?}",
            agent.role
        );
    }
}

#[test]
fn it_should_set_omit_claude_md_true_in_every_ported_agent() {
    for agent in agents() {
        assert_eq!(agent.parsed().omit_claude_md, Some(true), "{}", agent.role);
    }
}

#[test]
fn it_should_send_human_output_to_the_hook_announced_language_in_every_agent_prompt() {
    for agent in agents() {
        let body = squash(&agent.body);

        assert!(
            body.contains("is written in the language the `SubagentStart` hook line announces"),
            "{}: language sentence",
            agent.role
        );
        assert!(
            !body.contains("All output addressed to humans is written in pt-BR."),
            "{}: old pt-BR sentence",
            agent.role
        );
    }
}

#[test]
fn it_should_not_repeat_the_english_code_convention_in_any_agent_prompt() {
    for agent in agents() {
        assert!(
            !squash(&agent.body)
                .contains("Code identifiers, error messages and tests are written in English."),
            "{}: the convention lives in the guideline",
            agent.role
        );
    }
}

#[test]
fn it_should_not_repeat_the_no_emoji_rule_in_any_agent_prompt() {
    for agent in agents() {
        assert!(
            !squash(&agent.body).contains("Use no emoji."),
            "{}: the convention lives in the guideline",
            agent.role
        );
    }
}

#[test]
fn it_should_keep_every_top_level_section_tag_of_the_upstream_agent() {
    for (role, upstream) in UPSTREAM_TOP_LEVEL_TAGS {
        let ported = top_level_tags(&agent(role).body);

        let missing: Vec<&&str> = upstream
            .iter()
            .filter(|tag| !ported.iter().any(|kept| kept == **tag))
            .collect();

        assert!(missing.is_empty(), "{role} lost {missing:?}");
    }
}

#[test]
fn it_should_declare_a_valid_omhl_allow_in_each_agent() {
    for agent in agents() {
        let parsed = parse_role_allow(&agent.file);

        assert!(parsed.is_ok(), "{}: {:?}", agent.role, parsed.err());
    }
}

#[test]
fn it_should_declare_omhl_allow_commands_nodes_relations_and_retire_equal_to_the_compiled_matrix_of_its_role()
 {
    for (reference_role, commands) in ROLE_REFERENCE_COMMANDS {
        let role = reference_role.strip_prefix("omhl:").expect("omhl: prefix");
        let (_, nodes, relations, retire) = ROLE_REFERENCE_MATRIX
            .iter()
            .find(|(matrix_role, ..)| *matrix_role == reference_role)
            .expect("the role is in the reference matrix");

        let allow = agent(role).allow();
        let (declared_nodes, declared_relations, declared_retire) = allow
            .append
            .map(|append| (append.nodes, append.relations, append.retire))
            .unwrap_or_default();

        assert_eq!(strings(allow.commands), set(commands), "{role}: commands");
        assert_eq!(strings(declared_nodes), set(nodes), "{role}: nodes");
        assert_eq!(
            strings(declared_relations),
            set(relations),
            "{role}: relations"
        );
        assert_eq!(strings(declared_retire), set(retire), "{role}: retire");
    }
}

#[test]
fn it_should_deny_write_and_edit_to_every_agent_except_the_planner() {
    for agent in agents().iter().filter(|agent| agent.role != PLANNER) {
        assert!(
            agent
                .disallowed_tools()
                .is_superset(&set(&["Write", "Edit"])),
            "{}",
            agent.role
        );
    }
}

#[test]
fn it_should_leave_write_and_edit_available_to_the_planner_and_disallow_only_agent_and_monitor() {
    let planner = agent(PLANNER);

    assert_eq!(planner.disallowed_tools(), set(&["Agent", "Monitor"]));
}

#[test]
fn it_should_not_instruct_a_command_outside_the_matrix_of_its_role() {
    let groups = command_groups(&Cli::command());

    for agent in agents() {
        let allow = agent.allow();
        let citations = omhl_citations(&agent.body);

        let outside: Vec<String> = citations
            .iter()
            .filter_map(|words| match resolve_leaf(words, &groups) {
                Some(Leaf::Named(leaf)) if !allow.commands.contains(&leaf) => Some(leaf),
                _ => None,
            })
            .collect();

        assert!(
            !citations.is_empty(),
            "{}: cites no omhl command",
            agent.role
        );
        assert!(outside.is_empty(), "{} instructs {outside:?}", agent.role);
    }
}

#[test]
fn it_should_tell_critic_and_reviewers_to_ground_each_decision_with_rests_on() {
    for role in GROUNDED_ROLES {
        let body = squash(&agent(role).body);

        for phrase in ["rests-on", "work_item", "confirmed premise or directive"] {
            assert!(body.contains(phrase), "{role} lacks '{phrase}'");
        }
    }
}

#[test]
fn it_should_not_tell_the_reviewers_that_a_contradicts_may_start_from_a_divergence() {
    for role in ["code-reviewer", "security-reviewer"] {
        let body = squash(&agent(role).body);
        let start = body
            .find("`contradicts` relation only from")
            .unwrap_or_else(|| panic!("{role} lacks the contradicts rule"));
        let rule = &body[start..start + 120];

        assert!(
            rule.contains("only from a `decision` you wrote (never from a divergence"),
            "{role}: {rule}"
        );
    }
}

#[test]
fn it_should_tell_every_agent_with_append_to_use_a_quoted_heredoc_one_omhl_command_per_call_and_no_pipe()
 {
    let with_append: Vec<Agent> = agents()
        .into_iter()
        .filter(|agent| agent.allow().commands.contains(&"append".to_owned()))
        .collect();

    assert!(!with_append.is_empty());
    for agent in with_append {
        let body = squash(&agent.body);

        for phrase in [
            "omhl append <<'EOF'",
            "Run one `omhl` command per Bash call.",
            "Start the command with `omhl`",
            "Never pipe to or from `omhl`",
            "`&&`, `||` or `;`",
            "Never write `OMHL_ROLE`",
        ] {
            assert!(body.contains(phrase), "{} lacks '{phrase}'", agent.role);
        }
    }
}

#[test]
fn it_should_tell_the_planner_to_run_omhl_spec_init_and_omhl_spec_validate() {
    let body = planner_prompt();

    assert!(
        body.contains("omhl spec init"),
        "prompt lacks omhl spec init"
    );
    assert!(
        body.contains("omhl spec validate"),
        "prompt lacks omhl spec validate"
    );
}

#[test]
fn it_should_allow_omhl_spec_init_and_omhl_spec_validate_in_the_planner_matrix() {
    let allow = agent(PLANNER).allow();

    assert!(allow.commands.contains(&"spec init".to_owned()));
    assert!(allow.commands.contains(&"spec validate".to_owned()));
}

#[test]
fn it_should_ship_third_party_notices_md_with_the_mit_text_and_the_version_5_7_19() {
    let notices = squash(
        &fs::read_to_string(repo_root().join("THIRD_PARTY_NOTICES.md"))
            .expect("THIRD_PARTY_NOTICES.md exists at the repo root"),
    );

    for fragment in MIT_FRAGMENTS.into_iter().chain([ORIGIN_VERSION]) {
        assert!(notices.contains(fragment), "the notices lack '{fragment}'");
    }
}

#[test]
fn it_should_have_a_credits_section_in_the_readme_naming_the_omc_origin() {
    let readme = fs::read_to_string(repo_root().join("README.md")).expect("README.md exists");

    let section: String = readme
        .lines()
        .skip_while(|line| line.trim_end() != "## Créditos")
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect::<Vec<_>>()
        .join("\n");
    let section = squash(&section);

    for fragment in [
        "oh-my-claudecode",
        ORIGIN_VERSION,
        "Yeachan Heo",
        "THIRD_PARTY_NOTICES.md",
    ] {
        assert!(section.contains(fragment), "the credits lack '{fragment}'");
    }
}

const SPEC_READERS: [&str; 4] = ["code-reviewer", "security-reviewer", "verifier", "critic"];
const HANDOFF_ROLES: [&str; 6] = [
    "analyst",
    "architect",
    "code-reviewer",
    "security-reviewer",
    "verifier",
    "critic",
];
const DECISION_ROLES: [&str; 4] = ["architect", "critic", "code-reviewer", "security-reviewer"];
/// Conceitos que só existem no OMC e virariam instrução quebrada aqui (`ralplan` segue permitido; `executor` e `qa-tester` só como descrição de papel, nunca no handoff).
const OMC_ONLY_CONCEPTS: [&str; 7] = [
    "start-work",
    "AskUserQuestion",
    "notepad",
    "/team",
    "Task(",
    "lsp_",
    "ast_grep",
];

/// O texto de uma tag de topo do prompt (tag sozinha na linha, fora de cercas), sem as tags de abertura e fechamento.
fn section(body: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut inside = false;
    let mut lines = Vec::new();

    for (place, line) in lines_by_fence(body) {
        let outside_fence = place == FenceLine::Outside;
        let trimmed = line.trim();
        if outside_fence && !inside && trimmed == open {
            inside = true;
            continue;
        }
        if outside_fence && inside && trimmed == close {
            return lines.join("\n");
        }
        if inside {
            lines.push(line);
        }
    }

    if inside {
        panic!("no {close} closing line");
    }
    panic!("no {open} line outside code fences");
}

/// As duas últimas linhas não vazias do `Output_Format`, na ordem em que aparecem.
fn last_two_output_lines(agent: &Agent) -> [String; 2] {
    let output = section(&agent.body, "Output_Format");
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let [.., penultimate, last] = lines.as_slice() else {
        panic!(
            "{}: the Output_Format has {} non-empty lines, expected at least 2",
            agent.role,
            lines.len()
        );
    };

    [(*penultimate).to_owned(), (*last).to_owned()]
}

#[test]
fn it_should_tell_the_planner_where_the_work_item_id_comes_from() {
    let tools = squash(&section(&agent(PLANNER).body, "Tool_Usage"));

    for phrase in [
        "The main session gives you the work item id (`work-items.<id>`)",
        "`omhl list --type work-item`",
        "Never invent one.",
    ] {
        assert!(tools.contains(phrase), "planner lacks '{phrase}'");
    }
}

#[test]
fn it_should_tell_the_spec_readers_that_the_spec_lives_in_the_main_checkout_and_how_to_find_the_item_ids()
 {
    for role in SPEC_READERS {
        let tools = squash(&section(&agent(role).body, "Tool_Usage"));

        for phrase in [
            "lives in the main checkout, outside your worktree",
            "in the folder the main session gives you in the request (default `.omhl/specs/wi-<id>__<slug>/`)",
            "read the parts to find the `CMP-###` and `CA-###` items",
            "return to the main session asking for it",
        ] {
            assert!(tools.contains(phrase), "{role} lacks '{phrase}'");
        }
    }
}

#[test]
fn it_should_make_the_verifier_ask_the_main_session_for_the_contradicts_pair_to_retire() {
    let tools = squash(&section(&agent("verifier").body, "Tool_Usage"));

    for phrase in [
        "come from the request of the main session: the read commands return nodes, not relations",
        "return to the main session asking for it",
        "Never guess a pair and never retire a relation it did not name.",
    ] {
        assert!(tools.contains(phrase), "verifier lacks '{phrase}'");
    }
}

#[test]
fn it_should_spell_out_the_new_divergence_fields_in_the_verifier_instead_of_pointing_to_the_reviewers()
 {
    let tools = squash(&section(&agent("verifier").body, "Tool_Usage"));

    assert!(
        !tools.contains("example of the reviewers"),
        "the verifier still points to the reviewers"
    );
    for phrase in [
        "`source_ref` (the `CMP-###` or `CA-###` item, or the id of the premise or directive)",
        "`kind` (`missing`, `partial`, `contradicts` or `unrequested`)",
        "`evidence` (1 to 20 items",
        "and the `work_item`",
    ] {
        assert!(tools.contains(phrase), "verifier lacks '{phrase}'");
    }
}

#[test]
fn it_should_not_offer_the_planner_a_restart_it_cannot_perform() {
    let body = planner_prompt();

    for forbidden in ["\"restart\"", "Discard and start fresh"] {
        assert!(
            !body.contains(forbidden),
            "planner still offers {forbidden}"
        );
    }
}

#[test]
fn it_should_tell_the_planner_to_return_on_spec_exists() {
    let body = planner_prompt();

    for phrase in [
        "`spec-exists`",
        "do not run `omhl spec init`",
        "you cannot create a second spec",
        "edit them with Edit/Write",
    ] {
        assert!(body.contains(phrase), "planner lacks '{phrase}'");
    }
}

#[test]
fn it_should_put_the_next_role_field_second_to_last_in_the_output_format_of_every_agent() {
    for agent in agents() {
        let [penultimate, _] = last_two_output_lines(&agent);

        assert!(
            penultimate.starts_with("- Next role: "),
            "{}: {penultimate}",
            agent.role
        );
    }
}

#[test]
fn it_should_put_the_open_questions_field_last_in_the_output_format_of_every_agent() {
    for agent in agents() {
        let [_, last] = last_two_output_lines(&agent);

        assert!(
            last.starts_with("- Open questions for the main session"),
            "{}: {last}",
            agent.role
        );
    }
}

#[test]
fn it_should_carry_the_suggested_kind_in_the_open_questions_of_every_agent() {
    for agent in agents() {
        let output = section(&agent.body, "Output_Format");

        assert!(
            squash(&output).contains("(suggested kind: technical | business)"),
            "{}: the questions carry no kind",
            agent.role
        );
    }
}

/// As opções do campo `- Next role: <a | b | none>` do `Output_Format`, sem o `none`.
fn next_role_options(body: &str) -> BTreeSet<String> {
    let output = section(body, "Output_Format");
    let line = output
        .lines()
        .find(|line| line.trim_start().starts_with("- Next role: <"))
        .expect("the output format has a Next role field");
    let (_, rest) = line.split_once('<').expect("the field opens with <");
    let (options, _) = rest.split_once('>').expect("the field closes with >");

    options
        .split('|')
        .map(str::trim)
        .filter(|role| *role != "none")
        .map(str::to_owned)
        .collect()
}

#[test]
fn it_should_offer_in_the_next_role_field_only_the_seven_roles_and_the_analyst_and_the_critic_for_the_planner()
 {
    for agent in agents() {
        let expected = if agent.role == PLANNER {
            set(&["analyst", "critic"])
        } else {
            strings(roles().into_iter().map(str::to_owned).collect())
        };

        assert_eq!(
            next_role_options(&agent.body),
            expected,
            "{}: the Next role field",
            agent.role
        );
    }
}

#[test]
fn it_should_not_name_qa_tester_or_executor_in_the_handoff_instruction_of_any_agent() {
    for agent in agents() {
        for line in agent
            .body
            .lines()
            .filter(|line| line.contains("Hand off by naming the next role"))
        {
            for outsider in ["qa-tester", "executor"] {
                assert!(
                    !line.contains(outsider),
                    "{}: the handoff names {outsider}",
                    agent.role
                );
            }
        }
    }
}

#[test]
fn it_should_tell_the_analyst_to_choose_the_kind_by_who_can_answer_and_to_default_to_business() {
    let body = squash(&agent("analyst").body);

    for phrase in [
        "Choose the `kind` by who can answer",
        "policy, scope, priority, risk or cost",
        "When in doubt, use `business`",
    ] {
        assert!(body.contains(phrase), "analyst lacks '{phrase}'");
    }
}

#[test]
fn it_should_tell_the_critic_not_to_confirm_a_technical_premise_that_is_a_business_decision() {
    let body = squash(&agent("critic").body);

    for phrase in [
        "judge its content, not only its label",
        "is in fact a business decision",
        "do not confirm it",
    ] {
        assert!(body.contains(phrase), "critic lacks '{phrase}'");
    }
}

#[test]
fn it_should_forbid_copying_a_secret_value_into_the_graph_in_every_agent_that_records_evidence() {
    assert!(squash(&agent("security-reviewer").body).contains("Never copy the value of a secret"));

    for role in ["code-reviewer", "critic", "verifier"] {
        assert!(
            squash(&agent(role).body).contains("never copy a secret value found in it"),
            "{role} lacks the secret rule"
        );
    }
}

#[test]
fn it_should_name_the_handoff_fields_in_the_final_response_contract_of_the_six_agents_that_have_one()
 {
    for role in HANDOFF_ROLES {
        let contract = squash(&section(&agent(role).body, "Final_Response_Contract"));

        for phrase in [
            "ends with the Handoff fields",
            "the next role the main session should call (or \"none\")",
            "each marked `technical` or `business`",
        ] {
            assert!(contract.contains(phrase), "{role} lacks '{phrase}'");
        }
    }
}

#[test]
fn it_should_cite_the_recurrence_report_in_the_analyst_with_the_min_flag() {
    let body = squash(&agent("analyst").body);

    assert!(body.contains("`omhl report recurrences --min 2`"));
}

#[test]
fn it_should_allow_the_recurrence_report_in_the_analyst_matrix() {
    let allow = agent("analyst").allow();

    assert!(allow.commands.contains(&"report recurrences".to_owned()));
}

#[test]
fn it_should_have_the_min_flag_in_the_recurrence_report_of_the_cli() {
    let report = Cli::command()
        .find_subcommand("report")
        .expect("the report group exists")
        .clone();
    let recurrences = report
        .find_subcommand("recurrences")
        .expect("the recurrences report exists");

    assert!(recurrences.get_arguments().any(|arg| arg.get_id() == "min"));
}

#[test]
fn it_should_not_mention_any_omc_only_concept_that_would_be_a_broken_instruction_here() {
    for agent in agents() {
        for concept in OMC_ONLY_CONCEPTS {
            assert!(
                !agent.file.contains(concept),
                "{} mentions {concept}",
                agent.role
            );
        }
    }
}

#[test]
fn it_should_put_the_rests_on_instruction_inside_the_tool_usage_part_that_says_what_the_agent_records()
 {
    for role in DECISION_ROLES {
        let tools = squash(&section(&agent(role).body, "Tool_Usage"));
        let (_, recording) = tools
            .split_once("What you record")
            .unwrap_or_else(|| panic!("{role}: no 'What you record' part"));

        for phrase in [
            "link it with a `rests-on` relation",
            "to a confirmed premise or directive",
            "`work_item`",
            "cannot anchor",
        ] {
            assert!(
                recording.contains(phrase),
                "{role}: the recording part lacks '{phrase}'"
            );
        }
    }
}

#[test]
fn it_should_make_every_decision_recording_reviewer_list_each_relation_as_a_from_to_pair_in_the_final_message()
 {
    for role in ["critic", "code-reviewer", "security-reviewer"] {
        let contract = squash(&section(&agent(role).body, "Final_Response_Contract"));

        assert!(
            contract.contains("`<relation>: <from> -> <to>`"),
            "{role}: the final message must list each relation as a from -> to pair"
        );
    }
}
