//! As skills de orquestração (`plan-consensus` e `review`) e as referências novas da base `directives-flow`
//! (`cycle.md`, `roles.md`, `tracking-table.md`). São texto de skill: o teste prova a presença de cada regra,
//! não o comportamento do agente. Os limites de linhas, as citações `omhl ...` e os caminhos `.md` já são
//! conferidos pelo `coverage.rs` sobre todas as skills, as novas incluídas.

use std::fs;

use omhl::guard::ROLE_REFERENCE_COMMANDS;

use crate::skills::coverage::skills_dir;

const BASE_SENTENCE: &str = "This skill requires `directives-flow`. If it is not loaded, load it";

/// `omhl:analyst` -> `analyst`; os sete papéis vêm da constante de referência da guarda.
fn reference_roles() -> Vec<&'static str> {
    ROLE_REFERENCE_COMMANDS
        .iter()
        .map(|(role, _)| {
            role.strip_prefix("omhl:")
                .expect("reference roles carry the omhl: prefix")
        })
        .collect()
}

pub(super) fn read(relative: &str) -> String {
    fs::read_to_string(skills_dir().join(relative))
        .unwrap_or_else(|error| panic!("{relative} must exist and be readable: {error}"))
}

fn base_reference(name: &str) -> String {
    read(&format!("directives-flow/references/{name}"))
}

pub(super) fn line_with(text: &str, needles: &[&str]) -> Option<usize> {
    text.lines()
        .position(|line| needles.iter().all(|needle| line.contains(needle)))
}

/// A linha do item de lista que começa por `prefix` (a conferência da mesma frase em outra linha não conta).
fn list_item(text: &str, prefix: &str) -> Option<usize> {
    text.lines()
        .position(|line| line.trim_start().starts_with(prefix))
}

/// O texto depois do frontmatter: a `description` cita comandos e não conta como passo.
fn body(skill: &str) -> String {
    skill
        .splitn(3, "---\n")
        .nth(2)
        .unwrap_or_default()
        .to_owned()
}

fn first_line_after_heading(skill: &str) -> String {
    skill
        .lines()
        .skip_while(|line| !line.starts_with("# "))
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn it_should_require_the_directives_flow_base_in_plan_consensus_and_in_review() {
    for skill in ["plan-consensus", "review"] {
        let text = read(&format!("{skill}/SKILL.md"));

        assert!(
            first_line_after_heading(&text).starts_with(BASE_SENTENCE),
            "{skill} must open with the base requirement"
        );
    }
}

#[test]
fn it_should_give_the_new_skills_the_house_frontmatter() {
    for skill in ["plan-consensus", "review", "interview"] {
        let text = read(&format!("{skill}/SKILL.md"));
        let lines: Vec<&str> = text.lines().collect();

        assert_eq!(lines[0], "---", "{skill} frontmatter opens");
        assert_eq!(lines[1], format!("name: {skill}"));
        assert!(
            lines[2].starts_with("description: Protocol "),
            "{skill} description"
        );
        assert_eq!(lines[3], "---", "{skill} frontmatter closes");
    }
}

#[test]
fn it_should_state_the_three_round_limit_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");

    assert!(
        line_with(&text, &["3 rounds"]).is_some(),
        "the three-round limit"
    );
}

#[test]
fn it_should_escalate_the_impasse_to_the_owner_on_the_third_round_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");

    assert!(
        line_with(&text, &["third", "impasse", "owner"]).is_some(),
        "on the third round without acceptance, stop and show the impasse to the owner"
    );
}

#[test]
fn it_should_call_the_verifier_before_spec_validate_in_plan_consensus() {
    let text = body(&read("plan-consensus/SKILL.md"));

    let verifier = line_with(&text, &["oh-my-hexflow:verifier", "isolated context"])
        .expect("a step that calls the verifier in an isolated context");
    let validate =
        line_with(&text, &["run `omhl spec validate"]).expect("the final spec validate step");

    assert!(verifier < validate, "the verifier step comes first");
    assert!(
        text.lines()
            .nth(verifier)
            .is_some_and(|line| line.contains("contradicts")),
        "the verifier step retires the resolved contradicts"
    );
}

#[test]
fn it_should_call_the_verifier_in_an_isolated_context_before_gate_pr_in_review() {
    let text = body(&read("review/SKILL.md"));

    let verifier = line_with(&text, &["oh-my-hexflow:verifier", "isolated context"])
        .expect("a step that calls the verifier in an isolated context");
    let gate = line_with(&text, &["omhl gate-pr"]).expect("a step that cites gate-pr");

    assert!(verifier < gate, "the verifier step comes first");
}

