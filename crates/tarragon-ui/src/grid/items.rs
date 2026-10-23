//! Grid items and their display order.

use std::path::PathBuf;

use tarragon_core::entity::FileInfo;
use tarragon_core::sorting::SortMode;

/// One tile in the gallery grid.
///
/// The tile carries an uploaded texture and nothing else. The UI crate never
/// decodes or uploads an image. The lower-case file name is cached, so the
/// search filter allocates nothing per frame. The file-type badge text comes
/// from the path at paint time, so a caller cannot set a wrong label.
pub struct GridItem {
    /// Absolute path of the file.
    pub path: PathBuf,
    /// Last-modified time in epoch seconds.
    pub mtime: i64,
    /// The uploaded thumbnail, or none while the image has no texture.
    pub texture: Option<egui::TextureHandle>,
    /// The lower-case file name, or an empty string when the name is not text.
    name_lower: String,
}

impl GridItem {
    /// Build one item and cache its lower-case file name.
    #[must_use]
    pub fn new(path: PathBuf, mtime: i64, texture: Option<egui::TextureHandle>) -> Self {
        let name_lower = lower_name(&path);
        Self {
            path,
            mtime,
            texture,
            name_lower,
        }
    }

    /// Recompute the cached lower-case name from the current path.
    pub(crate) fn refresh_name(&mut self) {
        self.name_lower = lower_name(&self.path);
    }

    /// The cached lower-case file name. It is empty when the name is not text.
    #[must_use]
    pub fn name_lower(&self) -> &str {
        &self.name_lower
    }
}

/// The lower-case file name of `path`, or an empty string when it is not text.
fn lower_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map_or_else(String::new, str::to_lowercase)
}

/// Return the item indices in the given sort order.
///
/// The sort compares a temporary [`FileInfo`] built from the path and the
/// mtime. Size and pixel dimensions stay zero, because the grid never uses
/// them.
#[must_use]
pub fn items_in_order(items: &[GridItem], mode: SortMode) -> Vec<usize> {
    let keys: Vec<FileInfo> = items.iter().map(file_info).collect();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&left, &right| mode.compare(&keys[left], &keys[right]));
    order
}

/// Build the sort key for one item.
fn file_info(item: &GridItem) -> FileInfo {
    FileInfo {
        path: item.path.clone(),
        size: 0,
        mtime: item.mtime,
        width: 0,
        height: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Four items. The two name-numbered files share one mtime.
    fn items() -> Vec<GridItem> {
        let item = |name: &str, mtime: i64| {
            GridItem::new(PathBuf::from("/images").join(name), mtime, None)
        };
        vec![
            item("b.png", 10),
            item("a.png", 20),
            item("a 10.png", 5),
            item("a 2.png", 5),
        ]
    }

    #[test]
    fn natural_order_follows_the_path() {
        let items = items();
        let order = items_in_order(&items, SortMode::Natural);
        let names: Vec<String> = order
            .iter()
            .map(|&index| items[index].path.display().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "/images/a 2.png",
                "/images/a 10.png",
                "/images/a.png",
                "/images/b.png"
            ]
        );
    }

    #[test]
    fn date_order_puts_the_newest_first() {
        let items = items();
        assert_eq!(items_in_order(&items, SortMode::Date), [1, 0, 3, 2]);
    }

    #[test]
    fn empty_items_give_an_empty_order() {
        assert_eq!(items_in_order(&[], SortMode::Natural), Vec::<usize>::new());
        assert_eq!(items_in_order(&[], SortMode::Date), Vec::<usize>::new());
    }

    #[test]
    fn a_new_item_caches_the_lower_case_name() {
        let item = GridItem::new(PathBuf::from("/images/Sketch 2.PSD"), 5, None);
        assert_eq!(item.name_lower(), "sketch 2.psd");
        assert_eq!(item.path, PathBuf::from("/images/Sketch 2.PSD"));
    }
}
