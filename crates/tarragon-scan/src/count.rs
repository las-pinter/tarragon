//! Count the supported images directly in one folder.
//!
//! The count matches what a scan of the folder keeps: supported names that
//! are files, with a link followed to its target. Nothing is read beyond the
//! listing, the entry types, and a stat for each link.

use std::fs::DirEntry;
use std::path::Path;

use crate::extensions::is_supported;

/// The number of supported images directly in `folder`.
///
/// A folder that cannot be listed, or a listing that fails part way, gives
/// none rather than a number, so a missing or unreadable folder never reads
/// as empty. A count above `u32::MAX` saturates.
#[must_use]
pub fn count_supported(folder: &Path) -> Option<u32> {
    let mut count: u32 = 0;
    for entry in std::fs::read_dir(folder).ok()? {
        if is_supported_file(&entry.ok()?) {
            count = count.saturating_add(1);
        }
    }
    Some(count)
}

/// Whether the entry is a supported name that is a file, following a link.
fn is_supported_file(entry: &DirEntry) -> bool {
    let path = entry.path();
    if !is_supported(&path) {
        return false;
    }
    match entry.file_type() {
        Ok(kind) if kind.is_symlink() => std::fs::metadata(&path).is_ok_and(|meta| meta.is_file()),
        Ok(kind) => kind.is_file(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests;
