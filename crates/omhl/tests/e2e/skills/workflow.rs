//! A prosa das skills de workflow (`interview`, `plan-consensus`, `review`) e do `cycle.md`: a entrevista antes
//! do work item, o caminho leve e a chamada dos agentes sem `name`. É texto de skill: o teste prova que cada
//! regra está escrita, não o comportamento do agente. Nada no binário impõe a entrevista.

use std::fs;

use crate::skills::coverage::{leaf_commands, read_docs, skills_dir};
use crate::skills::orchestration::{line_with, numbered_steps, read, step_with};
use crate::support::repo_checks::repo_root;

const OPTIONAL_PREFIXES: [&str; 5] = ["refactor", "tests", "bugfix", "chore", "docs"];

/// A razão medida pelo G1 para nunca chamar `oh-my-hexflow:*` com `name`.
const NAME_REASON: [&str; 4] = [
    "`name`",
    "sem a matriz `omhl-allow`",
    "sem a vista pré-carregada",
    "`claude`",
];

/// As skills de CLI que viraram páginas de `directives-flow` e `review`.
const DISSOLVED_SKILLS: [&str; 3] = [
    "query-and-integrity",
    "mechanical-checks",
    "work-item-closing",
];

/// Cópia de `BASE_LEAVES` de `src/test_support/role_view/who_uses.rs`: as folhas da base não levam o bloco.
const BASE_LEAVES: [&str; 5] = [
    "append",
    "spec init",
    "spec validate",
    "gate-worktree",
    "guideline retire",
];

const WHO_USES_HEADING: &str = "## Quem usa, em qual etapa e com qual filtro";

/// A ordem do fechamento no passo 8 da `review`: a parada do PR filho vem antes do primeiro `check run`.
const CLOSING_ORDER: [&str; 7] = [
    "pare depois de colar a tabela",
    "omhl check run --event before-merge",
    "omhl attest",
    "omhl gate-pr",
    "`wt",
    "omhl close",
    "omhl report retro",
];

/// O texto da skill em minúsculas: o critério vale para a palavra, não para a caixa.
fn lowercase(relative: &str) -> String {
    read(relative).to_lowercase()
}