#[test]
fn it_should_carry_the_ten_cycle_steps_in_cycle_md() {
    let cycle = base_reference("cycle.md");

    for step in 1..=10 {
        assert!(
            cycle
                .lines()
                .any(|line| line.starts_with(&format!("{step}. "))),
            "cycle.md has step {step}"
        );
    }
}

#[test]
fn it_should_carry_the_critic_confirmation_and_anchor_rules_in_roles_md() {
    let roles = base_reference("roles.md");

    assert!(
        roles.contains("confirmed_by"),
        "roles.md has the critic confirmation rule"
    );
    assert!(roles.contains("rests-on"), "roles.md has the anchor rule");
}

#[test]
fn it_should_carry_the_heading_legend_and_totals_in_the_tracking_table_template() {
    let template = base_reference("tracking-table.md");

    assert!(template.contains("## Tracking"), "the template heading");
    assert!(template.contains("**Legend:**"), "the template legend");
    assert!(template.contains("**Totals ("), "the template totals line");
}

#[test]
fn it_should_state_that_the_override_replaces_the_default_template() {
    let template = base_reference("tracking-table.md");
    let review = read("review/SKILL.md");

    assert!(
        line_with(
            &template,
            &[
                ".omhl/templates/tracking-table.md",
                "First read",
                "applies in place"
            ]
        )
        .is_some(),
        "the override is read first and replaces the plugin template"
    );
    assert!(
        line_with(
            &review,
            &[
                ".omhl/templates/tracking-table.md",
                "exists",
                "takes its place"
            ]
        )
        .is_some(),
        "review: the override replaces the plugin template when the file exists"
    );
}

#[test]
fn it_should_state_that_the_plugin_template_applies_when_the_override_is_absent() {
    let template = base_reference("tracking-table.md");
    let review = read("review/SKILL.md");

    assert!(
        line_with(&template, &["absent", "plugin"]).is_some(),
        "the plugin template applies when the file is absent"
    );
    assert!(
        line_with(
            &review,
            &["tracking-table.md](", "plugin template", "exists"]
        )
        .is_some(),
        "review: the plugin template applies otherwise"
    );
}

#[test]
fn it_should_link_the_matrix_table_in_docs_from_roles_md_instead_of_copying_it() {
    let roles = base_reference("roles.md");

    // O plugin viaja sozinho (o `coverage.rs` exige que todo `.md` relativo resolva dentro de `plugin/`),
    // então o link para o `docs/` do repositório é uma URL absoluta.
    assert!(
        roles.contains(
            "](https://github.com/gabrielh-silvestre/oh-my-hexflow/blob/main/docs/directives-flow.md#a-matriz-por-papel)"
        ),
        "a markdown link to the matrix section"
    );
    assert!(
        !roles.contains("`append.nodes`"),
        "no copy of the matrix header"
    );
}

#[test]
fn it_should_name_the_seven_roles_in_roles_md() {
    let roles = base_reference("roles.md");

    for role in reference_roles() {
        assert!(
            roles.contains(&format!("oh-my-hexflow:{role}")),
            "roles.md names {role}"
        );
    }
}

#[test]
fn it_should_state_in_roles_md_that_the_owner_retires_contradicts_only_from_a_terminal_outside_claude_code()
 {
    let roles = base_reference("roles.md");

    assert!(
        line_with(
            &roles,
            &["owner", "terminal", "outside Claude Code", "contradicts"]
        )
        .is_some(),
        "the owner retires contradicts from an external terminal"
    );
    assert!(
        line_with(&roles, &["! omhl", "claude", "refused"]).is_some(),
        "`! omhl` inside Claude Code records claude and is refused"
    );
}

#[test]
fn it_should_tell_review_that_the_verifier_closes_a_reviewer_divergence_as_code_fixed_and_never_as_accepted()
 {
    let text = read("review/SKILL.md");

    assert!(
        line_with(&text, &["verifier", "code-fixed", "never", "accepted"]).is_some(),
        "one rule: code-fixed by the verifier, never accepted"
    );
}

