//! A prosa das skills de workflow (`interview`, `plan-consensus`, `review`) e do `cycle.md`: a entrevista antes
//! do work item, o caminho leve e a chamada dos agentes sem `name`. É texto de skill: o teste prova que cada
//! regra está escrita, não o comportamento do agente. Nada no binário impõe a entrevista.

use std::fs;
use std::path::Path;

use omhl::native::NativeBase;
use omhl::workflow::config::parse_workflow;

use crate::skills::coverage::{leaf_commands, read_docs, skills_dir};
use crate::skills::orchestration::{line_with, numbered_steps, read, step_with};
use crate::support::repo_checks::repo_root;

const OPTIONAL_PREFIXES: [&str; 5] = ["refactor", "tests", "bugfix", "chore", "docs"];

/// A razão medida pelo G1 para nunca chamar `oh-my-hexflow:*` com `name`.
const NAME_REASON: [&str; 4] = [
    "`name`",
    "without the `omhl-allow` matrix",
    "without the preloaded view",
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

const WHO_USES_HEADING: &str = "## Who uses it, at which stage and with which filter";

/// A ordem do fechamento no passo 8 da `review`.
const CLOSING_ORDER: [&str; 6] = [
    "omhl check run --event before-merge",
    "omhl attest",
    "omhl gate-delivery",
    "omhl merge",
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
        line_with(&text, &["scoreboard", "dimension", "ambiguity"]).is_some(),
        "a scoreboard of ambiguity per dimension"
    );
    assert!(
        line_with(&text, &["ambiguity", "20%", "or less"]).is_some(),
        "the interview stops at 20% ambiguity or less"
    );
    assert!(
        line_with(&text, &["before the work item"]).is_some(),
        "the interview comes before the work item"
    );
}

