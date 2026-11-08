use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::Path;

use tempfile::Builder;

use super::{ProjectedFile, ProjectionError, WriteReport};
use crate::extensions::files::list_sorted_capped;
use crate::project::layout::{
    GENERATED_HEADER, GITIGNORE_CONTENT, GITIGNORE_NAME, LOCK_NAME, OMHL_DIR, PROJECTION_DIR,
    PROJECTION_DIR_ENTRIES_MAX, TMP_PREFIX, TMP_SUFFIX,
};

/// Writes the projection under `<root>/.omhl/projection`: preflight first, then atomic
/// replacement of every changed file, then removal of generated orphans.
pub fn write(root: &Path, files: &[ProjectedFile]) -> Result<WriteReport, ProjectionError> {
    let omhl = root.join(OMHL_DIR);
    ensure_directory(&omhl, OMHL_DIR)?;
    let dir = omhl.join(PROJECTION_DIR);
    let dir_rel = format!("{OMHL_DIR}/{PROJECTION_DIR}");
    ensure_directory(&dir, &dir_rel)?;
    let _lock = acquire_lock(&dir)?;
    let listing = list_sorted_capped(&dir, PROJECTION_DIR_ENTRIES_MAX).map_err(|code| {
        error(
            dir_rel.clone(),
            code,
            "the projection directory cannot be listed",
        )
    })?;
    sweep_temporaries(&dir, &listing)?;
    for file in files {
        require_generated_or_absent(&dir, &file.name)?;
    }
    let mut report = replace_changed(&dir, files)?;
    write_if_different(&dir, GITIGNORE_NAME, GITIGNORE_CONTENT.as_bytes())?;
    report.removed = remove_orphans(&dir, &listing, files)?;
    for names in [
        &mut report.written,
        &mut report.unchanged,
        &mut report.removed,
    ] {
        names.sort();
    }
    File::open(&dir)
        .and_then(|handle| handle.sync_all())
        .map_err(io_error(&|| dir_rel.clone()))?;
    Ok(report)
}

fn replace_changed(dir: &Path, files: &[ProjectedFile]) -> Result<WriteReport, ProjectionError> {
    let mut report = WriteReport::default();
    for file in files {
        let target = if write_if_different(dir, &file.name, file.content.as_bytes())? {
            &mut report.written
        } else {
            &mut report.unchanged
        };
        target.push(file.name.clone());
    }
    Ok(report)
}

fn relative(name: &str) -> String {
    format!("{OMHL_DIR}/{PROJECTION_DIR}/{name}")
}

fn error(path: String, code: &'static str, message: &str) -> ProjectionError {
    ProjectionError {
        path,
        code,
        message: message.to_owned(),
    }
}

/// `rel` only runs when an error is built, so the happy path never allocates the path.
fn io_error(rel: &dyn Fn() -> String) -> impl Fn(io::Error) -> ProjectionError + '_ {
    move |_| error(rel(), "io-error", "the file system refused the operation")
}

/// `None` when the path is absent; a symlink is always refused, before anything is opened.
fn inspect(path: &Path, rel: &dyn Fn() -> String) -> Result<Option<Metadata>, ProjectionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_symlink() => {
            Err(error(rel(), "symlink", "symbolic links are not followed"))
        }
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error(rel)(source)),
    }
}

fn existing_file(
    path: &Path,
    rel: &dyn Fn() -> String,
) -> Result<Option<Metadata>, ProjectionError> {
    match inspect(path, rel)? {
        Some(metadata) if !metadata.is_file() => {
            Err(error(rel(), "not-regular-file", "expected a regular file"))
        }
        other => Ok(other),
    }
}

fn ensure_directory(path: &Path, rel: &str) -> Result<(), ProjectionError> {
    let rel = || rel.to_owned();
    match inspect(path, &rel)? {
        Some(metadata) if !metadata.is_dir() => {
            Err(error(rel(), "not-regular-file", "expected a directory"))
        }
        Some(_) => Ok(()),
        None => fs::create_dir(path).map_err(io_error(&rel)),
    }
}

