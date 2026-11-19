//! Cada exemplo `omhl append <<'EOF'` dos prompts em `plugin/agents/` roda pela CLI real (guarda incluída),
//! como o papel do agente, sobre um banco que tem só o que o texto ao redor do exemplo manda existir.
//! O JSON do exemplo é lido do arquivo do agente em cada execução: o teste segue o texto.
//! Só o `base` (`<record_id from omhl get ID>`) é trocado, pelo `record_id` vigente lido com `omhl get`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use hexlog::{Config, RelationQuery};
use omhl::facade::{Omhl, OmhlDeps};
use serde_json::{Value, json};
use support::hook::{bash_call, decision_of, run_agent_role_in};
use support::omhl::acme;
use support::project::Project;
use support::run::run_omhl;
use support::{FakeProbe, FixedClock, SeqIds, rty};

const BASE_OPEN: &str = "<record_id from omhl get ";

// Leitura dos exemplos

fn agent_file(role: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    root.join(format!("plugin/agents/{role}.md"))
}

/// O JSON de cada heredoc `omhl append <<'EOF'` do agente, na ordem em que aparecem.
fn examples_of(role: &str) -> Vec<Value> {
    let path = agent_file(role);
    let file = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is missing: {error}", path.display()));
    let mut lines = file.lines();
    let mut found = Vec::new();

    while let Some(line) = lines.next() {
        if line.trim() == "omhl append <<'EOF'" {
            let body: Vec<&str> = lines.by_ref().take_while(|line| *line != "EOF").collect();
            found.push(serde_json::from_str(&body.join("\n")).expect("the example is JSON"));
        }
    }

    found
}

fn example(role: &str, index: usize) -> Value {
    let mut found = examples_of(role);
    assert!(
        index < found.len(),
        "{role} has {} append examples, not {}",
        found.len(),
        index + 1
    );

    found.swap_remove(index)
}

fn items(request: &Value) -> &Vec<Value> {
    request["items"].as_array().expect("the example has items")
}

fn node<'a>(request: &'a Value, type_name: &str) -> &'a Value {
    items(request)
        .iter()
        .map(|item| &item["body"])
        .find(|body| body["kind"] == "node" && body["type"] == type_name)
        .unwrap_or_else(|| panic!("the example has no {type_name} node"))
}

fn relation<'a>(request: &'a Value, relation_type: &str) -> &'a Value {
    items(request)
        .iter()
        .map(|item| &item["body"])
        .find(|body| body["kind"] == "relation" && body["relation_type"] == relation_type)
        .unwrap_or_else(|| panic!("the example has no {relation_type} relation"))
}

/// O work item que o texto manda existir: o `work_item` dos nodes do exemplo.
fn work_item_of(request: &Value) -> String {
    items(request)
        .iter()
        .find_map(|item| item["body"]["data"]["work_item"].as_str())
        .expect("the example names its work item")
        .to_owned()
}

fn text(value: &Value) -> &str {
    value.as_str().expect("the field is text")
}

// Escrita

/// Grava o que o teste prepara, com o autor dado; falha o teste se a guarda recusar.
fn seed(project: &Project, role: Option<&str>, items: &[Value]) {
    let request = json!({ "items": items });

    let run = run_omhl(
        project,
        &["append", "--format", "json"],
        &request.to_string(),
        true,
        role,
    );

    assert!(
        run.stdout.contains("\"records\""),
        "seeding was refused: {}",
        run.stderr
    );
}

fn node_item(id: &str, type_name: &str, data: &Value) -> Value {
    json!({ "body": { "kind": "node", "id": id, "type": type_name, "data": data } })
}

fn person_confirmation() -> Value {
    json!({ "by": "person:ana", "question": "Is this right?", "answer": "Yes, it is." })
}

fn seed_work_item(project: &Project, work_item: &str) {
    let data = json!({ "title": "Export report", "status": "open" });

    seed(project, None, &[node_item(work_item, "work-item", &data)]);
}

/// A premissa confirmada pelo dono em que o `rests-on` do exemplo se apoia.
fn seed_confirmed_premise(project: &Project, id: &str, work_item: &str) {
    let data = json!({
        "text": "The export must not hold the whole report in memory.",
        "status": "confirmed",
        "kind": "business",
        "work_item": work_item,
        "confirmed_by": person_confirmation(),
    });

    seed(project, None, &[node_item(id, "premise", &data)]);
}

/// O work item e a premissa confirmada em que o `rests-on` do exemplo se apoia.
fn seed_rests_on_premise(project: &Project, request: &Value) {
    let work_item = work_item_of(request);
    seed_work_item(project, &work_item);
    seed_confirmed_premise(
        project,
        text(&relation(request, "rests-on")["to"]),
        &work_item,
    );
}

