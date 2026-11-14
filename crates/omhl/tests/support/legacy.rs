//! Dado de antes do PR: grava direto no `hexlog`, sem a guarda do `omhl`, que hoje recusa uma
//! premissa `confirmed` sem `confirmed_by`.

use std::path::Path;

use hexlog::{Config, Deps, Hexlog};
use serde_json::json;

use super::omhl::{acme, node_item, request};
use super::raw::RawHarness;
use super::{EmptyNative, FixedClock, SeqIds};

/// Grava a premissa `id` com `status: confirmed` e sem `confirmed_by`, no banco que o CLI já usa.
pub fn seed_legacy_confirmed_premise(db_path: &Path, id: &str, work_item: &str, kind: &str) {
    let clock = FixedClock::default();
    let ids = SeqIds::new();
    let deps = Deps {
        clock: &clock,
        ids: &ids,
        catalog: ::omhl::graphs::catalog().expect("the embedded catalogs build"),
        native: &EmptyNative,
    };
    let mut hexlog = Hexlog::open(&Config::new(db_path.to_path_buf()), deps)
        .expect("the legacy writer opens the database");
    let data = json!({ "text": "t", "status": "confirmed", "kind": kind, "work_item": work_item });

    hexlog
        .append(&request(vec![node_item(id, "premise", data)]))
        .expect("the legacy premise is written");
}

/// Grava a diretiva `id` sem `confirmed_by`, por um cliente que não é o `loader`, no banco do `raw`
/// (que ainda não tem tenant): a guarda do `omhl` recusa esse registro. Compartilha os ids do `raw`
/// para o `Omhl` aberto depois não repetir um id de record.
pub fn seed_unconfirmed_guideline(raw: &RawHarness, id: &str, kind: &str) {
    let catalog = ::omhl::graphs::catalog().expect("the embedded catalogs build");
    let mut hexlog = raw.hexlog(catalog, &EmptyNative);
    hexlog
        .ensure_tenant(&acme())
        .expect("the tenant is created");
    let data = json!({ "text": "keep it simple", "kind": kind });

    hexlog
        .append(&request(vec![node_item(id, "guideline", data)]))
        .expect("the unconfirmed guideline is written");
}
