//! Copy a file beside itself or into another folder without overwriting.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tarragon_core::file_name::{MAX_FILE_NAME_BYTES, copy_name};

use super::{FileOpError, io_error};

/// A file the copy made, with the values the disk had after the write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    /// The new file.
    pub path: PathBuf,
    /// The bytes in the new file.
    pub len: u64,
    /// The modification time of the new file, at the precision of the disk.
    /// It is `UNIX_EPOCH` when the disk gives none.
    pub mtime: SystemTime,
    /// The device and inode of the new file, so a later file of the same name
    /// is not taken for it.
    #[cfg(unix)]
    pub id: (u64, u64),
}

impl Copied {
    /// Whether the file is still the one the copy made: a regular file, not a
    /// link, with the saved length, the exact saved modification time, and on
    /// Unix the saved device and inode. A missing file, a link, a file that was
    /// edited since, or a new file that took the name gives false.
    #[must_use]
    pub fn is_unchanged(&self) -> bool {
        fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.is_file()
                && metadata.len() == self.len
                && metadata.modified().unwrap_or(UNIX_EPOCH) == self.mtime
                && self.same_file_id(&metadata)
        })
    }

    /// Whether `metadata` has the saved device and inode.
    #[cfg(unix)]
    fn same_file_id(&self, metadata: &fs::Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        (metadata.dev(), metadata.ino()) == self.id
    }

    /// Without device and inode numbers, the other checks stand alone.
    #[cfg(not(unix))]
    fn same_file_id(&self, _metadata: &fs::Metadata) -> bool {
        true
    }
}

/// Copy `source` into `folder` and give the new file.
///
/// The new file keeps the source name when `folder` has no file with that
/// name. Otherwise it gets the first free copy name, such as `a copy.png`.
/// To copy beside the source, pass the source's own folder. The function never
/// overwrites a file, and it removes a partial file when the copy fails.
///
/// # Errors
///
/// Returns [`FileOpError::NotAFile`] when `source` is not a regular file,
/// [`FileOpError::InvalidName`] when its name is missing or not UTF-8,
/// [`FileOpError::NameTooLong`] when the copy name is over
/// [`MAX_FILE_NAME_BYTES`] bytes, and [`FileOpError::Io`] or
/// [`FileOpError::ShortCopy`] when the copy fails.
pub fn copy_into(source: &Path, folder: &Path) -> Result<Copied, FileOpError> {
    copy_with(source, folder, |_| {})
}

/// [`copy_into`] with a call that runs after the name pick and before the file
/// is created, so a test can make a file in that gap.
fn copy_with(
    source: &Path,
    folder: &Path,
    mut before_create: impl FnMut(&Path),
) -> Result<Copied, FileOpError> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| FileOpError::InvalidName(source.to_path_buf()))?;
    // Opening a FIFO for reading blocks, so a non-file must be refused before the open.
    let is_file = fs::metadata(source)
        .map_err(|e| io_error("read", source, e))?
        .is_file();
    if !is_file {
        return Err(FileOpError::NotAFile(source.to_path_buf()));
    }
    let mut input = File::open(source).map_err(|e| io_error("open", source, e))?;
    // The handle gives the kind and size, so they match the bytes that are read.
    let metadata = input.metadata().map_err(|e| io_error("read", source, e))?;
    if !metadata.is_file() {
        return Err(FileOpError::NotAFile(source.to_path_buf()));
    }
    loop {
        let target = folder.join(pick_name(name, folder)?);
        before_create(&target);
        // `create_new` fails when the name appeared after the pick. The next
        // pick then sees that file and moves on.
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(output) => return write_copy(&mut input, output, &target, metadata.len()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io_error("create", &target, e)),
        }
    }
}

/// Give the source name when the folder has no such file, else a copy name.
fn pick_name(name: &str, folder: &Path) -> Result<String, FileOpError> {
    let used = |candidate: &str| folder.join(candidate).symlink_metadata().is_ok();
    let picked = if used(name) {
        copy_name(name, used)
    } else {
        name.to_owned()
    };
    if picked.len() > MAX_FILE_NAME_BYTES {
        return Err(FileOpError::NameTooLong {
            len: picked.len(),
            max: MAX_FILE_NAME_BYTES,
        });
    }
    Ok(picked)
}

/// Fill the new file from the source, then read its size and time. A failure
/// removes the file.
fn write_copy(
    input: &mut File,
    mut output: File,
    target: &Path,
    expected: u64,
) -> Result<Copied, FileOpError> {
    let result = io::copy(input, &mut output)
        .map_err(|e| io_error("write", target, e))
        .and_then(|copied| {
            if copied == expected {
                Ok(())
            } else {
                Err(FileOpError::ShortCopy {
                    path: target.to_path_buf(),
                    copied,
                    expected,
                })
            }
        })
        .and_then(|()| output.sync_all().map_err(|e| io_error("sync", target, e)))
        .and_then(|()| fs::metadata(target).map_err(|e| io_error("read", target, e)));
    drop(output);
    match result {
        Ok(metadata) => Ok(Copied {
            path: target.to_path_buf(),
            len: metadata.len(),
            mtime: metadata.modified().unwrap_or(UNIX_EPOCH),
            #[cfg(unix)]
            id: {
                use std::os::unix::fs::MetadataExt;
                (metadata.dev(), metadata.ino())
            },
        }),
        Err(error) => {
            // The write error is the one to report, so a failed cleanup is ignored.
            let _ = fs::remove_file(target);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
