use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time::{Duration, SystemTime};

use omhl::byproduct::{
    ByproductError, ByproductErrorKind, ByproductHash, ByproductRetention, ByproductStore,
};
use omhl::ports::blob::BlobStore;
use tempfile::TempDir;

use crate::support::omhl::acme;

// NIST "abc"
const ABC: &[u8] = b"abc";
const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const WITHIN: Duration = Duration::from_secs(5);

fn blob_path(db: &Path) -> PathBuf {
    db.join("blobs/by-db/omhl.db/acme")
        .join(&ABC_SHA256[..2])
        .join(ABC_SHA256)
}

fn abc_hash() -> ByproductHash {
    ByproductHash::parse(ABC_SHA256).expect("a valid hash")
}

/// Abrir um FIFO sem escritor trava para sempre: o `get`/`put` roda fora da thread do teste.
fn within_deadline<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, RecvTimeoutError> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(work());
    });

    receiver.recv_timeout(WITHIN)
}

fn make_fifo_at_blob_path(db: &Path) {
    let path = blob_path(db);
    fs::create_dir_all(path.parent().expect("the blob has a folder")).expect("folder is created");
    let made = Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo runs");
    assert!(made.success(), "mkfifo failed");
}

fn kind_of<T>(result: Result<T, ByproductError>) -> Result<(), ByproductErrorKind> {
    result.map(|_| ()).map_err(|error| error.kind)
}

#[test]
fn it_should_report_a_blob_that_is_a_fifo_as_corrupt_without_blocking() {
    let db = TempDir::new().expect("temporary directory is created");
    make_fifo_at_blob_path(db.path());
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());

    let result = within_deadline(move || kind_of(store.get(&abc_hash())));

    assert_eq!(result, Ok(Err(ByproductErrorKind::Corrupt)));
}

#[test]
fn it_should_replace_a_fifo_at_the_blob_path_with_a_regular_file_without_blocking() {
    let db = TempDir::new().expect("temporary directory is created");
    make_fifo_at_blob_path(db.path());
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());

    let result = within_deadline(move || store.put(ABC).map(|hash| hash.as_str().to_owned()));

    let stored = fs::symlink_metadata(blob_path(db.path())).expect("the blob exists");
    assert_eq!(
        (
            result,
            stored.is_file(),
            fs::read(blob_path(db.path())).ok()
        ),
        (Ok(Ok(ABC_SHA256.to_owned())), true, Some(ABC.to_vec()))
    );
}

#[test]
fn it_should_report_a_symlink_at_the_blob_path_as_corrupt() {
    let db = TempDir::new().expect("temporary directory is created");
    let elsewhere = TempDir::new().expect("temporary directory is created");
    let real = elsewhere.path().join("real");
    fs::write(&real, ABC).expect("the real file is written");
    let path = blob_path(db.path());
    fs::create_dir_all(path.parent().expect("the blob has a folder")).expect("folder is created");
    std::os::unix::fs::symlink(&real, &path).expect("the symlink is created");
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());

    let result = kind_of(store.get(&abc_hash()));

    assert_eq!(result, Err(ByproductErrorKind::Corrupt));
}

#[test]
fn it_should_replace_a_symlink_at_the_blob_path_without_writing_through_it() {
    let db = TempDir::new().expect("temporary directory is created");
    let elsewhere = TempDir::new().expect("temporary directory is created");
    let real = elsewhere.path().join("real");
    fs::write(&real, b"untouched").expect("the real file is written");
    let path = blob_path(db.path());
    fs::create_dir_all(path.parent().expect("the blob has a folder")).expect("folder is created");
    std::os::unix::fs::symlink(&real, &path).expect("the symlink is created");
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());

    let stored = store.put(ABC).map(|hash| hash.as_str().to_owned());

    let blob = fs::symlink_metadata(&path).expect("the blob exists");
    assert_eq!(
        (
            stored,
            blob.is_file(),
            fs::read(&real).expect("the real file is read")
        ),
        (Ok(ABC_SHA256.to_owned()), true, b"untouched".to_vec())
    );
}

#[test]
fn it_should_return_none_for_a_hash_that_was_never_stored() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());

    let result = store.get(&abc_hash());

    assert_eq!(result, Ok(None));
}

#[test]
fn it_should_read_back_a_stored_blob() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());
    let hash = store.put(ABC).expect("the blob is stored");

    let result = store.get(&hash);

    assert_eq!(result, Ok(Some(ABC.to_vec())));
}