/// As linhas da seção `## <heading>`, até o próximo título `## `.
fn section(text: &str, heading: &str) -> Vec<String> {
    text.lines()
        .skip_while(|line| !line.starts_with(&format!("## {heading}")))
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn it_should_state_the_ambiguity_gate_and_the_pre_work_item_order_in_the_interview() {
    let text = lowercase("interview/SKILL.md");

    assert!(
        line_with(&text, &["placar", "dimensão", "ambiguidade"]).is_some(),
        "a scoreboard of ambiguity per dimension"
    );
    assert!(
        line_with(&text, &["ambiguidade", "20%", "ou menos"]).is_some(),
        "the interview stops at 20% ambiguity or less"
    );
    assert!(
        line_with(&text, &["antes do work item"]).is_some(),
        "the interview comes before the work item"
    );
}

#[test]
fn it_should_keep_the_interview_notes_out_of_the_database() {
    let text = read("interview/SKILL.md");
    let gitignore =
        fs::read_to_string(repo_root().join(".gitignore")).expect("the root .gitignore");

    assert!(
        line_with(&text, &[".omhl/notes/interview-", "não vira registro"]).is_some(),
        "the round state lives in .omhl/notes and never becomes a record"
    );
    assert!(
        gitignore.lines().any(|line| line.trim() == ".omhl/notes/"),
        "the notes folder is ignored by git"
    );
}

#[test]
fn it_should_require_the_interview_for_feat_and_fix_and_list_the_five_optional_prefixes() {
    let text = read("interview/SKILL.md");

    let required = line_with(&text, &["Obrigatória", "`feat`", "`fix`"])
        .expect("a line that makes the interview mandatory for feat and fix");
    let optional = line_with(&text, &["Opcional", "a pedido do dono"])
        .expect("a line that makes the interview optional, on request");

    let required_line = text.lines().nth(required).unwrap_or_default();
    let optional_line = text.lines().nth(optional).unwrap_or_default();
    for prefix in OPTIONAL_PREFIXES {
        assert!(
            optional_line.contains(&format!("`{prefix}`")),
            "{prefix} is optional"
        );
        assert!(
            !required_line.contains(&format!("`{prefix}`")),
            "{prefix} is not mandatory"
        );
    }
    for mandatory in ["feat", "fix"] {
        assert!(
            !optional_line.contains(&format!("`{mandatory}`")),
            "{mandatory} is not optional"
        );
    }
}

#[test]
fn it_should_tell_the_analyst_step_to_cover_only_the_technical_after_the_interview() {
    let text = read("plan-consensus/SKILL.md");

    let step = step_with(
        &text,
        &[
            "oh-my-hexflow:analyst",
            "negócio já confirmado na entrevista; cubra só o técnico",
        ],
    )
    .expect("an analyst step that hands over the confirmed business");

    let line = &numbered_steps(&text)[step];
    assert!(
        line.contains("`feat`") && line.contains("`fix`"),
        "the instruction applies to feat and fix"
    );
}

#[test]
fn it_should_describe_the_light_path_for_chore_and_docs_without_planner_or_rounds() {
    let text = read("plan-consensus/SKILL.md");
    let heading = text
        .lines()
        .position(|line| line.starts_with("## Caminho leve"))
        .expect("a light path section");

    let last_step = numbered_steps(&text).pop().expect("numbered steps");
    let last_step_line = text
        .lines()
        .position(|line| line == last_step)
        .expect("the last numbered step is in the text");
    let light_path = section(&text, "Caminho leve").join("\n");

    assert!(
        last_step_line < heading,
        "the light path comes after the numbered list"
    );
    for needle in [
        "`chore`",
        "`docs`",
        "`omhl gate-worktree`",
        "sem planner nem voltas",
    ] {
        assert!(light_path.contains(needle), "the light path cites {needle}");
    }
}

#[test]
fn it_should_offer_the_challenge_modes_only_when_the_owner_asks() {
    let text = read("interview/SKILL.md");

    let heading = text
        .lines()
        .find(|line| line.starts_with("## Modos de desafio"))
        .expect("a challenge modes section");
    let modes = section(&text, "Modos de desafio").join("\n");

    assert!(
        heading.contains("só quando o dono pedir"),
        "heading: {heading}"
    );
    for mode in ["Contrarian", "Simplifier", "Ontologist"] {
        assert!(modes.contains(mode), "the section describes {mode}");
    }
    assert!(modes.contains("no máximo uma vez"), "each mode runs once");
}

#[test]
fn it_should_tell_the_session_to_call_plugin_agents_without_name() {
    let rules = [
        (
            "plan-consensus/SKILL.md",
            ["oh-my-hexflow:*", "**sem** `name`"],
        ),
        (
            "review/SKILL.md",
            ["oh-my-hexflow:<papel>", "**sem** `name`"],
        ),
        (
            "directives-flow/references/cycle.md",
            ["oh-my-hexflow:*", "sem o parâmetro `name`"],
        ),
    ];

    for (file, instruction) in rules {
        let text = read(file);
        let needles: Vec<&str> = instruction.into_iter().chain(NAME_REASON).collect();

        assert!(
            line_with(&text, &needles).is_some(),
            "{file} must tell the session to call plugin agents without name and say why"
        );
    }
}

#[test]
fn it_should_not_ship_the_dissolved_cli_skills() {
    for skill in DISSOLVED_SKILLS {
        assert!(
            !skills_dir().join(skill).exists(),
            "{skill} must live on as pages of directives-flow and review"
        );
    }
}

#[test]
fn it_should_open_each_migrated_leaf_page_with_the_who_uses_block() {
    let docs = read_docs(&skills_dir());
    let migrated: Vec<String> = leaf_commands()
        .into_iter()
        .filter(|leaf| !BASE_LEAVES.contains(&leaf.as_str()))
        .collect();

    let without_block: Vec<&str> = migrated
        .iter()
        .filter(|leaf| {
            let file = format!("/references/{}.md", leaf.replace(' ', "-"));
            let first_title = docs
                .iter()
                .find(|doc| doc.path.ends_with(&file))
                .and_then(|doc| doc.text.lines().find(|line| line.starts_with("## ")));

            first_title != Some(WHO_USES_HEADING)
        })
        .map(String::as_str)
        .collect();

    assert!(!migrated.is_empty(), "the CLI has migrated leaves");
    assert!(
        without_block.is_empty(),
        "pages whose first section is not the who-uses block: {without_block:?}"
    );
}

#[test]
fn it_should_keep_the_closing_order_in_review_step_8() {
    let text = read("review/SKILL.md");
    let step = numbered_steps(&text)
        .into_iter()
        .find(|step| step.starts_with("8. "))
        .expect("review has a step 8");

    let positions: Vec<usize> = CLOSING_ORDER
        .iter()
        .map(|needle| {
            step.find(needle)
                .unwrap_or_else(|| panic!("step 8 must say {needle}"))
        })
        .collect();

    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "step 8 must keep the closing order {CLOSING_ORDER:?}, found at {positions:?}"
    );
}
