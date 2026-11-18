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

/// Índice (0-based) da primeira linha de `text` que contém todos os `needles`.
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

/// A primeira linha de texto depois do título `# ` da skill.
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
        line_with(&text, &["omhl spec validate"]).expect("a step that cites spec validate");

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

    assert!(
        line_with(
            &template,
            &[".omhl/templates/tracking-table.md", "primeiro"]
        )
        .is_some(),
        "the override is read first"
    );
    assert!(
        line_with(&template, &["ausente", "plugin"]).is_some(),
        "the plugin template applies when the file is absent"
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
}
