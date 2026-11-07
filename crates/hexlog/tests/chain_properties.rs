mod support;

use hexlog::{Hexlog, RecordId};
use proptest::prelude::*;
use serde_json::json;
use support::mutations::{mutations, reference_chain};
use support::{Harness, node, relation, request, retire, revise, tenant};

/// Um node do modelo: o id semântico, o id técnico do último record e se está retirado.
struct ModelNode {
    id: String,
    latest: RecordId,
    retired: bool,
}

/// O que existe no banco, na visão do teste: só ele decide quais `append`s são válidos.
#[derive(Default)]
struct Model {
    nodes: Vec<ModelNode>,
    pairs: Vec<(usize, usize)>,
    written: u64,
}

impl Model {
    /// Índices dos nodes vigentes, em ordem de criação.
    fn live(&self) -> Vec<usize> {
        (0..self.nodes.len())
            .filter(|&index| !self.nodes[index].retired)
            .collect()
    }
}

fn append_item(hexlog: &mut Hexlog<'_>, item: hexlog::AppendItem) -> RecordId {
    hexlog
        .append(&request("acme", None, vec![item]))
        .expect("a valid item is appended")
        .records
        .remove(0)
        .id
}

/// Interpreta uma operação sorteada: emite o `append` só se for válido contra o modelo.
fn run_op(hexlog: &mut Hexlog<'_>, model: &mut Model, (kind, first, second): (u8, usize, usize)) {
    let live = model.live();
    match kind {
        0 => {
            let id = format!("p.n{}", model.nodes.len());
            let latest = append_item(hexlog, node(&id, "note", json!({"text": "new"})));
            model.nodes.push(ModelNode {
                id,
                latest,
                retired: false,
            });
        }
        1 if !live.is_empty() => {
            let target = live[first % live.len()];
            let data = json!({"count": model.written});
            let item = revise(
                node(&model.nodes[target].id, "note", data),
                &model.nodes[target].latest,
            );
            model.nodes[target].latest = append_item(hexlog, item);
        }
        2 if !live.is_empty() => {
            let target = live[first % live.len()];
            let item = retire(&model.nodes[target].id, &model.nodes[target].latest);
            model.nodes[target].latest = append_item(hexlog, item);
            model.nodes[target].retired = true;
        }
        3 if live.len() >= 2 => {
            // `from` sempre antes de `to` na ordem de criação: o grafo `after` nunca fecha ciclo.
            let (a, b) = (live[first % live.len()], live[second % live.len()]);
            let (from, to) = (a.min(b), a.max(b));
            if from == to || model.pairs.contains(&(from, to)) {
                return;
            }
            let item = relation("after", &model.nodes[from].id, &model.nodes[to].id);
            append_item(hexlog, item);
            model.pairs.push((from, to));
        }
        _ => return,
    }
    model.written += 1;
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn it_should_verify_after_any_sequence_of_valid_appends(
        ops in prop::collection::vec((0u8..4, 0usize..16, 0usize..16), 0..=12),
    ) {
        let harness = Harness::new();
        let mut hexlog = harness.open_acme();
        let mut model = Model::default();
        for op in ops {
            run_op(&mut hexlog, &mut model, op);
        }

        let report = hexlog
            .verify_chain(&tenant("acme"), None)
            .expect("verification runs");

        prop_assert!(report.ok);
        prop_assert_eq!(report.total_records, model.written);
    }

    #[test]
    fn it_should_detect_the_mutation_of_any_row_of_the_table_after_extra_appends(
        index in 0usize..34,
        extra_nodes in 0usize..=5,
    ) {
        let table = mutations();
        let mutation = &table[index];
        let harness = Harness::with_native_other();
        let reference = reference_chain(&harness, extra_nodes);

        let report = mutation.tamper_and_verify(&harness, &reference);

        prop_assert!(!report.ok, "{} is detected", mutation.id);
        let first = report.breaks.first().expect("a break is reported");
        prop_assert_eq!(first.reason, mutation.reason, "reason of {}", mutation.id);
        prop_assert_eq!(first.seq, mutation.expected_seq(&reference), "seq of {}", mutation.id);
    }
}
