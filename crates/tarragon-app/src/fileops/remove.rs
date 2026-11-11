//! Move files to the trash or delete them for good.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{FileOpError, io_error};

/// Move each file to the desktop trash and give the failures.
///
/// An empty list means every file is in the trash or was already gone. A
/// failure for one path does not stop the others. The database does not change.
/// A missing file counts as done. The failure for a path is
/// [`FileOpError::Trash`] when the trash refuses it, or [`FileOpError::Io`] when
/// the disk cannot say whether it exists. A file on another mount may be copied
/// into the home trash.
#[must_use]
pub fn trash(paths: &[PathBuf]) -> Vec<(PathBuf, FileOpError)> {
    each(paths, |path| {
        ::trash::delete(path).map_err(|source| FileOpError::Trash {
            path: path.to_path_buf(),
            source,
        })
    })
}

/// Delete each file for good and give the failures.
///
/// An empty list means every file is gone. A failure for one path does not stop
/// the others. The database does not change. A missing file counts as done. The
/// failure for a path is [`FileOpError::Io`], such as for a folder.
#[must_use]
pub fn delete_permanently(paths: &[PathBuf]) -> Vec<(PathBuf, FileOpError)> {
    each(paths, |path| match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error("remove", path, e)),
    })
}

/// Run `act` on each path that exists, and collect the failures. A path that is
/// missing is skipped. Any other error from the stat call gives a `read`
/// [`FileOpError::Io`] failure.
fn each(
    paths: &[PathBuf],
    mut act: impl FnMut(&Path) -> Result<(), FileOpError>,
) -> Vec<(PathBuf, FileOpError)> {
    let mut failures = Vec::new();
    for path in paths {
        let result = match path.symlink_metadata() {
            Ok(_) => act(path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_error("read", path, e)),
        };
        if let Err(error) = result {
            failures.push((path.clone(), error));
        }
    }
    failures
}

#[cfg(test)]
mod tests;