/// A diretiva confirmada pelo dono em que o `rests-on` do exemplo se apoia.
fn seed_guideline(project: &Project, id: &str) {
    let data = json!({
        "text": "Every query binds its parameters.",
        "kind": "business",
        "confirmed_by": person_confirmation(),
    });

    seed(project, None, &[node_item(id, "guideline", &data)]);
}

fn seed_decision(project: &Project, role: &str, id: &str, work_item: &str) {
    let data = json!({
        "text": "A decision recorded before the example runs.",
        "evidence": [{ "source": "src/export.rs:1", "summary": "Read while seeding." }],
        "work_item": work_item,
    });

    seed(project, Some(role), &[node_item(id, "decision", &data)]);
}

/// O `record_id` vigente do node, lido pelo `omhl get` como o agente leria.
fn record_id_of(project: &Project, id: &str) -> String {
    let fetched = get(project, id);

    text(&fetched["node"]["record_id"]).to_owned()
}

fn get(project: &Project, id: &str) -> Value {
    let run = run_omhl(project, &["get", id, "--format", "json"], "", true, None);

    serde_json::from_str(run.stdout.trim_end())
        .unwrap_or_else(|_| panic!("get {id} failed: {}", run.stderr))
}

/// Troca cada `base` `<record_id from omhl get ID>` pelo `record_id` vigente de `ID`.
fn with_real_bases(project: &Project, mut request: Value) -> Value {
    let items = request["items"]
        .as_array_mut()
        .expect("the example has items");
    for item in items {
        let Some(placeholder) = item["base"].as_str() else {
            continue;
        };
        let id = placeholder
            .strip_prefix(BASE_OPEN)
            .and_then(|rest| rest.strip_suffix('>'))
            .unwrap_or_else(|| panic!("unknown base placeholder: {placeholder}"));
        item["base"] = json!(record_id_of(project, id));
    }

    request
}

/// Roda o exemplo como o papel; devolve quantos records foram gravados, ou a linha de erro da recusa.
fn append_example(project: &Project, role: &str, request: &Value) -> Result<usize, String> {
    let run = run_omhl(
        project,
        &["append", "--format", "json"],
        &request.to_string(),
        true,
        Some(role),
    );
    if run.stdout.is_empty() {
        return Err(run.stderr);
    }
    let output: Value = serde_json::from_str(run.stdout.trim_end()).expect("the output is JSON");

    Ok(output["records"]
        .as_array()
        .expect("records are listed")
        .len())
}

/// Quantas relations `relation_type` seguem vigentes, lidas do banco do projeto.
fn current_relations(project: &Project, relation_type: &str) -> usize {
    let (clock, ids, probe) = (FixedClock::default(), SeqIds::new(), FakeProbe::failing());
    let deps = OmhlDeps {
        clock: &clock,
        ids: &ids,
        probe: &probe,
    };
    let mut omhl = Omhl::open(&Config::new(project.db_path()), deps).expect("omhl opens");
    let query = RelationQuery {
        relation_type: Some(rty(relation_type)),
        from: None,
        to: None,
        include_retired: false,
    };

    omhl.relations(&acme(), &query)
        .expect("the relations are read")
        .len()
}

// analyst

#[test]
fn it_should_accept_the_analyst_append_example() {
    let project = Project::new("acme");
    let request = example("analyst", 0);
    seed_work_item(&project, &work_item_of(&request));

    let result = append_example(&project, "omhl:analyst", &request);

    assert_eq!(result, Ok(items(&request).len()));
}

// architect

#[test]
fn it_should_accept_the_architect_append_example() {
    let project = Project::new("acme");
    let request = example("architect", 0);
    seed_rests_on_premise(&project, &request);

    let result = append_example(&project, "omhl:architect", &request);

    assert_eq!(result, Ok(items(&request).len()));
}

// code-reviewer

#[test]
fn it_should_accept_the_code_reviewer_append_example() {
    let project = Project::new("acme");
    let request = example("code-reviewer", 0);
    seed_rests_on_premise(&project, &request);

    let result = append_example(&project, "omhl:code-reviewer", &request);

    assert_eq!(result, Ok(items(&request).len()));
}

// security-reviewer

#[test]
fn it_should_accept_the_security_reviewer_append_example() {
    let project = Project::new("acme");
    let request = example("security-reviewer", 0);
    seed_work_item(&project, &work_item_of(&request));
    seed_guideline(&project, text(&relation(&request, "rests-on")["to"]));

    let result = append_example(&project, "omhl:security-reviewer", &request);

    assert_eq!(result, Ok(items(&request).len()));
}

// critic: decisão ancorada

#[test]
fn it_should_accept_the_critic_decision_append_example() {
    let project = Project::new("acme");
    let request = example("critic", 0);
    seed_rests_on_premise(&project, &request);

    let result = append_example(&project, "omhl:critic", &request);

    assert_eq!(result, Ok(items(&request).len()));
}

