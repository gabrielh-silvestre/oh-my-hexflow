use std::ffi::OsString;
use std::path::Path;

use ::omhl::facade::SessionSource;
use ::omhl::graphs;
use ::omhl::native::NativeBase;
use hexlog::{Config, Deps, Hexlog, Lineage};

use super::{FixedClock, SeqIds, tenant};

/// Um registro como o banco o guarda: a linhagem, se é retirada e a sessão que o gravou.
#[derive(Debug, PartialEq, Eq)]
pub struct Stamp {
    pub lineage: String,
    pub retired: bool,
    pub session: Option<String>,
}

impl Stamp {
    pub fn new(lineage: &str, retired: bool, session: &str) -> Stamp {
        Stamp {
            lineage: lineage.to_owned(),
            retired,
            session: Some(session.to_owned()),
        }
    }
}

fn with_hexlog<T>(db_path: &Path, read: impl FnOnce(&mut Hexlog<'_>) -> T) -> T {
    let clock = FixedClock::default();
    let ids = SeqIds::new();
    let deps = Deps {
        clock: &clock,
        ids: &ids,
        catalog: graphs::catalog().expect("the embedded catalogs build"),
        native: &NativeBase,
    };
    let mut hexlog = Hexlog::open(&Config::new(db_path.to_path_buf()), deps).expect("hexlog opens");

    read(&mut hexlog)
}

/// Todo registro do tenant `acme` em ordem de `seq`; o `omhl` não imprime a sessão, só a varredura do `hexlog` a lê.
pub fn stamps(db_path: &Path) -> Vec<Stamp> {
    with_hexlog(db_path, |hexlog| {
        let mut found = Vec::new();
        hexlog
            .scan_records(&tenant("acme"), |record| {
                let lineage = match record.lineage {
                    Lineage::Node(id) => id.as_str().to_owned(),
                    Lineage::Relation { from, to, .. } => {
                        format!("{}>{}", from.as_str(), to.as_str())
                    }
                };
                found.push(Stamp {
                    lineage,
                    retired: record.retired,
                    session: record.session.map(|label| label.as_str().to_owned()),
                });
            })
            .expect("the scan runs");

        found
    })
}

pub fn raw_session(text: &str) -> SessionSource {
    SessionSource::Raw(OsString::from(text))
}

pub fn chain_head(db_path: &Path) -> (u64, String) {
    with_hexlog(db_path, |hexlog| {
        let head = hexlog
            .chain_head(&tenant("acme"))
            .expect("the head is read");

        (head.seq, head.hash.as_str().to_owned())
    })
}
