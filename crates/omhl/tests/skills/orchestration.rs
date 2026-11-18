//! As skills de orquestração (`plan-consensus` e `review`) e as referências novas da base `directives-flow`
//! (`cycle.md`, `roles.md`, `tracking-table.md`). São texto de skill: o teste prova a presença de cada regra,
//! não o comportamento do agente. Os limites de linhas, as citações `omhl ...` e os caminhos `.md` já são
//! conferidos pelo `coverage.rs` sobre todas as skills, as novas incluídas.

use std::fs;

use crate::coverage::skills_dir;

const BASE_SENTENCE: &str =
    "Esta skill exige a `directives-flow`. Se não estiver carregada, carregue-a";
const ROLES: [&str; 7] = [
    "analyst",
    "planner",
    "architect",
    "critic",
    "code-reviewer",
    "security-reviewer",
    "verifier",
];

fn read(relative: &str) -> String {
    fs::read_to_string(skills_dir().join(relative))
        .unwrap_or_else(|error| panic!("{relative} must exist and be readable: {error}"))
}

fn base_reference(name: &str) -> String {
    read(&format!("directives-flow/references/{name}"))
}

fn line_with(text: &str, needles: &[&str]) -> Option<usize> {
    text.lines()
        .position(|line| needles.iter().all(|needle| line.contains(needle)))
}

/// O texto depois do frontmatter: a `description` cita comandos e não conta como passo.
fn body(skill: &str) -> String {
    skill
        .splitn(3, "---\n")
        .nth(2)
        .unwrap_or_default()
        .to_owned()
}

fn first_sentence_after_heading(skill: &str) -> String {
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
            first_sentence_after_heading(&text).starts_with(BASE_SENTENCE),
            "{skill} must open with the base requirement"
        );
    }
}

#[test]
fn it_should_give_the_new_skills_the_house_frontmatter() {
    for skill in ["plan-consensus", "review"] {
        let text = read(&format!("{skill}/SKILL.md"));
        let lines: Vec<&str> = text.lines().collect();

        assert_eq!(lines[0], "---", "{skill} frontmatter opens");
        assert_eq!(lines[1], format!("name: {skill}"));
        assert!(
            lines[2].starts_with("description: Protocolo "),
            "{skill} description"
        );
        assert_eq!(lines[3], "---", "{skill} frontmatter closes");
    }
}

#[test]
fn it_should_state_the_three_round_limit_and_the_escalation_to_the_owner_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");

    assert!(
        line_with(&text, &["3 voltas"]).is_some(),
        "the three-round limit"
    );
    assert!(
        line_with(&text, &["terceira", "impasse", "dono"]).is_some(),
        "on the third round without acceptance, stop and show the impasse to the owner"
    );
}

#[test]
fn it_should_call_the_verifier_before_spec_validate_in_plan_consensus() {
    let text = body(&read("plan-consensus/SKILL.md"));

    let verifier = line_with(&text, &["oh-my-hexflow:verifier", "contexto isolado"])
        .expect("a step that calls the verifier in an isolated context");
    let validate =
        line_with(&text, &["rode `omhl spec validate"]).expect("the final spec validate step");

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

    let verifier = line_with(&text, &["oh-my-hexflow:verifier", "contexto isolado"])
        .expect("a step that calls the verifier in an isolated context");
    let gate = line_with(&text, &["omhl gate-pr"]).expect("a step that cites gate-pr");

    assert!(verifier < gate, "the verifier step comes first");
}

#[test]
fn it_should_carry_the_cycle_the_roles_and_the_tracking_table_template_in_the_base_references() {
    let cycle = base_reference("cycle.md");
    let roles = base_reference("roles.md");
    let template = base_reference("tracking-table.md");

    for step in 1..=10 {
        assert!(
            cycle
                .lines()
                .any(|line| line.starts_with(&format!("{step}. "))),
            "cycle.md has step {step}"
        );
    }
    assert!(
        roles.contains("confirmed_by"),
        "roles.md has the critic confirmation rule"
    );
    assert!(roles.contains("rests-on"), "roles.md has the anchor rule");
    assert!(
        template.contains("## Acompanhamento"),
        "the template heading"
    );
    assert!(template.contains("**Legenda:**"), "the template legend");
    assert!(template.contains("**Totais ("), "the template totals line");
}

