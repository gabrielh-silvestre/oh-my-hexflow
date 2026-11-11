use std::path::{Path, PathBuf};

use crate::byproduct::{ByproductError, ByproductHash, ByproductStore};

pub struct BlobStore {
    #[allow(dead_code)] // S6 lê root (`ports/blob.rs`)
    root: PathBuf,
}

impl BlobStore {
    pub fn new(db_dir: &Path) -> BlobStore {
        BlobStore {
            root: db_dir.join("blobs"),
        }
    }
}

impl ByproductStore for BlobStore {
    fn put(&self, _bytes: &[u8]) -> Result<ByproductHash, ByproductError> {
        unimplemented!()
    }

    fn get(&self, _hash: &ByproductHash) -> Result<Option<Vec<u8>>, ByproductError> {
        unimplemented!()
    }
}