#[test]
fn it_should_tell_review_to_list_decisions_and_critic_confirmed_premises_without_a_pipe() {
    let text = read("review/SKILL.md");

    let decisions = list_item(&text, "- `omhl list --type decision --where work_item=")
        .expect("the decisions listing item");
    let premises = list_item(&text, "- `omhl list --type premise --where work_item=")
        .expect("the premises listing item");

    for item in [decisions, premises] {
        let line = text.lines().nth(item).unwrap_or_default();
        assert!(!line.contains('|'), "no pipe in the listing item: {line}");
    }
    assert!(
        line_with(&text, &["one command per call", "no pipe"]).is_some(),
        "one command per call, no pipe"
    );
    assert!(
        line_with(&text, &["confirmed_by.by", "critic", "author.agent"]).is_some(),
        "the rows come from the critic: decisions by author.agent, premises by confirmed_by.by"
    );
    assert!(
        line_with(&text, &["author.agent", "equal to `claude`", "reinstall"]).is_some(),
        "a decision recorded as claude means the role hook failed open: stop"
    );
    assert!(
        line_with(
            &text,
            &["`omhl:code-reviewer`", "`omhl:security-reviewer`", "live"]
        )
        .is_some(),
        "the reviewers' decisions with a live contradicts also enter the table"
    );
}

const NEGATIONS: [&str; 4] = ["not", "never", "no", "without"];

pub(super) fn numbered_steps(text: &str) -> Vec<String> {
    let step = regex::Regex::new(r"^\d+\. ").expect("regex");
    body(text)
        .lines()
        .filter(|line| step.is_match(line))
        .map(str::to_owned)
        .collect()
}

pub(super) fn step_with(text: &str, needles: &[&str]) -> Option<usize> {
    numbered_steps(text)
        .iter()
        .position(|step| needles.iter().all(|needle| step.contains(needle)))
}

/// O trecho da linha antes de `marker`: uma negação ali inverteria a regra. Casa a palavra inteira
/// (`nothing` não é `no`), sem distinguir maiúscula.
fn prefix_has_negation(line: &str, marker: &str) -> bool {
    let prefix = line.split(marker).next().unwrap_or_default();
    prefix
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| NEGATIONS.contains(&word.to_lowercase().as_str()))
}

#[test]
fn it_should_order_the_plan_consensus_steps_by_number() {
    let text = read("plan-consensus/SKILL.md");

    let technical_critic = step_with(
        &text,
        &[
            "oh-my-hexflow:critic",
            "technical premises",
            "there is no plan or spec folder yet",
            "absence of the folder",
            "VERDICT",
        ],
    )
    .expect("a step where the critic confirms the technical premises before any plan exists");
    let planner = step_with(&text, &["oh-my-hexflow:planner"]).expect("the planner step");
    let architect = step_with(&text, &["oh-my-hexflow:architect"]).expect("the architect step");
    let verifier = step_with(
        &text,
        &["oh-my-hexflow:verifier", "isolated context", "contradicts"],
    )
    .expect("the verifier step");
    let validate = step_with(&text, &["run `omhl spec validate"]).expect("the final validate");

    assert!(technical_critic < planner && planner < architect && architect < verifier);
    assert!(
        verifier < validate,
        "the verifier step comes before the final validate"
    );
}

#[test]
fn it_should_never_negate_the_verifier_call_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");
    let verifier = step_with(
        &text,
        &["oh-my-hexflow:verifier", "isolated context", "contradicts"],
    )
    .expect("the verifier step");

    assert!(!prefix_has_negation(
        &numbered_steps(&text)[verifier],
        "oh-my-hexflow:verifier"
    ));
}

#[test]
fn it_should_tell_plan_consensus_that_the_planner_validates_the_spec() {
    let text = read("plan-consensus/SKILL.md");
    let planner = &numbered_steps(&text)
        [step_with(&text, &["oh-my-hexflow:planner"]).expect("the planner step")];

    assert!(
        planner.contains("and validates it"),
        "the planner validates the spec"
    );
    assert!(
        !planner.contains("is left for"),
        "the validation is not deferred"
    );
}

#[test]
fn it_should_tell_plan_consensus_that_the_last_validate_is_the_final_check() {
    let text = read("plan-consensus/SKILL.md");
    let planner = &numbered_steps(&text)
        [step_with(&text, &["oh-my-hexflow:planner"]).expect("the planner step")];

    assert!(
        planner.contains("final check"),
        "the last validate is the final check"
    );
}