/// The lock lives as long as the returned handle.
fn acquire_lock(dir: &Path) -> Result<File, ProjectionError> {
    let rel = || relative(LOCK_NAME);
    let path = dir.join(LOCK_NAME);
    existing_file(&path, &rel)?;
    let lock = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(io_error(&rel))?;
    match lock.try_lock() {
        Ok(()) => Ok(lock),
        Err(TryLockError::WouldBlock) => Err(error(
            rel(),
            "projection-busy",
            "another process is writing the projection",
        )),
        Err(TryLockError::Error(source)) => Err(io_error(&rel)(source)),
    }
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn has_generated_header(path: &Path) -> io::Result<bool> {
    let mut head = [0; GENERATED_HEADER.len() + 1];
    let mut file = File::open(path)?;
    let mut filled = 0;
    while filled < head.len() {
        match file.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) => return Err(source),
        }
    }
    Ok(head[..filled]
        .strip_prefix(GENERATED_HEADER.as_bytes())
        .is_some_and(|rest| rest.first().is_none_or(|byte| *byte == b'\n')))
}

fn sweep_temporaries(dir: &Path, listing: &[OsString]) -> Result<(), ProjectionError> {
    let temporaries = listing
        .iter()
        .filter_map(|name| name.to_str())
        .filter(|name| name.starts_with(TMP_PREFIX) && name.ends_with(TMP_SUFFIX));
    for name in temporaries {
        let path = dir.join(name);
        if is_regular_file(&path) {
            fs::remove_file(&path).map_err(io_error(&|| relative(name)))?;
        }
    }
    Ok(())
}

fn require_generated_or_absent(dir: &Path, name: &str) -> Result<(), ProjectionError> {
    let rel = || relative(name);
    let path = dir.join(name);
    if existing_file(&path, &rel)?.is_none()
        || has_generated_header(&path).map_err(io_error(&rel))?
    {
        return Ok(());
    }
    Err(error(
        rel(),
        "not-generated",
        "the file was not generated by omhl",
    ))
}

/// `true` when the file was replaced, `false` when the bytes on disk were already equal.
fn write_if_different(dir: &Path, name: &str, bytes: &[u8]) -> Result<bool, ProjectionError> {
    let rel = || relative(name);
    let target = dir.join(name);
    if let Some(metadata) = existing_file(&target, &rel)? {
        let same_size = metadata.len() == bytes.len() as u64;
        if same_size && fs::read(&target).map_err(io_error(&rel))? == bytes {
            return Ok(false);
        }
    }
    let io = io_error(&rel);
    let mut builder = Builder::new();
    builder.prefix(TMP_PREFIX).suffix(TMP_SUFFIX);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o444));
    }
    let mut temporary = builder.tempfile_in(dir).map_err(&io)?;
    temporary.write_all(bytes).map_err(&io)?;
    // `persist` renames without flushing, so the data is synced here.
    temporary.as_file().sync_all().map_err(&io)?;
    temporary
        .persist(&target)
        .map_err(|failure| io(failure.error))?;
    Ok(true)
}

fn remove_orphans(
    dir: &Path,
    listing: &[OsString],
    files: &[ProjectedFile],
) -> Result<Vec<String>, ProjectionError> {
    let mut removed = Vec::new();
    for name in listing.iter().filter_map(|name| name.to_str()) {
        let listed = files.iter().any(|file| file.name == name);
        if listed || !name.ends_with(".md") {
            continue;
        }
        let path = dir.join(name);
        if !is_regular_file(&path) {
            continue;
        }
        let rel = || relative(name);
        if has_generated_header(&path).map_err(io_error(&rel))? {
            fs::remove_file(&path).map_err(io_error(&rel))?;
            removed.push(name.to_owned());
        }
    }
    Ok(removed)
}
