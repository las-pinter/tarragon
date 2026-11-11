//! Rename a file in its folder and in the database.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tarragon_cache::store::CacheStore;
use tarragon_core::file_name::{
    FileNameError, MAX_FILE_NAME_BYTES, split_file_name, validate_file_stem,
};
use tarragon_db::Database;

use super::{FileOpError, io_error};

/// Rename `old` to `new_stem` with the same extension and give the new path.
///
/// The extension is the one [`split_file_name`] finds, so `a.tar.gz` renamed to
/// `b` gives `b.gz`. The function refuses a name that is already used in the
/// folder, and it never overwrites a file. After the disk move it moves the
/// database rows. When that fails, it moves the file back and gives the error.
/// When the move back fails too, it gives [`FileOpError::RollbackFailed`].
/// After the commit it deletes the cache tier files of the old name and of a
/// stale row at the new name. A failure to delete one is logged only.
///
/// # Errors
///
/// Returns [`FileOpError::NotAFile`] when `old` is not a regular file,
/// [`FileOpError::InvalidName`] when its name is missing or not UTF-8,
/// [`FileOpError::Stem`] when the stem is refused, the whole new name is over
/// [`MAX_FILE_NAME_BYTES`] bytes, or a file has the new name,
/// [`FileOpError::Io`] when a disk call fails, [`FileOpError::Db`] when a
/// database call fails, and [`FileOpError::RollbackFailed`] as described above.
pub fn rename(
    db: &Database,
    cache: &CacheStore,
    old: &Path,
    new_stem: &str,
) -> Result<PathBuf, FileOpError> {
    rename_with(db, cache, old, new_stem, |_| {}, |_| {})
}

/// [`rename`] with two calls, so a test can make a file in a gap. `before_move`
/// runs after the name check and before the move. `before_rollback` runs after a
/// database failure and before the file moves back.
fn rename_with(
    db: &Database,
    cache: &CacheStore,
    old: &Path,
    new_stem: &str,
    before_move: impl FnOnce(&Path),
    before_rollback: impl FnOnce(&Path),
) -> Result<PathBuf, FileOpError> {
    let name = old
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| FileOpError::InvalidName(old.to_path_buf()))?;
    let is_file = fs::metadata(old)
        .map_err(|e| io_error("read", old, e))?
        .is_file();
    if !is_file {
        return Err(FileOpError::NotAFile(old.to_path_buf()));
    }
    let (_, extension) = split_file_name(name);
    let new_name = match extension {
        Some(extension) => format!("{new_stem}.{extension}"),
        None => new_stem.to_owned(),
    };
    let folder = old.parent().unwrap_or_else(|| Path::new(""));
    let new = folder.join(&new_name);
    // The check asks the disk about the whole new name, so it also sees a
    // file whose name differs from the listing only by case on some systems.
    validate_file_stem(new_stem, extension.unwrap_or(""), |_| {
        new.symlink_metadata().is_ok()
    })?;
    // The check above counts no trailing dot, so a name such as `a.` can pass
    // it one byte over the limit.
    if new_name.len() > MAX_FILE_NAME_BYTES {
        return Err(FileNameError::TooLong.into());
    }

    let old_text = db_text(old);
    let new_text = db_text(&new);
    let leftovers = tier_files(db, cache, folder, &[name, &new_name])?;

    before_move(&new);
    move_no_replace(old, &new)?;
    if let Err(db_error) = db.rename_file(&old_text, &new_text) {
        before_rollback(old);
        return Err(match move_no_replace(&new, old) {
            Ok(()) => db_error.into(),
            Err(back) => FileOpError::RollbackFailed {
                from: new,
                to: old.to_path_buf(),
                db: Box::new(db_error),
                source: Box::new(back),
            },
        });
    }
    for file in leftovers {
        if let Err(error) = fs::remove_file(&file)
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!("cannot remove the cache file {}: {error}", file.display());
        }
    }
    Ok(new)
}

/// The path text the database stores: forward slashes.
fn db_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The cache tier files of each named file in `folder`. A folder with no cache
/// id has none. A failed probe is logged and gives no files for that name.
fn tier_files(
    db: &Database,
    cache: &CacheStore,
    folder: &Path,
    names: &[&str],
) -> Result<Vec<PathBuf>, FileOpError> {
    let Some(uuid) = db.folder_uuid(&db_text(folder))? else {
        return Ok(Vec::new());
    };
    let folder_name = folder.file_name().map_or_else(
        || "root".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut files = Vec::new();
    for name in names {
        match cache.find_entry(&folder_name, &uuid, name) {
            Ok(Some(entry)) => files.extend([entry.thumb320, entry.preview1024, entry.full]),
            Ok(None) => {}
            Err(error) => tracing::warn!("cannot look up the cache files of {name}: {error}"),
        }
    }
    Ok(files)
}

/// Move `from` to `to` and never replace a file at `to`.
///
/// A hard link fails when `to` exists, and it is atomic, so no file is lost.
fn move_no_replace(from: &Path, to: &Path) -> Result<(), FileOpError> {
    match fs::hard_link(from, to) {
        Ok(()) => {
            if let Err(error) = fs::remove_file(from) {
                // Both names exist. Drop the new one so the file keeps one name.
                if let Err(cleanup) = fs::remove_file(to) {
                    tracing::warn!("cannot remove the extra link {}: {cleanup}", to.display());
                }
                return Err(io_error("remove", from, error));
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(FileNameError::Taken.into())
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied
            ) =>
        {
            // Hard links are unsupported or refused here. A file that appears between
            // the check and the rename would be replaced.
            if to.symlink_metadata().is_ok() {
                return Err(FileNameError::Taken.into());
            }
            fs::rename(from, to).map_err(|e| io_error("rename", from, e))
        }
        Err(error) => Err(io_error("link", to, error)),
    }
}

#[cfg(test)]
mod tests;
