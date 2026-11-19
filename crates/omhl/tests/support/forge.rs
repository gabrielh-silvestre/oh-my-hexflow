use std::fs;

use hexlog::{AppendItem, AppendRequest, Author, Config, Deps, Hexlog};

use super::omhl::acme;
use super::project::Project;
use super::{EmptyNative, FixedClock, SeqIds};

/// Grava direto no banco do projeto, sem a guarda do `omhl` e sem a base nativa: monta o que a CLI recusa
/// (um check de id reservado por um nativo, um work item já fechado, uma attestation sem git).
/// Uma chamada por projeto: os ids dos records são sequenciais e recomeçam a cada chamada.
pub fn forge(project: &Project, author: Author, items: Vec<AppendItem>) {
    let db_path = project.db_path();
    fs::create_dir_all(db_path.parent().expect("the database has a folder"))
        .expect("the data folder is created");
    let clock = FixedClock::default();
    let ids = SeqIds::new();
    let deps = Deps {
        clock: &clock,
        ids: &ids,
        catalog: ::omhl::graphs::catalog().expect("the embedded catalogs build"),
        native: &EmptyNative,
    };
    let mut hexlog = Hexlog::open(&Config::new(db_path), deps).expect("the database opens");
    hexlog
        .ensure_tenant(&acme())
        .expect("the tenant is created");

    hexlog
        .append(&AppendRequest {
            tenant: acme(),
            author,
            key: None,
            items,
        })
        .expect("the forged append is accepted");
}