#[test]
fn it_should_state_that_the_override_replaces_the_default_template_and_that_the_plugin_template_applies_when_the_file_is_absent()
 {
    let template = base_reference("tracking-table.md");
    let review = read("review/SKILL.md");

    assert!(
        line_with(
            &template,
            &[
                ".omhl/templates/tracking-table.md",
                "primeiro",
                "vale no lugar"
            ]
        )
        .is_some(),
        "the override is read first and replaces the plugin template"
    );
    assert!(
        line_with(&template, &["ausente", "plugin"]).is_some(),
        "the plugin template applies when the file is absent"
    );
    assert!(
        line_with(
            &review,
            &[
                ".omhl/templates/tracking-table.md",
                "existir",
                "vale no lugar"
            ]
        )
        .is_some(),
        "review: the override replaces the plugin template when the file exists"
    );
    assert!(
        line_with(
            &review,
            &["tracking-table.md](", "template do plugin", "existir"]
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

    for role in ROLES {
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
            &["dono", "terminal", "fora do Claude Code", "contradicts"]
        )
        .is_some(),
        "the owner retires contradicts from an external terminal"
    );
    assert!(
        line_with(&roles, &["! omhl", "claude", "recusad"]).is_some(),
        "`! omhl` inside Claude Code records claude and is refused"
    );
}

#[test]
fn it_should_tell_review_that_the_verifier_closes_a_reviewer_divergence_as_code_fixed_and_never_as_accepted()
 {
    let text = read("review/SKILL.md");

    assert!(
        line_with(&text, &["verifier", "code-fixed", "nunca", "accepted"]).is_some(),
        "one rule: code-fixed by the verifier, never accepted"
    );
}

#[test]
fn it_should_tell_review_to_list_decisions_and_critic_confirmed_premises_without_a_pipe() {
    let text = read("review/SKILL.md");

    let decisions = line_with(&text, &["omhl list --type decision --where work_item="])
        .expect("the decisions listing");
    let premises = line_with(&text, &["omhl list --type premise --where work_item="])
        .expect("the premises listing");

    assert_ne!(decisions, premises, "one command per line");
    assert!(
        line_with(&text, &["um comando por chamada", "sem pipe"]).is_some(),
        "one command per call, no pipe"
    );
    assert!(
        line_with(&text, &["confirmed_by.by", "critic", "author.agent"]).is_some(),
        "the rows come from the critic: decisions by author.agent, premises by confirmed_by.by"
    );
    assert!(
        line_with(&text, &["author.agent", "igual a `claude`", "reinstalar"]).is_some(),
        "a decision recorded as claude means the role hook failed open: stop"
    );
    assert!(
        line_with(
            &text,
            &[
                "`omhl:code-reviewer`",
                "`omhl:security-reviewer`",
                "vigente"
            ]
        )
        .is_some(),
        "the reviewers' decisions with a live contradicts also enter the table"
    );
}

const NEGATIONS: [&str; 3] = ["não", "nunca", "sem "];

fn numbered_steps(text: &str) -> Vec<String> {
    let step = regex::Regex::new(r"^\d+\. ").expect("regex");
    body(text)
        .lines()
        .filter(|line| step.is_match(line))
        .map(str::to_owned)
        .collect()
}

fn step_with(text: &str, needles: &[&str]) -> Option<usize> {
    numbered_steps(text)
        .iter()
        .position(|step| needles.iter().all(|needle| step.contains(needle)))
}

/// O trecho da linha antes de `marker`: uma negação ali inverteria a regra.
fn prefix_has_negation(line: &str, marker: &str) -> bool {
    let prefix = line.split(marker).next().unwrap_or_default();
    NEGATIONS.iter().any(|negation| prefix.contains(negation))
}

#[test]
fn it_should_order_the_plan_consensus_steps_by_number_and_never_negate_the_verifier_call() {
    let text = read("plan-consensus/SKILL.md");

    let technical_critic = step_with(
        &text,
        &[
            "oh-my-hexflow:critic",
            "premissas técnicas",
            "ainda não há plano",
            "ausência da pasta",
            "VERDICT",
        ],
    )
    .expect("a step where the critic confirms the technical premises before any plan exists");
    let planner = step_with(&text, &["oh-my-hexflow:planner"]).expect("the planner step");
    let architect = step_with(&text, &["oh-my-hexflow:architect"]).expect("the architect step");
    let verifier = step_with(
        &text,
        &["oh-my-hexflow:verifier", "contexto isolado", "contradicts"],
    )
    .expect("the verifier step");
    let validate = step_with(&text, &["rode `omhl spec validate"]).expect("the final validate");
    let steps = numbered_steps(&text);

    assert!(technical_critic < planner && planner < architect && architect < verifier);
    assert!(
        verifier < validate,
        "the verifier step comes before the final validate"
    );
    assert!(!prefix_has_negation(
        &steps[verifier],
        "oh-my-hexflow:verifier"
    ));
}

#[test]
fn it_should_tell_plan_consensus_that_the_planner_validates_and_the_last_validate_is_the_final_check()
 {
    let text = read("plan-consensus/SKILL.md");
    let planner = &numbered_steps(&text)[step_with(&text, &["oh-my-hexflow:planner"]).unwrap()];

    assert!(
        planner.contains("e a valida"),
        "the planner validates the spec"
    );
    assert!(
        planner.contains("conferência final"),
        "the last validate is the final check"
    );
    assert!(
        !planner.contains("fica para"),
        "the validation is not deferred"
    );
}

#[test]
fn it_should_define_acceptance_and_the_loop_exits_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");
    let rounds = &numbered_steps(&text)[step_with(&text, &["3 voltas"]).expect("the rounds step")];

    for phrase in [
        "`ACCEPT`",
        "ACCEPT-WITH-RESERVATIONS",
        "terceira",
        "impasse",
        "dono",
        "gate-pr",
        "`REJECT`",
        "`REVISE`",
        "outra volta",
        "devolva as objeções ao architect",
    ] {
        assert!(rounds.contains(phrase), "the rounds step lacks '{phrase}'");
    }
    assert!(
        rounds.contains("o dono os retira") && rounds.contains("o verifier retira"),
        "at the deadlock the owner retires the contradicts or orders a revision that the verifier closes"
    );
    assert!(
        step_with(
            &text,
            &["oh-my-hexflow:verifier", "`FAIL`", "volta sem aceite"]
        )
        .is_some(),
        "a verifier FAIL counts as a round without acceptance"
    );
}