#[test]
fn it_should_define_acceptance_and_the_loop_exits_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");
    let rounds = &numbered_steps(&text)[step_with(&text, &["3 rounds"]).expect("the rounds step")];

    for phrase in [
        "`ACCEPT`",
        "ACCEPT-WITH-RESERVATIONS",
        "third",
        "impasse",
        "owner",
        "gate-pr",
        "`REJECT`",
        "`REVISE`",
        "another round",
        "send the objections back to the architect",
    ] {
        assert!(rounds.contains(phrase), "the rounds step lacks '{phrase}'");
    }
    assert!(
        rounds.contains("the owner retires them") && rounds.contains("the verifier retires them"),
        "at the deadlock the owner retires the contradicts or orders a revision that the verifier closes"
    );
    assert!(
        step_with(
            &text,
            &[
                "oh-my-hexflow:verifier",
                "`FAIL`",
                "round without acceptance"
            ]
        )
        .is_some(),
        "a verifier FAIL counts as a round without acceptance"
    );
}

#[test]
fn it_should_keep_the_critic_verdict_out_of_the_graph_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");

    assert!(
        step_with(
            &text,
            &["verdict", "only in the message", "do not record it"]
        )
        .is_some(),
        "the verdict circulates only in the message"
    );
}

#[test]
fn it_should_hand_agents_the_absolute_spec_folder_in_both_skills() {
    for skill in ["plan-consensus", "review"] {
        let text = read(&format!("{skill}/SKILL.md"));

        assert!(
            line_with(
                &text,
                &["**absolute**", "does not exist inside the worktree"]
            )
            .is_some(),
            "{skill}: the spec folder is passed as an absolute path"
        );
    }
}

#[test]
fn it_should_hand_agents_the_from_to_pairs_in_both_skills() {
    for skill in ["plan-consensus", "review"] {
        let text = read(&format!("{skill}/SKILL.md"));

        assert!(
            line_with(
                &text,
                &["<from> -> <to>", "## Relations", "work-items.<id>.md"]
            )
            .is_some(),
            "{skill}: the pairs come from the reply or from the projection"
        );
    }
}

#[test]
fn it_should_tell_roles_md_where_the_from_to_pair_is_read() {
    let roles = base_reference("roles.md");

    assert!(
        line_with(
            &roles,
            &[
                "omhl project",
                "## Relations",
                ".omhl/projection/work-items.<id>.md"
            ]
        )
        .is_some(),
        "the projection holds the live relations"
    );
    assert!(
        !roles
            .to_lowercase()
            .contains("no read command returns relations"),
        "the false statement is gone"
    );
}

#[test]
fn it_should_tell_roles_md_that_the_verifier_cannot_read_the_pair() {
    let roles = base_reference("roles.md");

    assert!(
        line_with(&roles, &["verifier does not run `project`"]).is_some(),
        "the verifier cannot run project, so the pair travels in the request"
    );
}

#[test]
fn it_should_tell_roles_md_that_the_spec_folder_is_absolute() {
    let roles = base_reference("roles.md");

    assert!(
        line_with(&roles, &["**absolute**", "`dir`", "omhl spec init"]).is_some(),
        "the spec folder is absolute"
    );
}

#[test]
fn it_should_state_each_cycle_step_with_its_role() {
    let cycle = base_reference("cycle.md");
    let expectations: [(usize, &[&str]); 9] = [
        (2, &["oh-my-hexflow:analyst"]),
        (
            3,
            &[
                "oh-my-hexflow:critic",
                "technical",
                "human",
                "A gap without `kind`",
            ],
        ),
        (4, &["oh-my-hexflow:planner", "omhl spec init"]),
        (5, &["oh-my-hexflow:architect", "rests-on"]),
        (6, &["oh-my-hexflow:critic", "contradicts"]),
        (7, &["3 rounds", "owner"]),
        (8, &["verifier", "omhl spec validate", "omhl gate-worktree"]),
        (
            9,
            &[
                "oh-my-hexflow:code-reviewer",
                "oh-my-hexflow:security-reviewer",
                "oh-my-hexflow:verifier",
            ],
        ),
        (10, &["omhl gate-pr", "omhl close"]),
    ];

    for (number, needles) in expectations {
        let step = cycle
            .lines()
            .find(|line| line.starts_with(&format!("{number}. ")))
            .unwrap_or_else(|| panic!("cycle.md has step {number}"));

        for needle in needles {
            assert!(
                step.contains(needle),
                "cycle.md step {number} lacks '{needle}'"
            );
        }
    }
}