#[test]
fn it_should_keep_the_interview_notes_out_of_the_database() {
    let text = read("interview/SKILL.md");
    let gitignore =
        fs::read_to_string(repo_root().join(".gitignore")).expect("the root .gitignore");

    assert!(
        line_with(
            &text,
            &[".omhl/notes/interview-", "does not become a record"]
        )
        .is_some(),
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

    let required = line_with(&text, &["Mandatory", "`feat`", "`fix`"])
        .expect("a line that makes the interview mandatory for feat and fix");
    let optional = line_with(&text, &["Optional", "at the owner's request"])
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
            "business already confirmed in the interview; cover only the technical",
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
        .position(|line| line.starts_with("## Light path"))
        .expect("a light path section");

    let last_step = numbered_steps(&text).pop().expect("numbered steps");
    let last_step_line = text
        .lines()
        .position(|line| line == last_step)
        .expect("the last numbered step is in the text");
    let light_path = section(&text, "Light path").join("\n");

    assert!(
        last_step_line < heading,
        "the light path comes after the numbered list"
    );
    for needle in [
        "`chore`",
        "`docs`",
        "`omhl gate-worktree`",
        "no planner or rounds",
    ] {
        assert!(light_path.contains(needle), "the light path cites {needle}");
    }
}

#[test]
fn it_should_offer_the_challenge_modes_only_when_the_owner_asks() {
    let text = read("interview/SKILL.md");

    let heading = text
        .lines()
        .find(|line| line.starts_with("## Challenge modes"))
        .expect("a challenge modes section");
    let modes = section(&text, "Challenge modes").join("\n");

    assert!(
        heading.contains("only when the owner asks"),
        "heading: {heading}"
    );
    for mode in ["Contrarian", "Simplifier", "Ontologist"] {
        assert!(modes.contains(mode), "the section describes {mode}");
    }
    assert!(modes.contains("at most once"), "each mode runs once");
}

#[test]
fn it_should_tell_the_session_to_call_plugin_agents_without_name() {
    let rules = [
        (
            "plan-consensus/SKILL.md",
            ["oh-my-hexflow:*", "**without** `name`"],
            NAME_REASON,
        ),
        (
            "review/SKILL.md",
            ["oh-my-hexflow:<role>", "**without** `name`"],
            NAME_REASON,
        ),
        (
            "directives-flow/references/cycle.md",
            ["oh-my-hexflow:*", "without the `name` parameter"],
            NAME_REASON,
        ),
    ];

    for (file, instruction, reason) in rules {
        let text = read(file);
        let needles: Vec<&str> = instruction.into_iter().chain(reason).collect();

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

fn read_agent(name: &str) -> String {
    fs::read_to_string(repo_root().join(format!("plugin/agents/{name}.md")))
        .unwrap_or_else(|error| panic!("{name}.md must exist and be readable: {error}"))
}

#[test]
fn it_should_open_the_interview_with_round_zero_parts_locked_in_the_note_and_ask_the_owner_name() {
    let text = read("interview/SKILL.md");

    assert!(
        line_with(
            &text,
            &[
                "Round 0",
                "confirm, merge, split or postpone",
                "`Parts` section",
                "does not change it"
            ]
        )
        .is_some(),
        "round 0 lists the parts and locks them in the Parts section of the note"
    );
    assert!(
        line_with(&text, &["`person:<name>`", "Round 0 or in round 1"]).is_some(),
        "the owner name is asked in round 0 or round 1"
    );
}

#[test]
fn it_should_draft_and_confirm_one_premise_per_round_with_the_proof_in_the_confirmed_by() {
    let text = read("interview/SKILL.md");

    assert!(
        line_with(
            &text,
            &[
                "draft the business premise",
                "confirm or correct it",
                "in the same round"
            ]
        )
        .is_some(),
        "each round drafts the premise and the owner confirms or corrects it"
    );
    assert!(
        line_with(
            &text,
            &[
                "`confirmed_by`",
                "the premise as you drafted it",
                "confirmation or correction",
                "2000 characters"
            ]
        )
        .is_some(),
        "the confirmed_by carries the drafted premise and the owner's literal answer, up to 2000 characters"
    );
    assert!(
        line_with(&text, &["literal question and answer, in full"]).is_some(),
        "the note keeps the whole literal question and answer of the round"
    );
}

#[test]
fn it_should_close_the_interview_only_with_low_ambiguity_and_every_active_part_covered() {
    let text = read("interview/SKILL.md");

    assert!(
        line_with(
            &text,
            &[
                "20% or less",
                "20% by default",
                "each active part of Round 0",
                "confirmed business premise"
            ]
        )
        .is_some(),
        "the stop rule needs the ambiguity and the coverage"
    );
    assert!(
        line_with(
            &text,
            &[
                "Coverage",
                "record in the note",
                "resuming after compaction"
            ]
        )
        .is_some(),
        "the coverage is recorded per round in the note"
    );
}

#[test]
fn it_should_write_both_interview_batches_at_the_end_without_a_batch_confirmation() {
    let text = read("interview/SKILL.md");

    let step = step_with(
        &text,
        &[
            "batch 2",
            "idempotency `key`",
            "**without** a new batch confirmation round",
        ],
    )
    .expect("a step that writes batch 2 without a new confirmation");
    let line = &numbered_steps(&text)[step];

    assert!(
        line.contains("record only it again"),
        "a failed batch 2 is rewritten alone"
    );
    assert!(
        line_with(&text, &["batch 1", "idempotency `key`"]).is_some(),
        "batch 1 carries a key too"
    );
}

#[test]
fn it_should_mark_every_business_suggestion_after_the_interview_as_late_in_plan_consensus_and_the_agents()
 {
    let text = read("plan-consensus/SKILL.md");

    let analyst_step = step_with(
        &text,
        &["oh-my-hexflow:analyst", "late (`origin: late`, `assumed`)"],
    )
    .expect("an analyst step that marks the business suggestion as late");
    assert!(
        !numbered_steps(&text)[analyst_step].contains("goes to the owner afterwards"),
        "the analyst step no longer sends the business premise to the owner afterwards"
    );
    assert!(
        line_with(
            &text,
            &["The planner does not cite in the spec a late premise that is still `assumed`"]
        )
        .is_some(),
        "the planner does not cite a late premise that is still assumed"
    );
    for agent in ["analyst", "planner", "critic"] {
        assert!(
            read_agent(agent).contains("origin"),
            "{agent}.md says the business suggestion is late"
        );
    }
    assert!(
        line_with(
            &read_agent("planner"),
            &["Never cite in the spec a late premise that is still `assumed`"]
        )
        .is_some(),
        "planner.md forbids citing a late premise that is still assumed"
    );
}

#[test]
fn it_should_give_the_confirmed_late_premise_two_destinations_around_the_spec_freeze() {
    let needles = [
        "until the spec is frozen",
        "reopens the plan",
        "counts in the 3",
        "the impasse goes to the owner",
        "after the freeze",
        "`divergence`",
        "`amendment-NN`",
    ];
    for file in [
        "plan-consensus/SKILL.md",
        "directives-flow/SKILL.md",
        "directives-flow/references/cycle.md",
    ] {
        let text = read(file);

        assert!(
            line_with(&text, &needles).is_some(),
            "{file} must say both destinations of the confirmed late premise"
        );
    }
}

#[test]
fn it_should_ask_review_and_verifier_for_one_line_per_confirmed_premise_with_evidence() {
    let review = read("review/SKILL.md");
    let verifier = read_agent("verifier");
    let view = read("role-verifier/SKILL.md");
    let command =
        "omhl list --type premise --where work_item=work-items.<id> --where status=confirmed";

    assert!(
        line_with(
            &review,
            &[
                command,
                "`next_after_seq`",
                "one line per premise",
                "evidence in the delivery"
            ]
        )
        .is_some(),
        "review step 4 asks for one line per confirmed premise, read to the last page"
    );
    assert!(
        line_with(
            &verifier,
            &[
                "--where status=confirmed",
                "`next_after_seq`",
                "one row per premise",
                "evidence in the delivery"
            ]
        )
        .is_some(),
        "verifier.md asks for one row per confirmed premise with evidence"
    );
    assert!(
        view.contains(command),
        "the role-verifier view lists the read command"
    );
}

#[test]
fn it_should_fail_a_premise_without_evidence_and_open_a_missing_divergence_on_its_id() {
    let review = read("review/SKILL.md");
    let verifier = read_agent("verifier");

    assert!(
        line_with(
            &review,
            &[
                "A confirmed premise without evidence",
                "`FAIL`",
                "`divergence` `open`",
                "`source_ref` equal to the premise id",
                "`kind` `missing`"
            ]
        )
        .is_some(),
        "review says a premise without evidence fails and opens a missing divergence"
    );
    assert!(
        line_with(
            &verifier,
            &[
                "A premise without evidence is a FAIL",
                "`source_ref` equal to the id of the premise",
                "`kind` `missing`"
            ]
        )
        .is_some(),
        "verifier.md says a premise without evidence fails and opens a missing divergence"
    );
}

const NOTES_WRITE_REASON: [&str; 3] = [
    "with the `Write` tool",
    "the hook does not see what `Bash` writes",
    "`Write` is where the folder gets its `.gitignore`",
];

#[test]
fn it_should_tell_the_interview_to_write_its_state_with_the_write_tool_and_say_why() {
    let text = read("interview/SKILL.md");
    let mut needles = vec![".omhl/notes/interview-<slug>.md", "in the main checkout"];
    needles.extend(NOTES_WRITE_REASON);

    let line = line_with(&text, &needles);

    assert!(line.is_some(), "the state is written with Write, and why");
}

#[test]
fn it_should_tell_the_plan_consensus_retirements_note_to_use_write_and_say_why() {
    let text = read("plan-consensus/SKILL.md");
    let mut needles = vec![
        "<main checkout>/.omhl/notes/<id>-retiradas.md",
        "absolute path",
    ];
    needles.extend(&NOTES_WRITE_REASON[1..]);
    needles.push("use `Write`, not `Bash`");

    let step = step_with(&text, &needles);

    assert!(step.is_some(), "step 8 writes the note with Write, and why");
}

#[test]
fn it_should_tell_the_review_retirements_note_to_use_write_and_say_why() {
    let text = read("review/SKILL.md");
    let mut needles = vec![
        "<main checkout>/.omhl/notes/<id>-retiradas.md",
        "absolute path",
    ];
    needles.extend(&NOTES_WRITE_REASON[1..]);
    needles.push("use `Write`, not `Bash`");

    let step = step_with(&text, &needles);

    assert!(step.is_some(), "step 4 writes the note with Write, and why");
}

const NATIVE_CHECKS_PAGE: &str = "directives-flow/references/native-checks.md";

#[test]
fn it_should_cite_the_native_ids_and_the_steps_for_a_project_without_git_in_native_checks_md() {
    let text = read(NATIVE_CHECKS_PAGE);
    let mut needles: Vec<&str> = NativeBase.checks().iter().map(|check| check.id).collect();
    needles.extend(["main-dir-unknown", "git init", "first commit"]);

    let missing: Vec<&str> = needles
        .into_iter()
        .filter(|needle| !text.contains(needle))
        .collect();

    assert!(
        missing.is_empty(),
        "native-checks.md does not cite {missing:?}"
    );
}

#[test]
fn it_should_give_native_checks_md_toml_examples_that_parse_as_a_workflow() {
    let text = read(NATIVE_CHECKS_PAGE);
    let examples: Vec<&str> = text
        .split("```toml\n")
        .skip(1)
        .filter_map(|block| block.split("```").next())
        .collect();

    assert!(
        !examples.is_empty(),
        "native-checks.md has a workflow.toml example"
    );
    for example in examples {
        let parsed = parse_workflow(Path::new(".omhl/workflow.toml"), example.as_bytes());

        assert!(parsed.is_ok(), "the example is not accepted:\n{example}");
    }
}

#[test]
fn it_should_link_native_checks_md_from_the_directives_flow_skill() {
    let text = read("directives-flow/SKILL.md");

    assert!(
        text.contains("](references/native-checks.md)"),
        "SKILL.md links native-checks.md"
    );
}

const CLOSE_PAGE: &str = "review/references/close.md";
const GATE_DELIVERY_PAGE: &str = "review/references/gate-delivery.md";
const CHECK_RETIRE_PAGE: &str = "review/references/check-retire.md";
const APPEND_PAGE: &str = "directives-flow/references/append.md";
const LOAD_PAGE: &str = "review/references/load.md";

/// As páginas do `review` que só apontam para a dona de cada regra de fechamento, mais a dona.
const CLOSING_PAGES: [&str; 4] = [
    "review/SKILL.md",
    "review/references/fechamento.md",
    "review/references/checks.md",
    CLOSE_PAGE,
];

fn assert_page_says(page: &str, phrase: &str) {
    let text = lowercase(page);

    assert!(
        text.contains(&phrase.to_lowercase()),
        "{page} must say: {phrase}"
    );
}

#[test]
fn it_should_say_in_close_md_that_close_runs_at_the_tip_of_the_work_item_branch() {
    assert_page_says(CLOSE_PAGE, "tip of the work item branch");
}

#[test]
fn it_should_say_in_close_md_to_check_the_commit_before_any_other_exit() {
    assert_page_says(CLOSE_PAGE, "before any other exit");
}

#[test]
fn it_should_say_in_close_md_that_no_commit_goes_after_the_merge() {
    assert_page_says(CLOSE_PAGE, "no commit after the merge");
}

#[test]
fn it_should_say_in_close_md_that_a_merged_item_never_closes_with_abandon() {
    assert_page_says(CLOSE_PAGE, "never closes with `--abandon`");
}

#[test]
fn it_should_say_in_close_md_that_a_merged_item_closes_with_failed_delivery_and_a_note() {
    assert_page_says(CLOSE_PAGE, "`--failed-delivery --note`");
}

#[test]
fn it_should_say_in_close_md_which_merges_failed_delivery_does_not_cover() {
    assert_page_says(
        CLOSE_PAGE,
        "squash or rebase merges or a project without `branch.integration`",
    );
}

#[test]
fn it_should_say_in_close_md_that_abandon_reason_tells_how_the_merge_happened() {
    assert_page_says(CLOSE_PAGE, "`--reason` says how the merge happened");
}

#[test]
fn it_should_say_in_close_md_that_force_does_not_help_in_close_refused() {
    assert_page_says(CLOSE_PAGE, "`--force` does not help in `close-refused`");
}

#[test]
fn it_should_say_in_close_md_that_force_note_is_only_for_close_warnings() {
    assert_page_says(CLOSE_PAGE, "`--force --note` only for `close-warnings`");
}

#[test]
fn it_should_say_in_close_md_that_the_note_holds_the_owner_phrase() {
    assert_page_says(CLOSE_PAGE, "the owner's literal phrase in the `--note`");
}

#[test]
fn it_should_say_in_close_md_that_abandon_takes_reason_and_note() {
    assert_page_says(CLOSE_PAGE, "`--abandon --reason --note`");
}

#[test]
fn it_should_say_in_close_md_that_the_retro_is_required_only_after_failed_delivery() {
    assert_page_says(
        CLOSE_PAGE,
        "the retro is required only after `--failed-delivery`",
    );
}

#[test]
fn it_should_say_in_close_md_to_propose_one_improvement_per_failed_refuse_question() {
    assert_page_says(CLOSE_PAGE, "one improvement per failed `refuse` question");
}

#[test]
fn it_should_say_in_close_md_that_abandon_has_no_required_retro() {
    assert_page_says(CLOSE_PAGE, "`--abandon` has no required retro");
}

#[test]
fn it_should_say_in_gate_delivery_md_to_pass_the_printed_merge_base_as_base() {
    assert_page_says(GATE_DELIVERY_PAGE, "the merge-base it prints");
}

#[test]
fn it_should_say_in_check_retire_md_that_retiring_a_check_is_the_owners_task() {
    assert_page_says(CHECK_RETIRE_PAGE, "retiring a check is the owner's task");
}

#[test]
fn it_should_say_in_append_md_to_record_the_type_when_creating_the_work_item() {
    assert_page_says(APPEND_PAGE, "record the `type` when creating the work item");
}

#[test]
fn it_should_say_in_load_md_to_edit_workflow_and_checks_only_inside_a_worktree() {
    assert_page_says(LOAD_PAGE, "only inside a worktree");
}

#[test]
fn it_should_not_say_in_review_pages_that_close_closes_with_a_failed_gate() {
    for page in ["review/SKILL.md", CLOSE_PAGE] {
        let text = lowercase(page);

        assert!(
            !text.contains("closes even with the gate failed")
                && !text.contains("closing is recorded anyway"),
            "{page} still says close closes with a failed gate"
        );
    }
}

#[test]
fn it_should_not_tell_the_agent_in_review_pages_to_retire_a_check() {
    let skill = read("review/SKILL.md");
    let checks = read("review/references/checks.md");

    assert!(
        !skill.contains("Run `omhl check retire`")
            && !checks.contains("`omhl check retire` only when"),
        "review/SKILL.md and checks.md must leave retiring to the owner"
    );
}

#[test]
fn it_should_not_teach_recreating_the_branch_or_the_worktree_after_the_merge() {
    for page in CLOSING_PAGES {
        let text = lowercase(page);

        assert!(
            !text.contains("recreate the branch") && !text.contains("recreate the worktree"),
            "{page} must not teach recreating the branch or the worktree"
        );
    }
}

#[test]
fn it_should_turn_the_loosening_review_on_in_the_workflow_of_this_repository() {
    let path = repo_root().join(".omhl/workflow.toml");
    let bytes = std::fs::read(&path).expect("the repository workflow is readable");

    let parsed = parse_workflow(&path, &bytes);

    assert!(parsed.is_ok(), "the workflow is not accepted: {parsed:?}");
    let table: toml::Table =
        toml::from_str(&String::from_utf8(bytes).expect("UTF-8")).expect("the workflow is TOML");
    let checks = table["native"]["checks"].as_array().expect("native checks");
    assert!(
        checks
            .iter()
            .any(|id| id.as_str() == Some("checks.config-loosening-review"))
    );
}

#[test]
fn it_should_ship_the_command_pages_that_own_the_close_refusals_and_options() {
    let pages = [
        "review/references/close.md",
        "review/references/gate-delivery.md",
        "review/references/load.md",
        "review/references/check-retire.md",
        "directives-flow/references/append.md",
        "directives-flow/references/errors.md",
    ];

    let missing: Vec<&str> = pages
        .into_iter()
        .filter(|page| !skills_dir().join(page).is_file())
        .collect();

    assert_eq!(missing, Vec::<&str>::new());
}

fn all_files(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            all_files(&path, found);
        } else {
            found.push(path);
        }
    }
}

#[test]
fn it_should_make_the_interview_threshold_and_weights_configurable_through_the_workflow_table() {
    let text = read("interview/SKILL.md");

    for needles in [
        &["`[interview]`", "20% by default", ".omhl/workflow.toml"][..],
        &["0.35", "`objective`"],
        &["0.25", "`constraints`"],
        &["0.25", "`success`"],
        &["0.15", "`context`"],
    ] {
        assert!(
            line_with(&text, needles).is_some(),
            "interview/SKILL.md must carry {needles:?}"
        );
    }
}

#[test]
fn it_should_drop_the_fixed_threshold_and_fixed_question_shape_from_the_interview() {
    let text = read("interview/SKILL.md");

    for gone in [
        "The threshold is fixed",
        "One per message",
        "2 to 4 options",
    ] {
        assert!(
            !text.contains(gone),
            "interview/SKILL.md must not say {gone:?}"
        );
    }
}

#[test]
fn it_should_word_the_interview_stop_rule_with_the_configurable_threshold() {
    let text = read("interview/SKILL.md");

    assert!(
        text.contains("ends with ambiguity of 20% or less (20% by default; the `[interview]` table of the main checkout `.omhl/workflow.toml` changes it)"),
        "the stop rule names the default and the table that changes it"
    );
}

#[test]
fn it_should_call_the_interview_threshold_a_default_in_cycle_md_and_the_directives_doc() {
    let cycle = read("directives-flow/references/cycle.md");
    let doc = fs::read_to_string(repo_root().join("docs/directives-flow.md")).expect("the doc");

    assert!(
        line_with(&cycle, &["until the ambiguity reaches 20% by default"]).is_some(),
        "cycle.md calls 20% a default"
    );
    assert!(
        line_with(&doc, &["a ambiguidade chegar a 20% por padrão"]).is_some(),
        "the directives doc calls 20% a default"
    );
}

#[test]
fn it_should_keep_model_names_out_of_skills_and_docs() {
    let mut files = Vec::new();
    all_files(&repo_root().join("plugin/skills"), &mut files);
    all_files(&repo_root().join("plugin/docs"), &mut files);

    let offenders: Vec<_> = files
        .iter()
        .filter(|path| {
            fs::read_to_string(path).is_ok_and(|text| {
                let lower = text.to_lowercase();
                lower.contains("opus") || lower.contains("sonnet")
            })
        })
        .collect();

    assert!(offenders.is_empty(), "model names found in {offenders:?}");
}

#[test]
fn it_should_keep_the_model_line_on_line_four_of_every_agent() {
    for name in [
        "analyst",
        "architect",
        "code-reviewer",
        "critic",
        "planner",
        "security-reviewer",
        "verifier",
    ] {
        let text = read_agent(name);

        assert!(
            text.lines()
                .nth(3)
                .is_some_and(|line| line.starts_with("model:")),
            "{name}.md has `model:` on line 4"
        );
    }
}

#[test]
fn it_should_say_the_agent_model_is_a_default_the_caller_may_override() {
    for file in [
        "directives-flow/references/roles.md",
        "plan-consensus/SKILL.md",
    ] {
        let text = read(file);

        assert!(
            text.contains("default") && text.contains("may pass another model"),
            "{file} says the `model:` line is a default and the caller may pass another model"
        );
    }
}

#[test]
fn it_should_not_name_child_prs_aggregator_prs_or_stacking_in_the_review_skill() {
    let skill_dir = skills_dir().join("review");
    let mut pages = vec![skill_dir.join("SKILL.md")];
    pages.extend(
        fs::read_dir(skill_dir.join("references"))
            .expect("review has references")
            .map(|entry| entry.expect("readable entry").path()),
    );

    let offending: Vec<String> = pages
        .iter()
        .flat_map(|page| {
            let text = fs::read_to_string(page)
                .expect("readable page")
                .to_lowercase();
            ["child pr", "aggregator pr", "stacked"]
                .into_iter()
                .filter(move |term| text.contains(term))
                .map(move |term| format!("{}: {term}", page.display()))
        })
        .collect();

    assert_eq!(offending, Vec::<String>::new());
}

#[test]
fn it_should_say_the_findings_table_is_method_and_its_template_is_a_changeable_default() {
    let text = lowercase("review/SKILL.md");

    assert!(text.contains("findings table is part of the method"));
    assert!(text.contains("template is a default the project can change"));
}

#[test]
fn it_should_describe_the_child_and_aggregator_pr_flow_in_contributing() {
    let contributing =
        fs::read_to_string(repo_root().join("CONTRIBUTING.md")).expect("CONTRIBUTING.md");

    let heading = contributing
        .lines()
        .find(|line| line.starts_with("## ") && line.contains("PR filho"))
        .expect("CONTRIBUTING.md has a section about the child PR");
    let section = section(&contributing, &heading[3..]).join("\n");

    assert!(section.contains("gate-delivery"));
    assert!(section.contains("close"));
}

#[test]
fn it_should_leave_docs_adr_untouched_by_the_work_item() {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "develop", "--", "docs/adr/"])
        .current_dir(repo_root())
        .output()
        .expect("git diff runs");

    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "");
}