fn tenant_root(db: &Path, db_file: &str, tenant: &str) -> PathBuf {
    db.join("blobs/by-db").join(db_file).join(tenant)
}

fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .expect("the path exists")
        .permissions()
        .mode()
        & 0o777
}

fn other_tenant() -> hexlog::TenantId {
    hexlog::TenantId::parse("other").expect("a valid tenant")
}

fn legacy_blob_path(db: &Path) -> PathBuf {
    db.join("blobs").join(&ABC_SHA256[..2]).join(ABC_SHA256)
}

fn write_legacy_blob(db: &Path, bytes: &[u8]) {
    let path = legacy_blob_path(db);
    fs::create_dir_all(path.parent().expect("the blob has a folder")).expect("folder is created");
    fs::write(path, bytes).expect("the legacy blob is written");
}

#[test]
fn it_should_store_a_blob_under_the_folder_of_the_database_and_tenant_with_private_modes() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());

    store.put(ABC).expect("the blob is stored");

    let root = tenant_root(db.path(), "omhl.db", "acme");
    assert_eq!(
        (
            mode_of(&root),
            mode_of(&root.join(&ABC_SHA256[..2])),
            mode_of(&blob_path(db.path())),
            fs::read(blob_path(db.path())).ok(),
        ),
        (0o700, 0o700, 0o600, Some(ABC.to_vec()))
    );
}

#[test]
fn it_should_read_the_legacy_layout_when_the_new_folder_has_no_such_blob() {
    let db = TempDir::new().expect("temporary directory is created");
    write_legacy_blob(db.path(), ABC);
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());

    let result = store.get(&abc_hash());

    assert_eq!(result, Ok(Some(ABC.to_vec())));
}

#[test]
fn it_should_report_a_corrupt_blob_in_the_new_folder_without_falling_back_to_the_legacy_layout() {
    let db = TempDir::new().expect("temporary directory is created");
    write_legacy_blob(db.path(), ABC);
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());
    store.put(ABC).expect("the blob is stored");
    fs::write(blob_path(db.path()), b"abd").expect("the new blob is corrupted");

    let result = kind_of(store.get(&abc_hash()));

    assert_eq!(result, Err(ByproductErrorKind::Corrupt));
}

#[test]
fn it_should_keep_the_same_content_of_two_tenants_in_two_files() {
    let db = TempDir::new().expect("temporary directory is created");
    let db_file = db.path().join("omhl.db");

    BlobStore::new(&db_file, &acme()).put(ABC).expect("stored");
    BlobStore::new(&db_file, &other_tenant())
        .put(ABC)
        .expect("stored");

    let other = tenant_root(db.path(), "omhl.db", "other");
    assert!(blob_path(db.path()).is_file());
    assert!(other.join(&ABC_SHA256[..2]).join(ABC_SHA256).is_file());
}

#[test]
fn it_should_copy_a_blob_that_only_exists_in_the_legacy_layout_into_the_new_folder() {
    let db = TempDir::new().expect("temporary directory is created");
    write_legacy_blob(db.path(), ABC);
    let store = BlobStore::new(&db.path().join("omhl.db"), &acme());

    store.put(ABC).expect("the blob is stored");

    assert_eq!(
        (
            fs::read(blob_path(db.path())).ok(),
            fs::read(legacy_blob_path(db.path())).ok()
        ),
        (Some(ABC.to_vec()), Some(ABC.to_vec()))
    );
}

#[test]
fn it_should_not_let_two_databases_in_the_same_folder_read_each_others_blobs() {
    let db = TempDir::new().expect("temporary directory is created");
    let first = BlobStore::new(&db.path().join("omhl.db"), &acme());
    let second = BlobStore::new(&db.path().join("scratch.db"), &acme());
    first.put(ABC).expect("stored");

    let from_second = second.get(&abc_hash());
    second.put(ABC).expect("stored");

    let scratch = tenant_root(db.path(), "scratch.db", "acme");
    assert_eq!(from_second, Ok(None));
    assert!(blob_path(db.path()).is_file());
    assert!(scratch.join(&ABC_SHA256[..2]).join(ABC_SHA256).is_file());
}

// Renovação, poda e carimbo

const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn store_in(db: &TempDir) -> BlobStore {
    BlobStore::new(&db.path().join("omhl.db"), &acme())
}

fn set_modified(path: &Path, at: SystemTime) {
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("the file opens")
        .set_modified(at)
        .expect("the date is set");
}