// critic: confirmação de premissa técnica que responde a um gap

/// O que o texto manda existir: a premissa técnica `assumed` do analyst (o `data` do exemplo antes de o
/// critic mudar `status` e `confirmed_by`) e o gap `technical` que o `answers` aponta.
fn seed_critic_confirmation_prerequisites(project: &Project, request: &Value) {
    let work_item = work_item_of(request);
    let premise = node(request, "premise");
    let mut assumed = premise["data"].clone();
    assumed["status"] = json!("assumed");
    assumed
        .as_object_mut()
        .expect("data is an object")
        .remove("confirmed_by");
    let gap = json!({
        "question": "Which time zone do the export timestamps use?",
        "context": "The request does not say.",
        "kind": "technical",
        "work_item": work_item,
    });
    seed_work_item(project, &work_item);
    seed(
        project,
        Some("omhl:analyst"),
        &[
            node_item(text(&premise["id"]), "premise", &assumed),
            node_item(text(&relation(request, "answers")["to"]), "gap", &gap),
        ],
    );
}

#[test]
fn it_should_accept_the_critic_confirmation_append_example() {
    let project = Project::new("acme");
    let request = example("critic", 1);
    seed_critic_confirmation_prerequisites(&project, &request);
    let request = with_real_bases(&project, request);

    let result = append_example(&project, "omhl:critic", &request);

    assert_eq!(result, Ok(items(&request).len()));
}

#[test]
fn it_should_confirm_the_premise_when_the_critic_confirmation_example_runs() {
    let project = Project::new("acme");
    let request = example("critic", 1);
    seed_critic_confirmation_prerequisites(&project, &request);
    let request = with_real_bases(&project, request);
    let premise_id = text(&node(&request, "premise")["id"]).to_owned();

    let _ = append_example(&project, "omhl:critic", &request);

    assert_eq!(
        get(&project, &premise_id)["node"]["data"]["status"],
        "confirmed"
    );
}

// verifier: fechamento de divergência

/// A divergência `open` do code-reviewer: o `data` do exemplo antes do fechamento, com a evidência
/// que existia antes da checagem nova (todas menos a última) e `resolution` `open`.
fn seed_open_divergence(project: &Project, request: &Value) {
    let divergence = node(request, "divergence");
    let mut open = divergence["data"].clone();
    open["resolution"] = json!("open");
    open["evidence"]
        .as_array_mut()
        .expect("evidence is a list")
        .pop();
    seed_work_item(project, &work_item_of(request));
    seed(
        project,
        Some("omhl:code-reviewer"),
        &[node_item(text(&divergence["id"]), "divergence", &open)],
    );
}

#[test]
fn it_should_accept_the_verifier_closing_append_example() {
    let project = Project::new("acme");
    let request = example("verifier", 0);
    seed_open_divergence(&project, &request);
    let request = with_real_bases(&project, request);

    let result = append_example(&project, "omhl:verifier", &request);

    assert_eq!(result, Ok(items(&request).len()));
}

#[test]
fn it_should_close_the_divergence_as_code_fixed_when_the_verifier_closing_example_runs() {
    let project = Project::new("acme");
    let request = example("verifier", 0);
    seed_open_divergence(&project, &request);
    let request = with_real_bases(&project, request);
    let divergence_id = text(&node(&request, "divergence")["id"]).to_owned();

    let _ = append_example(&project, "omhl:verifier", &request);

    assert_eq!(
        get(&project, &divergence_id)["node"]["data"]["resolution"],
        "code-fixed"
    );
}

// verifier: retirada de contradicts

/// O `contradicts` que o texto manda existir: de uma decision do code-reviewer para uma decision do
/// architect, as duas pontas lidas da `lineage` do exemplo; o verifier não o gravou.
fn seed_contradiction(project: &Project, request: &Value) {
    let lineage = &items(request)[0]["body"]["lineage"];
    let (from, to) = (text(&lineage["from"]), text(&lineage["to"]));
    let work_item = "work-items.w1";
    seed_work_item(project, work_item);
    seed_decision(project, "omhl:architect", to, work_item);
    seed_decision(project, "omhl:code-reviewer", from, work_item);
    seed(
        project,
        Some("omhl:code-reviewer"),
        &[json!({ "body": {
            "kind": "relation", "relation_type": "contradicts", "from": from, "to": to,
        } })],
    );
}

/// A primeira chamada do texto (sem `base`): a recusa que o verifier lê, como o JSON de erro da CLI.
fn first_retirement_refusal(project: &Project) -> Value {
    let request = example("verifier", 1);
    let run = run_omhl(
        project,
        &["append", "--format", "json"],
        &request.to_string(),
        true,
        Some("omhl:verifier"),
    );

    serde_json::from_str(run.stderr.trim_end())
        .unwrap_or_else(|_| panic!("the first call was not refused as JSON: {}", run.stderr))
}

