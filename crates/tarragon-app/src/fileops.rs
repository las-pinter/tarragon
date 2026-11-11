//! Disk operations on image files: copy a file beside itself or into a folder.

mod copy;

pub use copy::{Copied, copy_into};

use std::io;
use std::path::PathBuf;

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
