use std::path::PathBuf;

use hexlog::chain::{canonical, hash_record};
use hexlog::{BreakReason, ChainReport, Hash, RecordId};
use serde_json::{Value, json};

use super::tamper::Tamper;
use super::{
    DEFAULT_INSTANT, Harness, commit, node, relation, request, retire, retire_relation, revise,
    tenant, unused_record_id, with_external, with_snapshot,
};

/// Id da linha da tabela que é argumento da chamada, e não campo armazenado.
pub const EXPECTED_HEAD_ID: &str = "expected_chain_head";
/// Instante diferente do `DEFAULT_INSTANT` do relógio fixo.
pub const OTHER_INSTANT: &str = "2000-01-01T00:00:00.000Z";

/// Um record da cadeia de referência: id técnico e `seq`.
pub struct Target {
    pub id: RecordId,
    pub seq: u64,
}

/// Cadeia de referência da seção 4.5 do plano: RC1 a RC6 no tenant `acme`, em cinco lotes.
pub struct Reference {
    pub tenant: String,
    pub path: PathBuf,
    records: [Target; 6],
}

impl Reference {
    /// RC`n`, de 1 a 6.
    pub fn rc(&self, n: usize) -> &Target {
        &self.records[n - 1]
    }

    fn seq_text(&self, n: usize) -> String {
        self.rc(n).seq.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedSeq {
    /// O `seq` do alvo da adulteração.
    Target,
    /// O `seq` seguinte ao do alvo: o record adulterado muda de posição, e quem diverge é o seguinte.
    TargetPlusOne,
    /// Quebra sem `seq`: âncora, lote ou chain head esperado.
    Absent,
}

pub struct Mutation {
    pub id: &'static str,
    pub reason: BreakReason,
    pub seq: ExpectedSeq,
    /// Número do RC alvo (1 a 6); 0 quando a linha não adultera um record.
    pub target: usize,
    /// Devolve as linhas alteradas; o teste confere que foi exatamente 1.
    pub apply: fn(&mut Tamper, &Reference) -> u64,
}

impl Mutation {
    pub fn expected_seq(&self, reference: &Reference) -> Option<u64> {
        match self.seq {
            ExpectedSeq::Target => Some(reference.rc(self.target).seq),
            ExpectedSeq::TargetPlusOne => Some(reference.rc(self.target).seq + 1),
            ExpectedSeq::Absent => None,
        }
    }

    /// Aplica a adulteração com `Tamper` (já sem o `Hexlog` que escreveu) e roda `verify_chain`.
    /// A linha `expected_chain_head` não mexe no banco: ela passa um chain head desconhecido.
    pub fn tamper_and_verify(&self, harness: &Harness, reference: &Reference) -> ChainReport {
        let mut tamper = Tamper::open(&reference.path);
        let changed = (self.apply)(&mut tamper, reference);
        assert_eq!(changed, 1, "{} alters exactly one row", self.id);
        drop(tamper);
        let expected = (self.id == EXPECTED_HEAD_ID).then(unknown_head);

        harness
            .open()
            .verify_chain(&tenant(&reference.tenant), expected.as_ref())
            .expect("verification runs")
    }
}

pub fn unknown_head() -> Hash {
    Hash::parse(&"d".repeat(64)).expect("valid hash")
}

fn write(
    hexlog: &mut hexlog::Hexlog<'_>,
    key: Option<&str>,
    items: Vec<hexlog::AppendItem>,
) -> Vec<Target> {
    hexlog
        .append(&request("acme", key, items))
        .expect("the reference batch is appended")
        .records
        .into_iter()
        .map(|record| Target {
            id: record.id,
            seq: record.seq,
        })
        .collect()
}

/// Grava `extra_nodes` nós (`x.0`, `x.1`, ...) e depois a cadeia de referência, no tenant `acme`.
/// O `harness` deve vir de `Harness::with_native_other`. O `Hexlog` que escreve é solto ao sair.
pub fn reference_chain(harness: &Harness, extra_nodes: usize) -> Reference {
    let mut hexlog = harness.open_acme();
    for n in 0..extra_nodes {
        write(
            &mut hexlog,
            None,
            vec![node(&format!("x.{n}"), "note", json!({}))],
        );
    }

    let mut first = write(
        &mut hexlog,
        Some("K1"),
        vec![
            node("a.one", "note", json!({"text": "one"})),
            with_external(
                with_snapshot(node("a.two", "strict-note", json!({})), commit('c')),
                "x",
            ),
        ],
    );
    let rc2 = first.remove(1);
    let rc1 = first.remove(0);
    let rc3 = write(
        &mut hexlog,
        None,
        vec![revise(
            node("a.one", "note", json!({"text": "two"})),
            &rc1.id,
        )],
    )
    .remove(0);
    let rc4 = write(
        &mut hexlog,
        None,
        vec![relation("after", "a.one", "n.other")],
    )
    .remove(0);
    let rc5 = write(&mut hexlog, None, vec![retire("a.two", &rc2.id)]).remove(0);
    let rc6 = write(
        &mut hexlog,
        None,
        vec![retire_relation("after", "a.one", "n.other", &rc4.id)],
    )
    .remove(0);

    Reference {
        tenant: "acme".to_owned(),
        path: harness.db_path(),
        records: [rc1, rc2, rc3, rc4, rc5, rc6],
    }
}

// ---------------------------------------------------------------- adulterações reutilizáveis

/// `UPDATE` de uma coluna do record RC`rc`. `column` é sempre um literal fixo do esquema.
pub fn set_column(
    tamper: &mut Tamper,
    reference: &Reference,
    rc: usize,
    column: &'static str,
    value: &str,
) -> u64 {
    let sql = format!("UPDATE records SET {column} = ? WHERE tenant = ? AND seq = ?");

    tamper.execute(&sql, &[value, &reference.tenant, &reference.seq_text(rc)])
}

fn stored_text_at(tamper: &mut Tamper, tenant: &str, seq: u64, column: &'static str) -> String {
    let sql = format!("SELECT CAST({column} AS TEXT) FROM records WHERE tenant = ? AND seq = ?");

    tamper
        .read_text(&sql, &[tenant, &seq.to_string()])
        .remove(0)
}

/// Troca só o texto do `body` do RC`rc`, sem recalcular o `hash`.
pub fn replace_body(tamper: &mut Tamper, reference: &Reference, rc: usize, body: &str) -> u64 {
    set_column(tamper, reference, rc, "body", body)
}

/// Troca o `body` do record de `seq` e grava o `hash` recalculado com `hexlog::chain::hash_record`, num só `UPDATE`.
pub fn rewrite_body_at(tamper: &mut Tamper, tenant: &str, seq: u64, body: &str) -> u64 {
    let prev_hash = stored_text_at(tamper, tenant, seq, "prev_hash");
    let hash = hash_record(&prev_hash, body);

    tamper.execute(
        "UPDATE records SET body = ?, hash = ? WHERE tenant = ? AND seq = ?",
        &[body, hash.as_str(), tenant, &seq.to_string()],
    )
}

/// Como `rewrite_body_at`, para o RC`rc`.
pub fn rewrite_body(tamper: &mut Tamper, reference: &Reference, rc: usize, body: &str) -> u64 {
    rewrite_body_at(tamper, &reference.tenant, reference.rc(rc).seq, body)
}

/// O corpo do record de `seq` lido como JSON, com `edit` aplicado e de volta em JCS.
fn edited_body_at(
    tamper: &mut Tamper,
    tenant: &str,
    seq: u64,
    edit: impl FnOnce(&mut Value),
) -> String {
    let stored = tamper.read_body(tenant, seq);
    let mut body: Value = serde_json::from_str(&stored).expect("the stored body is JSON");
    edit(&mut body);

    canonical(&body).expect("the edited body has a canonical form")
}

/// O corpo do RC`rc` lido como JSON, com `edit` aplicado e de volta em JCS.
fn edited_body(
    tamper: &mut Tamper,
    reference: &Reference,
    rc: usize,
    edit: impl FnOnce(&mut Value),
) -> String {
    edited_body_at(tamper, &reference.tenant, reference.rc(rc).seq, edit)
}

/// Edita o corpo do record de `seq` mantendo-o canônico e recalcula o `hash`.
pub fn rewrite_json_at(
    tamper: &mut Tamper,
    tenant: &str,
    seq: u64,
    edit: impl FnOnce(&mut Value),
) -> u64 {
    let body = edited_body_at(tamper, tenant, seq, edit);

    rewrite_body_at(tamper, tenant, seq, &body)
}

/// Como `rewrite_json_at`, para o RC`rc`.
pub fn rewrite_json(
    tamper: &mut Tamper,
    reference: &Reference,
    rc: usize,
    edit: impl FnOnce(&mut Value),
) -> u64 {
    rewrite_json_at(tamper, &reference.tenant, reference.rc(rc).seq, edit)
}

/// Troca de lugar os dois primeiros membros de um objeto JSON, no texto, e deixa o resto como está.
fn swap_first_two_members(object: &str) -> String {
    let inner = &object[1..object.len() - 1];
    let mut members = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    let (mut in_string, mut escaped) = (false, false);
    for (at, ch) in inner.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                members.push(&inner[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    members.push(&inner[start..]);
    members.swap(0, 1);

    format!("{{{}}}", members.join(","))
}

fn batch_k1(tamper: &mut Tamper, reference: &Reference, set: &'static str, binds: &[&str]) -> u64 {
    let sql = format!("UPDATE batches SET {set} WHERE tenant = ? AND key = 'K1'");
    let mut all: Vec<&str> = binds.to_vec();
    all.push(&reference.tenant);

    tamper.execute(&sql, &all)
}

fn entry(
    id: &'static str,
    reason: BreakReason,
    seq: ExpectedSeq,
    target: usize,
    apply: fn(&mut Tamper, &Reference) -> u64,
) -> Mutation {
    Mutation {
        id,
        reason,
        seq,
        target,
        apply,
    }
}

/// As 34 linhas da tabela de mutações do plano (seção 4.5), na ordem da tabela, com as precisões do contrato.
pub fn mutations() -> Vec<Mutation> {
    use BreakReason::{
        AnchorMismatch, BatchIndexMismatch, ChainHeadNotFound, ColumnMismatch, DivergingSeq,
        HashMismatch, InvalidBody,
    };
    use ExpectedSeq::{Absent, Target, TargetPlusOne};

    vec![
        entry("seq", DivergingSeq, TargetPlusOne, 3, |t, r| {
            set_column(t, r, 3, "seq", "99")
        }),
        entry("prev_hash", HashMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "prev_hash", &"0".repeat(64))
        }),
        entry("hash", HashMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "hash", &"f".repeat(64))
        }),
        entry("body_content", HashMismatch, Target, 3, |t, r| {
            let body = edited_body(t, r, 3, |body| body["data"]["text"] = json!("six"));
            replace_body(t, r, 3, &body)
        }),
        entry("id", ColumnMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "id", unused_record_id().as_str())
        }),
        entry("at", ColumnMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "at", "1999-01-01T00:00:00.000Z")
        }),
        entry("type", ColumnMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "type", "other")
        }),
        entry("lineage_kind", ColumnMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "lineage_kind", "relation")
        }),
        entry("lineage_key", ColumnMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "lineage_key", "a.other")
        }),
        entry("retired", ColumnMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "retired", "1")
        }),
        entry("revises", ColumnMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "revises", unused_record_id().as_str())
        }),
        entry("def_hash", ColumnMismatch, Target, 3, |t, r| {
            set_column(t, r, 3, "def_hash", &"0".repeat(64))
        }),
        entry("relation_type", ColumnMismatch, Target, 4, |t, r| {
            set_column(t, r, 4, "relation_type", "other-relation-type")
        }),
        entry("from_id", ColumnMismatch, Target, 4, |t, r| {
            set_column(t, r, 4, "from_id", "a.two")
        }),
        // O RC4 já aponta para `n.other`: o valor da tabela do plano seria uma adulteração sem efeito.
        entry("to_id", ColumnMismatch, Target, 4, |t, r| {
            set_column(t, r, 4, "to_id", "a.two")
        }),
        entry("from_origin", ColumnMismatch, Target, 4, |t, r| {
            set_column(t, r, 4, "from_origin", "native")
        }),
        entry("to_origin", ColumnMismatch, Target, 4, |t, r| {
            set_column(t, r, 4, "to_origin", "tenant")
        }),
        entry("snapshot_kind", ColumnMismatch, Target, 2, |t, r| {
            let sql = "UPDATE records SET snapshot_kind = NULL WHERE tenant = ? AND seq = ?";
            t.execute(sql, &[&r.tenant, &r.seq_text(2)])
        }),
        entry("snapshot_id", ColumnMismatch, Target, 2, |t, r| {
            set_column(t, r, 2, "snapshot_id", &"0".repeat(40))
        }),
        entry("external_id", ColumnMismatch, Target, 2, |t, r| {
            set_column(t, r, 2, "external_id", "zzz")
        }),
        entry("body_not_canonical", InvalidBody, Target, 6, |t, r| {
            let stored = t.read_body(&r.tenant, r.rc(6).seq);
            rewrite_body(t, r, 6, &swap_first_two_members(&stored))
        }),
        entry("body_not_json", InvalidBody, Target, 6, |t, r| {
            rewrite_body(t, r, 6, "not json")
        }),
        entry("body_tenant", ColumnMismatch, Target, 6, |t, r| {
            rewrite_json(t, r, 6, |body| body["tenant"] = json!("zz"))
        }),
        // O `seq` do corpo é o do alvo mais um (7 na cadeia sem nós extras): um valor fixo coincidiria com o `seq` real de alguns bancos.
        entry("body_seq", ColumnMismatch, Target, 6, |t, r| {
            let wrong = r.rc(6).seq + 1;
            rewrite_json(t, r, 6, |body| body["seq"] = json!(wrong))
        }),
        entry("tenants_anchor", AnchorMismatch, Absent, 0, |t, r| {
            t.execute(
                "UPDATE tenants SET anchor = ? WHERE tenant = ?",
                &[&"0".repeat(64), &r.tenant],
            )
        }),
        entry("tenants_anchor_doc", AnchorMismatch, Absent, 0, |t, r| {
            let doc = t
                .read_text(
                    "SELECT CAST(anchor_doc AS TEXT) FROM tenants WHERE tenant = ?",
                    &[&r.tenant],
                )
                .remove(0);
            assert!(
                doc.contains(DEFAULT_INSTANT),
                "the anchor doc carries created_at"
            );
            t.execute(
                "UPDATE tenants SET anchor_doc = ? WHERE tenant = ?",
                &[&doc.replace(DEFAULT_INSTANT, OTHER_INSTANT), &r.tenant],
            )
        }),
        entry("tenants_created_at", AnchorMismatch, Absent, 0, |t, r| {
            t.execute(
                "UPDATE tenants SET created_at = ? WHERE tenant = ?",
                &[OTHER_INSTANT, &r.tenant],
            )
        }),
        entry("batches_key", BatchIndexMismatch, Absent, 0, |t, r| {
            batch_k1(t, r, "key = 'K9'", &[])
        }),
        entry(
            "batches_fingerprint",
            BatchIndexMismatch,
            Absent,
            0,
            |t, r| batch_k1(t, r, "fingerprint = ?", &[&"0".repeat(64)]),
        ),
        entry(
            "batches_first_seq",
            BatchIndexMismatch,
            Absent,
            0,
            |t, r| {
                let moved = (r.rc(1).seq + 1).to_string();
                batch_k1(t, r, "first_seq = ?", &[&moved])
            },
        ),
        entry(
            "batches_item_count",
            BatchIndexMismatch,
            Absent,
            0,
            |t, r| batch_k1(t, r, "item_count = 3", &[]),
        ),
        entry(
            "batches_row_missing",
            BatchIndexMismatch,
            Absent,
            0,
            |t, r| {
                t.execute(
                    "DELETE FROM batches WHERE tenant = ? AND key = 'K1'",
                    &[&r.tenant],
                )
            },
        ),
        entry(
            "batches_row_extra",
            BatchIndexMismatch,
            Absent,
            0,
            |t, r| {
                t.execute(
                    "INSERT INTO batches (tenant, key, fingerprint, first_seq, item_count) \
                 VALUES (?, 'K8', ?, ?, 1)",
                    &[&r.tenant, &"0".repeat(64), &r.seq_text(1)],
                )
            },
        ),
        entry(EXPECTED_HEAD_ID, ChainHeadNotFound, Absent, 0, |_, _| 1),
    ]
}
