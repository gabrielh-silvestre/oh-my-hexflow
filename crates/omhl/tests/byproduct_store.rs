//! Blob store local por hash (S6): `<db>/blobs/<2 primeiros hex>/<hash>`, gravação atômica e leitura conferida.

use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use omhl::byproduct::{ByproductErrorKind, ByproductHash, ByproductStore};
use omhl::ports::blob::BlobStore;

// NIST "abc"
const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
// NIST, 448 bits
const TWO_BLOCK: &[u8] = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
const TWO_BLOCK_SHA256: &str = "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn store() -> (tempfile::TempDir, BlobStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = BlobStore::new(dir.path());

    (dir, store)
}

fn blob_path(db: &Path, hash: &str) -> PathBuf {
    db.join("blobs").join(&hash[..2]).join(hash)
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();

    names
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn hash(text: &str) -> ByproductHash {
    ByproductHash::parse(text).unwrap()
}

#[test]
fn it_should_store_and_read_back_a_blob_by_its_sha256() {
    let (dir, store) = store();

    let stored = store.put(b"abc").unwrap();
    let read_back = store.get(&stored);

    assert_eq!(stored.as_str(), ABC_SHA256);
    assert_eq!(read_back, Ok(Some(b"abc".to_vec())));
    assert!(blob_path(dir.path(), ABC_SHA256).exists());
}

#[test]
fn it_should_hash_a_known_vector() {
    let (_dir, store) = store();

    let stored = store.put(TWO_BLOCK).unwrap();

    assert_eq!(stored.as_str(), TWO_BLOCK_SHA256);
}

#[test]
fn it_should_be_idempotent_when_the_same_bytes_are_stored_twice() {
    let (dir, store) = store();
    let path = blob_path(dir.path(), ABC_SHA256);
    let first = store.put(b"abc").unwrap();
    let inode_before = fs::metadata(&path).unwrap().ino();

    let second = store.put(b"abc").unwrap();

    assert_eq!(first.as_str(), ABC_SHA256);
    assert_eq!(second.as_str(), ABC_SHA256);
    assert_eq!(fs::metadata(&path).unwrap().ino(), inode_before);
    assert_eq!(entries(&dir.path().join("blobs/ba")), [ABC_SHA256]);
    assert_eq!(fs::read(&path).unwrap(), b"abc");
}

#[test]
fn it_should_create_the_store_with_private_permissions() {
    let (dir, store) = store();

    store.put(b"abc").unwrap();

    assert_eq!(mode(&dir.path().join("blobs")), 0o700);
    assert_eq!(mode(&dir.path().join("blobs/ba")), 0o700);
    assert_eq!(mode(&blob_path(dir.path(), ABC_SHA256)), 0o600);
}

#[test]
fn it_should_return_none_for_an_unknown_hash() {
    let (_dir, store) = store();
    let on_empty_store = store.get(&hash(EMPTY_SHA256));
    store.put(b"abc").unwrap();

    let missing_prefix_folder = store.get(&hash(TWO_BLOCK_SHA256));
    let missing_file = store.get(&hash(&("ba".to_owned() + &"0".repeat(62))));

    assert_eq!(on_empty_store, Ok(None));
    assert_eq!(missing_prefix_folder, Ok(None));
    assert_eq!(missing_file, Ok(None));
}

#[test]
fn it_should_reject_a_hash_that_is_not_64_lowercase_hex() {
    let invalid = [
        ABC_SHA256.to_uppercase(),
        ABC_SHA256[..63].to_owned(),
        ABC_SHA256.to_owned() + "0",
        "g".repeat(64),
        String::new(),
        "../".to_owned() + &"a".repeat(61),
    ];

    let kinds: Vec<_> = invalid
        .iter()
        .map(|text| ByproductHash::parse(text).err().map(|error| error.kind))
        .collect();
    let valid = ByproductHash::parse(ABC_SHA256);

    assert_eq!(kinds, vec![Some(ByproductErrorKind::InvalidHash); 6]);
    assert_eq!(
        valid.map(|parsed| parsed.as_str().to_owned()),
        Ok(ABC_SHA256.to_owned())
    );
}

#[test]
fn it_should_not_leave_a_temporary_file_after_a_successful_put() {
    let (dir, store) = store();

    store.put(b"abc").unwrap();

    assert_eq!(entries(&dir.path().join("blobs")), ["ba"]);
    assert_eq!(entries(&dir.path().join("blobs/ba")), [ABC_SHA256]);
}

#[test]
fn it_should_store_two_different_blobs_under_different_paths() {
    let (dir, store) = store();

    store.put(b"abc").unwrap();
    store.put(TWO_BLOCK).unwrap();

    assert_eq!(fs::read(blob_path(dir.path(), ABC_SHA256)).unwrap(), b"abc");
    assert_eq!(
        fs::read(blob_path(dir.path(), TWO_BLOCK_SHA256)).unwrap(),
        TWO_BLOCK
    );
    assert_eq!(entries(&dir.path().join("blobs")), ["24", "ba"]);
}

#[test]
fn it_should_refuse_a_blob_whose_bytes_do_not_match_its_hash() {
    let (dir, store) = store();
    store.put(b"abc").unwrap();
    fs::write(blob_path(dir.path(), ABC_SHA256), b"abd").unwrap();

    let result = store.get(&hash(ABC_SHA256));

    assert_eq!(
        result.map_err(|error| error.kind),
        Err(ByproductErrorKind::Corrupt)
    );
}

#[test]
fn it_should_store_a_blob_when_a_stale_temporary_file_of_an_earlier_run_exists() {
    let (dir, store) = store();
    let prefix_dir = dir.path().join("blobs/ba");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&prefix_dir)
        .unwrap();
    let pid = std::process::id();
    let stale_names = [
        format!("{ABC_SHA256}.tmp-{pid}-0"),
        format!("{ABC_SHA256}.tmp-{pid}-1"),
    ];
    for name in &stale_names {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(prefix_dir.join(name))
            .unwrap();
        std::io::Write::write_all(&mut file, b"stale").unwrap();
    }

    let stored = store.put(b"abc");

    assert_eq!(
        stored.map(|put| put.as_str().to_owned()),
        Ok(ABC_SHA256.to_owned())
    );
    assert_eq!(fs::read(blob_path(dir.path(), ABC_SHA256)).unwrap(), b"abc");
    for name in &stale_names {
        assert_eq!(fs::read(prefix_dir.join(name)).unwrap(), b"stale");
    }
    assert_eq!(
        entries(&prefix_dir),
        [
            ABC_SHA256.to_owned(),
            stale_names[0].clone(),
            stale_names[1].clone()
        ]
    );
}
