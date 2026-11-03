//! What the Preview shows for the selection, read from the database.

use std::path::{Path, PathBuf};

use tarragon_db::Database;
use tarragon_db::repository::Thumbnail;
use tarragon_ui::preview::budget::scaled_size;
use tarragon_ui::preview::{Meta, UNKNOWN};

/// The longest side of the preview tier. A picture is never drawn larger.
const PREVIEW_SIDE: u32 = 1024;

/// One picture the loader can decode.
pub(crate) struct SourceItem {
    /// The path of the selected file.
    pub(crate) key: PathBuf,
    /// The cache tier file to decode.
    pub(crate) tier: PathBuf,
    /// The most width and height the decoded picture needs.
    pub(crate) size: (u32, u32),
}

/// What one selection needs on screen.
pub(crate) struct Source {
    /// The details of the file. `Some` only for a selection of one.
    pub(crate) meta: Option<Meta>,
    /// The pictures to decode, in selection order.
    pub(crate) items: Vec<SourceItem>,
    /// How many files are selected, also the ones past the cap.
    pub(crate) total: usize,
}

/// Read the row of each of the first `cap` selected paths on the calling thread.
///
/// A path whose row is missing, has no tier file on disk, or fails to read
/// gives no item. A failed read is logged.
pub(crate) fn read(db: &Database, selected: &[PathBuf], cap: usize) -> Source {
    let mut items = Vec::new();
    let mut first_row = None;
    for (index, path) in selected.iter().take(cap).enumerate() {
        let row = lookup(db, path);
        if let Some(item) = row.as_ref().and_then(|row| item_of(path, row)) {
            items.push(item);
        }
        if index == 0 {
            first_row = row;
        }
    }
    let meta = match selected {
        [path] => Some(meta_of(path, first_row.as_ref())),
        _ => None,
    };
    Source {
        meta,
        items,
        total: selected.len(),
    }
}

/// The row of `path`. A failed read is logged and gives `None`.
fn lookup(db: &Database, path: &Path) -> Option<Thumbnail> {
    match db.thumbnail(&path.to_string_lossy()) {
        Ok(row) => row,
        Err(error) => {
            tracing::warn!("cannot read the row of {}: {error}", path.display());
            None
        }
    }
}

/// The item for `path`: the 1024 tier file if it exists, else the 256 one.
fn item_of(path: &Path, row: &Thumbnail) -> Option<SourceItem> {
    let tier = [&row.preview_cache_path, &row.thumbnail_cache_path]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .find(|tier| tier.is_file())?;
    let size = if row.width > 0 && row.height > 0 {
        scaled_size((row.width, row.height), PREVIEW_SIDE)
    } else {
        // A zero width means the scan did not read the size.
        image::image_dimensions(&tier).ok()?
    };
    Some(SourceItem {
        key: path.to_path_buf(),
        tier,
        size,
    })
}

/// The details of `path`. The row wins; the file fills a size it lacks.
fn meta_of(path: &Path, row: Option<&Thumbnail>) -> Meta {
    let size = row
        .and_then(|row| u64::try_from(row.size).ok())
        .or_else(|| std::fs::metadata(path).ok().map(|meta| meta.len()));
    let dimensions = row
        .filter(|row| row.width > 0 && row.height > 0)
        .map(|row| (row.width, row.height));
    Meta {
        name: path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        dimensions,
        size,
        format: path.extension().map_or_else(
            || UNKNOWN.to_owned(),
            |ext| ext.to_string_lossy().to_uppercase(),
        ),
    }
}

#[cfg(test)]
mod tests;
