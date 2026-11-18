//! Where the file bytes come from: a file read by range, or memory.
//!
//! A large PSB can be many gigabytes. The parser never reads a whole file.
//! It asks the source for a byte range, and the source answers from the file
//! with a positional read. A positional read has no shared cursor, so many
//! threads can read one document at the same time without a lock.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::PsdError;

/// The largest single read the source allows into a new buffer.
///
/// A record, a tagged block, or the resources section is read whole. A declared
/// length above this cap is a typed error, so a corrupt length cannot ask for
/// gigabytes of memory.
pub(crate) const MAX_READ: usize = 256 * 1024 * 1024;

/// The largest extra data one layer record may declare, in bytes.
///
/// The extra data holds the mask, the blending ranges, the name, and the tagged
/// blocks of one layer. A record is read whole, and a document can have
/// thousands of them, so this cap is far below [`MAX_READ`].
pub(crate) const MAX_RECORD_EXTRA: usize = 64 * 1024 * 1024;

/// A readable PSD or PSB byte source.
pub(crate) struct Source {
    inner: Inner,
    /// The total length in bytes.
    len: u64,
    #[cfg(test)]
    stats: stats::Counters,
}

enum Inner {
    /// A file on disk, read by range. The path is kept for error messages.
    File { file: File, path: PathBuf },
    /// Bytes already in memory, for `from_bytes` and tests.
    Memory(Arc<[u8]>),
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.inner {
            Inner::File { .. } => "file",
            Inner::Memory(_) => "memory",
        };
        f.debug_struct("Source")
            .field("kind", &kind)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl Source {
    /// Open a file for range reads. The file is not read.
    pub(crate) fn open(path: &Path) -> Result<Self, PsdError> {
        let io = |source| PsdError::Io {
            path: path.to_path_buf(),
            source,
        };
        let file = File::open(path).map_err(io)?;
        let len = file.metadata().map_err(io)?.len();
        Ok(Self::new(
            Inner::File {
                file,
                path: path.to_path_buf(),
            },
            len,
        ))
    }

    /// Wrap owned bytes.
    pub(crate) fn memory(data: Vec<u8>) -> Self {
        let len = data.len() as u64;
        Self::new(Inner::Memory(Arc::from(data)), len)
    }

    fn new(inner: Inner, len: u64) -> Self {
        Self {
            inner,
            len,
            #[cfg(test)]
            stats: stats::Counters::default(),
        }
    }

    /// The total length in bytes.
    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    /// Fill `buf` from `offset`.
    ///
    /// A range past the end of the source is [`PsdError::Truncated`], for a
    /// file and for memory alike.
    pub(crate) fn read_into(&self, offset: u64, buf: &mut [u8]) -> Result<(), PsdError> {
        let wanted = buf.len() as u64;
        let remaining = self.len.saturating_sub(offset);
        if offset > self.len || wanted > remaining {
            return Err(PsdError::Truncated {
                offset,
                wanted: buf.len(),
                remaining: usize::try_from(remaining).unwrap_or(usize::MAX),
            });
        }
        #[cfg(test)]
        self.stats.record(wanted);
        match &self.inner {
            Inner::Memory(data) => {
                // The range check above keeps both bounds inside `data`.
                let start = usize::try_from(offset).unwrap_or(usize::MAX);
                buf.copy_from_slice(&data[start..start + buf.len()]);
                Ok(())
            }
            Inner::File { file, path } => read_exact_at(file, buf, offset).map_err(|source| {
                // A file that shrank after the open ends early.
                if source.kind() == std::io::ErrorKind::UnexpectedEof {
                    PsdError::Truncated {
                        offset,
                        wanted: buf.len(),
                        remaining: 0,
                    }
                } else {
                    PsdError::Io {
                        path: path.clone(),
                        source,
                    }
                }
            }),
        }
    }

    /// Read `length` bytes from `offset` into a new buffer.
    ///
    /// # Errors
    ///
    /// A length above [`MAX_READ`] is [`PsdError::LengthTooLarge`]. The check
    /// runs before the allocation.
    pub(crate) fn read_vec(&self, offset: u64, length: usize) -> Result<Vec<u8>, PsdError> {
        if length > MAX_READ {
            return Err(PsdError::LengthTooLarge {
                length: length as u64,
            });
        }
        let mut buf = vec![0u8; length];
        self.read_into(offset, &mut buf)?;
        Ok(buf)
    }

    /// The read counters, for tests that prove which ranges were read.
    #[cfg(test)]
    pub(crate) fn stats(&self) -> stats::Snapshot {
        self.stats.snapshot()
    }
}

#[cfg(not(any(unix, windows)))]
compile_error!("tarragon-psd reads files by position and supports only Unix and Windows targets");

/// A positional read that leaves no shared cursor.
#[cfg(unix)]
fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
}

/// A positional read that leaves no shared cursor the parser relies on.
///
/// Windows `seek_read` moves the handle cursor, but no code here reads it, so
/// concurrent reads stay correct. It may return fewer bytes, so loop.
#[cfg(windows)]
fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    use std::io::ErrorKind;
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_read(buf, offset) {
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(count) => {
                buf = &mut buf[count..];
                offset += count as u64;
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Read counters, compiled for unit tests only.
#[cfg(test)]
pub(crate) mod stats {
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Counters shared by every thread that reads one source.
    #[derive(Default)]
    pub(crate) struct Counters {
        reads: AtomicU64,
        bytes: AtomicU64,
        largest: AtomicU64,
    }

    /// A copy of the counters at one moment.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Snapshot {
        /// The number of reads.
        pub(crate) reads: u64,
        /// The sum of all read lengths.
        pub(crate) bytes: u64,
        /// The longest single read.
        pub(crate) largest: u64,
    }

    impl Counters {
        pub(super) fn record(&self, bytes: u64) {
            self.reads.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(bytes, Ordering::Relaxed);
            self.largest.fetch_max(bytes, Ordering::Relaxed);
        }

        pub(super) fn snapshot(&self) -> Snapshot {
            Snapshot {
                reads: self.reads.load(Ordering::Relaxed),
                bytes: self.bytes.load(Ordering::Relaxed),
                largest: self.largest.load(Ordering::Relaxed),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_and_file_agree_on_ranges_and_truncation() {
        let data: Vec<u8> = (0..=255u8).collect();
        let dir = std::env::temp_dir().join(format!("tarragon-source-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("bytes.bin");
        std::fs::write(&path, &data).expect("write");
        let file = Source::open(&path).expect("opens");
        let memory = Source::memory(data);
        for source in [&file, &memory] {
            assert_eq!(source.len(), 256);
            assert_eq!(
                source.read_vec(250, 6).expect("tail"),
                [250, 251, 252, 253, 254, 255]
            );
            assert!(matches!(
                source.read_vec(250, 7),
                Err(PsdError::Truncated {
                    offset: 250,
                    wanted: 7,
                    remaining: 6
                })
            ));
            assert!(matches!(
                source.read_vec(300, 1),
                Err(PsdError::Truncated { remaining: 0, .. })
            ));
            assert!(matches!(
                source.read_vec(0, MAX_READ + 1),
                Err(PsdError::LengthTooLarge { .. })
            ));
        }
        assert_eq!(file.stats().bytes, 6, "a refused range is not counted");
        std::fs::remove_dir_all(&dir).ok();
    }
}