/// O `details[0].current` da recusa, que o texto manda usar como `base` da segunda chamada.
fn current_of(refusal: &Value) -> &str {
    text(&refusal["error"]["details"][0]["current"])
}

/// A segunda chamada do texto: o mesmo pedido, com o `base` lido da recusa da primeira.
fn second_retirement_request(refusal: &Value) -> Value {
    let mut request = example("verifier", 2);
    let base = request["items"][0]
        .get_mut("base")
        .expect("the second retirement example carries a `base` to replace");
    *base = json!(current_of(refusal));

    request
}

#[test]
fn it_should_refuse_the_first_verifier_retirement_call_as_not_current() {
    let project = Project::new("acme");
    seed_contradiction(&project, &example("verifier", 1));

    let refusal = first_retirement_refusal(&project);

    assert_eq!(
        (
            &refusal["error"]["code"],
            &refusal["error"]["details"][0]["code"]
        ),
        (&json!("FORK_REJECTED"), &json!("not-current"))
    );
}

#[test]
fn it_should_send_the_first_verifier_retirement_call_without_base() {
    let first = example("verifier", 1);

    assert!(first["items"][0].get("base").is_none());
}

#[test]
fn it_should_send_the_same_retirement_in_both_verifier_calls() {
    let first = example("verifier", 1);
    let second = example("verifier", 2);

    assert_eq!(first["items"][0]["body"], second["items"][0]["body"]);
}

#[test]
fn it_should_accept_the_second_verifier_retirement_call_with_the_base_from_the_refusal() {
    let project = Project::new("acme");
    seed_contradiction(&project, &example("verifier", 1));
    let request = second_retirement_request(&first_retirement_refusal(&project));

    let result = append_example(&project, "omhl:verifier", &request);

    assert_eq!(result, Ok(items(&request).len()));
}

#[test]
fn it_should_leave_no_current_contradicts_when_the_two_verifier_retirement_calls_run() {
    let project = Project::new("acme");
    seed_contradiction(&project, &example("verifier", 1));
    let request = second_retirement_request(&first_retirement_refusal(&project));

    let _ = append_example(&project, "omhl:verifier", &request);

    assert_eq!(current_relations(&project, "contradicts"), 0);
}

// Hook agent-role: cada comando `omhl` do prompt, como escrito, é liberado com o papel do agente

/// Cada bloco cercado do agente cujo texto começa com `omhl`, exatamente como escrito.
fn fenced_omhl_commands(role: &str) -> Vec<String> {
    let file = fs::read_to_string(agent_file(role)).expect("the agent file is readable");
    let mut commands = Vec::new();
    let mut open: Option<Vec<&str>> = None;

    for line in file.lines() {
        match (line.trim_start().starts_with("```"), open.take()) {
            (true, Some(block)) => {
                if block.first().is_some_and(|first| first.starts_with("omhl")) {
                    commands.push(block.join("\n"));
                }
            }
            (true, None) => open = Some(Vec::new()),
            (false, Some(mut block)) => {
                block.push(line);
                open = Some(block);
            }
            (false, None) => {}
        }
    }

    commands
}

fn assert_hook_allows_every_fenced_command(role: &str) {
    let plugin_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin");
    let agent_type = format!("oh-my-hexflow:{role}");
    let commands = fenced_omhl_commands(role);
    assert!(!commands.is_empty(), "{role} shows no omhl command");

    for command in commands {
        let output = run_agent_role_in(&plugin_root, &bash_call(&command, Some(&agent_type)));
        let decision = decision_of(&output);

        assert_eq!(
            decision["permissionDecision"], "allow",
            "{role}: {command}\n{decision}"
        );
        assert_eq!(
            decision["updatedInput"]["command"],
            json!(format!("OMHL_ROLE=omhl:{role} {command}")),
            "{role}: {command}"
        );
    }
}

#[test]
fn it_should_allow_every_fenced_command_of_the_analyst_through_the_hook() {
    assert_hook_allows_every_fenced_command("analyst");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_architect_through_the_hook() {
    assert_hook_allows_every_fenced_command("architect");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_code_reviewer_through_the_hook() {
    assert_hook_allows_every_fenced_command("code-reviewer");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_security_reviewer_through_the_hook() {
    assert_hook_allows_every_fenced_command("security-reviewer");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_critic_through_the_hook() {
    assert_hook_allows_every_fenced_command("critic");
}

#[test]
fn it_should_allow_every_fenced_command_of_the_verifier_through_the_hook() {
    assert_hook_allows_every_fenced_command("verifier");
}
