use std::path::PathBuf;

use hexlog::{Catalog, Config, Deps, Hexlog, NativeNodes};
use tempfile::TempDir;

use super::{FixedClock, SeqIds};

/// Donos do que um `Hexlog` empresta: diretório temporário, relógio e ids.
pub struct RawHarness {
    pub dir: TempDir,
    pub clock: FixedClock,
    pub ids: SeqIds,
}

impl RawHarness {
    pub fn new() -> RawHarness {
        RawHarness {
            dir: TempDir::new().expect("temporary directory is created"),
            clock: FixedClock::default(),
            ids: SeqIds::new(),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.dir.path().join("hexlog.db")
    }

    pub fn config(&self) -> Config {
        Config::new(self.db_path())
    }

    /// Abre o `hexlog` sem criar tenant: o teste chama `ensure_tenant`.
    pub fn hexlog<'a>(&'a self, catalog: Catalog, native: &'a dyn NativeNodes) -> Hexlog<'a> {
        let deps = Deps {
            clock: &self.clock,
            ids: &self.ids,
            catalog,
            native,
        };

        Hexlog::open(&self.config(), deps).expect("hexlog opens")
    }
}

impl Default for RawHarness {
    fn default() -> RawHarness {
        RawHarness::new()
    }
}