#[test]
fn it_should_keep_the_critic_verdict_out_of_the_graph_in_plan_consensus() {
    let text = read("plan-consensus/SKILL.md");

    assert!(
        step_with(&text, &["veredito", "só na mensagem", "não o grave"]).is_some(),
        "the verdict circulates only in the message"
    );
}

#[test]
fn it_should_hand_agents_the_absolute_spec_folder_and_the_from_to_pairs_in_both_skills() {
    for skill in ["plan-consensus", "review"] {
        let text = read(&format!("{skill}/SKILL.md"));

        assert!(
            line_with(&text, &["**absoluto**", "não existe dentro da worktree"]).is_some(),
            "{skill}: the spec folder is passed as an absolute path"
        );
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
fn it_should_tell_roles_md_where_the_from_to_pair_is_read_and_who_can_read_it() {
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
        line_with(&roles, &["verifier não roda `project`"]).is_some(),
        "the verifier cannot run project, so the pair travels in the request"
    );
    assert!(
        !roles.contains("nenhum comando de leitura devolve relações"),
        "the false statement is gone"
    );
    assert!(
        line_with(&roles, &["**absoluto**", "dir", "omhl spec init"]).is_some(),
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
                "técnico",
                "humano",
                "Gap sem `kind`",
            ],
        ),
        (4, &["oh-my-hexflow:planner", "omhl spec init"]),
        (5, &["oh-my-hexflow:architect", "rests-on"]),
        (6, &["oh-my-hexflow:critic", "contradicts"]),
        (7, &["3 voltas", "dono"]),
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
fn it_should_forbid_claude_from_retiring_and_keep_the_owner_rule_affirmative() {
    let roles = base_reference("roles.md");
    let owner = roles
        .lines()
        .find(|line| line.contains("O dono (`user`)"))
        .expect("the owner line");

    assert!(
        line_with(
            &roles,
            &["`claude`", "não retira", "role-retirement-forbidden"]
        )
        .is_some(),
        "claude does not retire"
    );
    assert!(!prefix_has_negation(owner, "retira um `contradicts`"));
    assert!(owner.contains("só num terminal externo, fora do Claude Code"));
}

#[test]
fn it_should_not_say_in_the_base_skill_that_every_record_is_claude() {
    let base = read("directives-flow/SKILL.md");

    assert!(
        line_with(
            &base,
            &[
                "`claude` na sessão principal",
                "`omhl:<papel>` num agente do plugin",
                "`user` para o dono"
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
                "`rests-on` para uma diretriz ou premissa confirmada",
                "nunca para uma `decision`",
                "`contradicts`"
            ]
        )
        .is_some()
    );
    assert!(line_with(&text, &["`source_ref` no `CA-###`", "`resolution` `open`"]).is_some());
    assert!(line_with(&text, &["o resto", "só na mensagem"]).is_some());
    assert!(
        line_with(&text, &["CRITICAL ou HIGH", "corpo do PR", "ao dono"]).is_some(),
        "a CRITICAL or HIGH security finding without a target goes to the owner and to the PR body"
    );
}

#[test]
fn it_should_close_the_review_loop_when_the_verifier_fails_or_leaves_a_divergence_open() {
    let text = read("review/SKILL.md");
    let steps = numbered_steps(&text);
    let loop_step = step_with(
        &text,
        &["`FAIL`", "`INCOMPLETE`", "`open`", "volte ao passo 3"],
    )
    .expect("a step that loops back to step 3");
    let fix = step_with(&text, &["Corrija o código"]).expect("the fix step");

    assert!(steps[loop_step].contains("chame-o de novo"));
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
        template.contains("`| ID | Item | Estado | Evidência |`"),
        "the PR table header"
    );
    assert!(line_with(&template, &["**Item:**", "`data.text`"]).is_some());
    assert!(line_with(&template, &["**Evidência:**", "mensagem do verifier"]).is_some());
    assert!(line_with(&template, &["**ID:**", "texto simples"]).is_some());
    for (name, text) in [("review", &review), ("tracking-table", &template)] {
        assert!(
            text.contains("byproduct.md"),
            "{name}: command output stays out of the PR"
        );
        assert!(text.contains("`\\|`"), "{name}: the pipe is escaped");
    }
}