fn modified(path: &Path) -> SystemTime {
    fs::metadata(path)
        .expect("the file exists")
        .modified()
        .expect("a date")
}

#[test]
fn it_should_renew_the_date_and_keep_the_inode_when_the_same_content_is_stored_again() {
    use std::os::unix::fs::MetadataExt;

    let db = TempDir::new().expect("temporary directory is created");
    let store = store_in(&db);
    store.put(ABC).expect("the blob is stored");
    let path = blob_path(db.path());
    let inode = fs::metadata(&path).expect("the blob exists").ino();
    set_modified(&path, SystemTime::now() - 3 * DAY);

    store.put(ABC).expect("the blob is stored again");

    let age = SystemTime::now()
        .duration_since(modified(&path))
        .unwrap_or_default();
    assert_eq!(
        (
            age < Duration::from_secs(60),
            fs::metadata(&path).map(|m| m.ino()).ok()
        ),
        (true, Some(inode))
    );
}

#[test]
fn it_should_not_remove_a_blob_whose_date_was_renewed_between_the_listing_and_the_removal() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = store_in(&db);
    store.put(ABC).expect("the blob is stored");
    set_modified(&blob_path(db.path()), SystemTime::now() - 8 * DAY);
    let listed = store.stale(WEEK).expect("the listing works");
    store.put(ABC).expect("a concurrent put renews the blob");

    let removed = listed.first().map(|blob| store.remove(blob, WEEK));

    assert_eq!((listed.len(), removed), (1, Some(Ok(false))));
    assert!(blob_path(db.path()).is_file());
}

#[test]
fn it_should_list_and_remove_a_blob_older_than_the_retention() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = store_in(&db);
    store.put(ABC).expect("the blob is stored");
    set_modified(&blob_path(db.path()), SystemTime::now() - 8 * DAY);

    let listed = store.stale(WEEK).expect("the listing works");
    let removed = listed.first().map(|blob| store.remove(blob, WEEK));

    assert_eq!(
        (listed.first().map(|blob| blob.bytes), removed),
        (Some(3), Some(Ok(true)))
    );
    assert!(!blob_path(db.path()).exists());
}

#[test]
fn it_should_not_list_a_blob_dated_in_the_future() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = store_in(&db);
    store.put(ABC).expect("the blob is stored");
    set_modified(&blob_path(db.path()), SystemTime::now() + DAY);

    let listed = store.stale(WEEK);

    assert_eq!(listed, Ok(Vec::new()));
}

#[test]
fn it_should_count_a_stamp_dated_in_the_future_as_expired() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = store_in(&db);
    store.put(ABC).expect("the blob is stored");
    let stamp = tenant_root(db.path(), "omhl.db", "acme").join(".pruned-at");
    fs::write(&stamp, b"").expect("the stamp is written");
    set_modified(&stamp, SystemTime::now() + DAY);

    let claimed = store.claim_prune_slot(DAY);

    let age = SystemTime::now()
        .duration_since(modified(&stamp))
        .unwrap_or_default();
    assert_eq!((claimed, age < Duration::from_secs(60)), (true, true));
}

#[test]
fn it_should_refuse_a_second_claim_inside_the_interval() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = store_in(&db);
    store.put(ABC).expect("the blob is stored");

    let first = store.claim_prune_slot(DAY);
    let second = store.claim_prune_slot(DAY);

    assert_eq!((first, second), (true, false));
}

#[test]
fn it_should_not_sweep_a_shard_that_is_a_symlink_to_a_folder_outside() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = store_in(&db);
    store.put(ABC).expect("the blob is stored");
    let outside = TempDir::new().expect("temporary directory is created");
    let foreign = outside.path().join(ABC_SHA256);
    fs::write(&foreign, b"outside").expect("the foreign file is written");
    set_modified(&foreign, SystemTime::now() - 30 * DAY);
    let root = tenant_root(db.path(), "omhl.db", "acme");
    std::os::unix::fs::symlink(outside.path(), root.join("cd")).expect("the symlink is created");

    let listed = store.stale(WEEK);

    assert_eq!(listed, Ok(Vec::new()));
    assert!(foreign.is_file());
}

#[test]
fn it_should_refuse_the_claim_and_create_nothing_when_the_folder_does_not_exist() {
    let db = TempDir::new().expect("temporary directory is created");
    let store = store_in(&db);

    let claimed = store.claim_prune_slot(DAY);

    assert_eq!(
        (claimed, fs::read_dir(db.path()).map(Iterator::count).ok()),
        (false, Some(0))
    );
}