#[test]
fn it_should_retire_the_contradicts_before_the_final_spec_validate_in_cycle_md() {
    let cycle = base_reference("cycle.md");
    let step = cycle
        .lines()
        .find(|line| line.starts_with("8. "))
        .expect("cycle.md step 8");

    assert!(
        step.find("verifier") < step.find("omhl spec validate"),
        "the verifier retires first, then the final validate"
    );
}

#[test]
fn it_should_keep_roles_md_free_of_a_copy_of_the_matrix() {
    let roles = base_reference("roles.md");

    for row in roles.lines().filter(|line| line.starts_with('|')) {
        assert_eq!(
            row.matches('|').count(),
            3,
            "only the two-column pass table: {row}"
        );
    }
    assert!(
        roles.contains("](../../../agents/critic.md)"),
        "a relative link to the matrix that travels with the installed version"
    );
}

#[test]
fn it_should_forbid_claude_from_retiring() {
    let roles = base_reference("roles.md");

    assert!(
        line_with(
            &roles,
            &["`claude`", "does not retire", "role-retirement-forbidden"]
        )
        .is_some(),
        "claude does not retire"
    );
}

#[test]
fn it_should_keep_the_owner_rule_affirmative() {
    let roles = base_reference("roles.md");
    let owner = roles
        .lines()
        .find(|line| line.contains("The owner (`user`)"))
        .expect("the owner line");

    assert!(!prefix_has_negation(owner, "retires a `contradicts`"));
    assert!(owner.contains("only in an external terminal, outside Claude Code"));
}

#[test]
fn it_should_not_say_in_the_base_skill_that_every_record_is_claude() {
    let base = read("directives-flow/SKILL.md");

    assert!(
        line_with(
            &base,
            &[
                "`claude` in the main session",
                "`omhl:<role>` in a plugin agent",
                "`user` for the owner"
            ]
        )
        .is_some(),
        "the recorded agent depends on who runs the command"
    );
}

#[test]
fn it_should_point_review_findings_to_a_decision_that_rests_on_a_guideline_or_premise_never_on_a_decision()
 {
    let text = read("review/SKILL.md");

    assert!(
        line_with(
            &text,
            &[
                "`rests-on` to a confirmed guideline or premise",
                "never a `decision`",
                "`contradicts`"
            ]
        )
        .is_some()
    );
    assert!(
        line_with(
            &text,
            &["`source_ref` on the spec's `CA-###`", "`resolution` `open`"]
        )
        .is_some()
    );
    assert!(line_with(&text, &["the rest", "stays only in the message"]).is_some());
    assert!(
        line_with(&text, &["CRITICAL or HIGH", "PR body", "to the owner"]).is_some(),
        "a CRITICAL or HIGH security finding without a target goes to the owner and to the PR body"
    );
}

#[test]
fn it_should_close_the_review_loop_when_the_verifier_fails_or_leaves_a_divergence_open() {
    let text = read("review/SKILL.md");
    let steps = numbered_steps(&text);
    let loop_step = step_with(
        &text,
        &["`FAIL`", "`INCOMPLETE`", "`open`", "go back to step 3"],
    )
    .expect("a step that loops back to step 3");
    let fix = step_with(&text, &["Fix the code"]).expect("the fix step");

    assert!(steps[loop_step].contains("call it again"));
    assert!(
        steps[loop_step].contains("`PASS`"),
        "only a PASS verdict lets the review go on"
    );
    assert_eq!(fix + 1, 3, "the loop target is the fix step");
}

#[test]
fn it_should_give_the_pr_table_recipe_its_header_the_critic_path_and_the_pagination() {
    let review = read("review/SKILL.md");
    let template = base_reference("tracking-table.md");

    assert!(
        review.contains("`data.confirmed_by.by`"),
        "the critic path lives under data"
    );
    assert!(
        line_with(&review, &["next_after_seq", "--after-seq <n>"]).is_some(),
        "the listing is paginated"
    );
    assert!(
        template.contains("`| ID | Item | State | Evidence |`"),
        "the PR table header"
    );
    assert!(line_with(&template, &["**Item:**", "`data.text`"]).is_some());
    assert!(line_with(&template, &["**Evidence:**", "verifier message"]).is_some());
    assert!(line_with(&template, &["**ID:**", "plain text"]).is_some());
    for (name, text) in [("review", &review), ("tracking-table", &template)] {
        assert!(
            text.contains("byproduct.md"),
            "{name}: command output stays out of the PR"
        );
        assert!(text.contains("`\\|`"), "{name}: the pipe is escaped");
    }
}