#[test]
fn it_should_keep_the_two_fixed_reply_fields_and_the_gap_recording_rule_in_both_skills() {
    for skill in ["plan-consensus", "review"] {
        let text = read(&format!("{skill}/SKILL.md"));

        assert!(text.contains("**Next role:**"), "{skill}: Next role field");
        assert!(
            line_with(
                &text,
                &[
                    "**Open questions for the main session:**",
                    "`gap`",
                    "`omhl append`",
                    "`kind`"
                ]
            )
            .is_some(),
            "{skill}: open questions become gaps with the suggested kind"
        );
        assert!(
            line_with(&text, &["Sem `kind`", "vai ao humano"]).is_some(),
            "{skill}: a gap without kind goes to the human"
        );
    }
}

#[test]
fn it_should_send_a_gap_without_kind_to_the_human_in_roles_md() {
    let roles = base_reference("roles.md");

    assert!(line_with(&roles, &["Sem `kind`", "vai ao humano"]).is_some());
}

#[test]
fn it_should_send_business_premises_and_gaps_without_kind_to_the_human_in_plan_consensus_step_2() {
    let text = read("plan-consensus/SKILL.md");
    let steps = numbered_steps(&text);

    let step = step_with(&text, &["Leve ao humano", "`business`", "sem `kind`"]).expect("step 2");

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
            &["`role-record-forbidden`", "não regrave", "`claude`"]
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
    let fix = step_with(&text, &["Corrija o código"]).expect("the fix step");
    let verifier = step_with(&text, &["oh-my-hexflow:verifier", "contexto isolado"])
        .expect("the verifier step");
    let table = step_with(&text, &["Monte a tabela do PR"]).expect("the table step");
    let gate = step_with(&text, &["omhl gate-pr"]).expect("the gate step");

    assert!(reviewers < fix && fix < verifier && verifier < table && table < gate);
    let step = &steps[reviewers];
    assert!(
        step.find("oh-my-hexflow:code-reviewer") < step.find("oh-my-hexflow:security-reviewer"),
        "the code-reviewer comes before the security-reviewer"
    );
}
