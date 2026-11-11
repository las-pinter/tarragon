//! Disk operations on image files: copy a file and rename a file.

mod copy;
mod rename;

pub use copy::{Copied, copy_into};
pub use rename::rename;

use std::io;
use std::path::{Path, PathBuf};

use tarragon_core::file_name::FileNameError;
use tarragon_db::DbError;

/// Why a file operation failed.
#[derive(Debug, thiserror::Error)]
pub enum FileOpError {
    /// The source is not a regular file.
    #[error("{} is not a regular file", .0.display())]
    NotAFile(PathBuf),
    /// The source has no name, or its name is not valid UTF-8.
    #[error("{} has no usable file name", .0.display())]
    InvalidName(PathBuf),
    /// The copy name has more bytes than a file name may have.
    #[error("the copy name is {len} bytes, more than the {max} allowed")]
    NameTooLong {
        /// The bytes in the copy name.
        len: usize,
        /// The most bytes a file name may have.
        max: usize,
    },
    /// The copy has fewer bytes than the source.
    #[error("copied {copied} of {expected} bytes to {}", path.display())]
    ShortCopy {
        /// The file that was being written.
        path: PathBuf,
        /// The bytes written.
        copied: u64,
        /// The bytes in the source.
        expected: u64,
    },
    /// The new file stem is refused, or a file already has the new name.
    #[error(transparent)]
    Stem(#[from] FileNameError),
    /// A database call failed.
    #[error("database call failed: {0}")]
    Db(#[from] DbError),
    /// The database call failed and the file could not move back.
    #[error(
        "the database call failed ({db}), and the file is still at {}: move it back to {}",
        from.display(),
        to.display()
    )]
    RollbackFailed {
        /// Where the file is now.
        from: PathBuf,
        /// The old path the file should have.
        to: PathBuf,
        /// The database error.
        db: Box<DbError>,
        /// Why the move back failed.
        source: Box<FileOpError>,
    },
    /// A disk call failed.
    #[error("cannot {action} {}: {source}", path.display())]
    Io {
        /// What the call tried to do.
        action: &'static str,
        /// The path the call used.
        path: PathBuf,
        /// The error from the operating system.
        source: io::Error,
    },
}

/// Wrap a disk error with what the call did and the path it used.
fn io_error(action: &'static str, path: &Path, source: io::Error) -> FileOpError {
    FileOpError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}