#[test]
fn it_should_point_both_skills_to_the_reply_fields_rule_kept_in_roles_md() {
    let roles = base_reference("roles.md");

    for skill in ["plan-consensus", "review"] {
        let text = read(&format!("{skill}/SKILL.md"));

        assert!(
            line_with(
                &text,
                &[
                    "**Next role:**",
                    "**Open questions for the main session:**",
                    "roles.md",
                    "What to do with the answer"
                ]
            )
            .is_some(),
            "{skill}: the reply fields point to roles.md"
        );
    }
    assert!(
        roles.contains("**Next role:**"),
        "roles.md: Next role field"
    );
    assert!(
        line_with(
            &roles,
            &[
                "**Open questions for the main session:**",
                "`gap`",
                "`omhl append`",
                "`kind`"
            ]
        )
        .is_some(),
        "roles.md: open questions become gaps with the suggested kind"
    );
}

#[test]
fn it_should_send_a_gap_without_kind_to_the_human_in_roles_md() {
    let roles = base_reference("roles.md");

    assert!(line_with(&roles, &["Without `kind`", "goes to the human"]).is_some());
}

#[test]
fn it_should_send_business_premises_and_gaps_without_kind_to_the_human_in_plan_consensus_step_2() {
    let text = read("plan-consensus/SKILL.md");
    let steps = numbered_steps(&text);

    let step = step_with(
        &text,
        &["Take to the human", "`business`", "without `kind`"],
    )
    .expect("step 2");

    assert_eq!(step + 1, 2);
    assert!(!steps[step].contains("critic"));
}

#[test]
fn it_should_not_let_the_main_session_record_again_as_claude_what_a_role_refused_in_plan_consensus()
{
    let text = read("plan-consensus/SKILL.md");

    assert!(
        line_with(
            &text,
            &[
                "`role-record-forbidden`",
                "do not record the same content again",
                "`claude`"
            ]
        )
        .is_some(),
        "a refusal by the role matrix is the answer, not an order to record it as claude"
    );
}

#[test]
fn it_should_order_the_review_steps_by_number() {
    let text = read("review/SKILL.md");
    let steps = numbered_steps(&text);

    let reviewers = step_with(
        &text,
        &[
            "oh-my-hexflow:code-reviewer",
            "oh-my-hexflow:security-reviewer",
        ],
    )
    .expect("the step that calls both reviewers");
    let fix = step_with(&text, &["Fix the code"]).expect("the fix step");
    let verifier = step_with(&text, &["oh-my-hexflow:verifier", "isolated context"])
        .expect("the verifier step");
    let table = step_with(&text, &["Build the PR table"]).expect("the table step");
    let gate = step_with(&text, &["omhl gate-pr"]).expect("the gate step");

    assert!(reviewers < fix && fix < verifier && verifier < table && table < gate);
    let step = &steps[reviewers];
    assert!(
        step.find("oh-my-hexflow:code-reviewer") < step.find("oh-my-hexflow:security-reviewer"),
        "the code-reviewer comes before the security-reviewer"
    );
}

const GAP_ANSWER_LATE: [&str; 5] = [
    "first `decision`",
    "`origin: late`",
    "reopens the plan",
    "freeze",
    "`origin-required`",
];

#[test]
fn it_should_record_a_business_gap_answer_after_the_first_decision_as_late_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");

    let line = line_with(&text, &GAP_ANSWER_LATE).expect("the gap-answer sentence");

    assert!(
        text.lines()
            .nth(line)
            .is_some_and(|line| !line.starts_with(|c: char| c.is_ascii_digit())),
        "the sentence is a paragraph, not a numbered step"
    );
}

#[test]
fn it_should_record_a_business_gap_answer_after_the_first_decision_as_late_in_directives_flow() {
    let text = read("directives-flow/SKILL.md");

    assert!(
        line_with(&text, &GAP_ANSWER_LATE).is_some(),
        "directives-flow carries the gap-answer sentence"
    );
}

#[test]
fn it_should_explain_a_test_suite_failure_in_byproduct_md_without_naming_a_runner() {
    let text = read("review/references/byproduct.md");

    let mentions_cargo = text.to_lowercase().contains("cargo");

    assert!(!mentions_cargo, "byproduct.md is about any test runner");
    for fragment in [
        "omhl byproduct checks.test-suite <work-item> --tail 40",
        "ceiling of 500",
        "On the `develop` branch there is no `byproduct`",
    ] {
        assert!(text.contains(fragment), "byproduct.md lacks '{fragment}'");
    }
}
