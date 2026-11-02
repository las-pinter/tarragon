//! Filter bar state: the filters plus the order their chips were added.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::color::ColorBucket;
use crate::filter_state::FilterState;

/// One entry of the color menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// A dominant-color bucket. It maps to `FilterState::colors`.
    Bucket(ColorBucket),
    /// The transparent flag. It maps to `FilterState::transparent`.
    Transparent,
}

/// A short display name for a folder path: its last two normal parts joined
/// with `/`. A path with two parts or fewer, or none (such as `/`), is returned
/// whole. The root does not count as a part.
#[must_use]
pub fn short_folder_name(path: &str) -> String {
    let parts: Vec<_> = Path::new(path)
        .components()
        .filter(|c| !matches!(c, Component::RootDir | Component::Prefix(_)))
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    match parts.as_slice() {
        [.., a, b] if parts.len() > 2 => format!("{a}/{b}"),
        _ => path.to_owned(),
    }
}

/// The filter state with the chip order for tags and folders.
///
/// Change tags and folders only through the toggle, rename, and prune methods.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterBar {
    /// The active filters.
    pub state: FilterState,
    tag_order: Vec<String>,
    folder_order: Vec<PathBuf>,
}

/// Flip `item` in `set` and keep `order` in step with it.
fn toggle_ordered<T: Ord + Clone>(set: &mut BTreeSet<T>, order: &mut Vec<T>, item: T) {
    if set.remove(&item) {
        order.retain(|x| *x != item);
    } else {
        set.insert(item.clone());
        order.push(item);
    }
    debug_assert_eq!(set.len(), order.len());
}

impl FilterBar {
    /// Active tags in activation order.
    #[must_use]
    pub fn tag_order(&self) -> &[String] {
        &self.tag_order
    }

    /// Active folders in activation order.
    #[must_use]
    pub fn folder_order(&self) -> &[PathBuf] {
        &self.folder_order
    }

    /// Flip a color bucket or the transparent flag.
    pub fn toggle_color(&mut self, choice: ColorChoice) {
        match choice {
            ColorChoice::Bucket(bucket) => {
                if !self.state.colors.remove(&bucket) {
                    self.state.colors.insert(bucket);
                }
            }
            ColorChoice::Transparent => self.state.transparent = !self.state.transparent,
        }
    }

    /// Flip a tag. A new tag goes last in `tag_order`.
    pub fn toggle_tag(&mut self, tag: &str) {
        toggle_ordered(&mut self.state.tags, &mut self.tag_order, tag.to_owned());
    }

    /// Flip a folder. A new folder goes last in `folder_order`.
    pub fn toggle_folder(&mut self, folder: &Path) {
        toggle_ordered(
            &mut self.state.folders,
            &mut self.folder_order,
            folder.to_path_buf(),
        );
    }

    /// Drop every active folder that is not in `keep`. Return true if any went.
    pub fn prune_folders(&mut self, keep: &[String]) -> bool {
        let kept = |path: &Path| keep.iter().any(|k| Path::new(k) == path);
        let before = self.folder_order.len();
        self.state.folders.retain(|f| kept(f));
        self.folder_order.retain(|f| kept(f));
        debug_assert_eq!(self.state.folders.len(), self.folder_order.len());
        self.folder_order.len() != before
    }

    /// Rename an active tag and keep its place in `tag_order`. Return false if
    /// `old` is not active, or if `old` and `new` are the same name.
    ///
    /// If `new` is already active, only `old` goes, so no chip repeats.
    pub fn rename_tag(&mut self, old: &str, new: &str) -> bool {
        if old == new || !self.state.tags.remove(old) {
            return false;
        }
        if self.state.tags.contains(new) {
            self.tag_order.retain(|t| t != old);
        } else {
            self.state.tags.insert(new.to_owned());
            if let Some(slot) = self.tag_order.iter_mut().find(|t| *t == old) {
                new.clone_into(slot);
            }
        }
        debug_assert_eq!(self.state.tags.len(), self.tag_order.len());
        true
    }

    /// Drop every active tag that is not in `keep`. Return true if any went.
    pub fn prune_tags(&mut self, keep: &[String]) -> bool {
        let before = self.tag_order.len();
        self.state.tags.retain(|t| keep.contains(t));
        self.tag_order.retain(|t| keep.contains(t));
        debug_assert_eq!(self.state.tags.len(), self.tag_order.len());
        self.tag_order.len() != before
    }

    /// Clear the color buckets and the transparent flag. Tags and folders stay.
    pub fn clear_colors(&mut self) {
        self.state.colors.clear();
        self.state.transparent = false;
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ColorChoice, FilterBar};
    use crate::color::ColorBucket;
    use crate::filter_state::FilterScope;

    #[test]
    fn short_names_keep_the_last_two_parts() {
        let short = super::short_folder_name;
        assert_eq!(short("/home/u/photos/vacation"), "photos/vacation");
        assert_eq!(short("/a/b"), "/a/b");
        assert_eq!(short("/a/b/c/"), "b/c");
        assert_eq!(short("/a/b/"), "/a/b/");
        assert_eq!(
            (short("/"), short("photos"), short("")),
            ("/".into(), "photos".into(), String::new())
        );
    }

    type Step = (fn(&mut FilterBar), usize, usize);

    #[test]
    fn toggling_twice_returns_to_empty() {
        let mut bar = FilterBar::default();
        bar.toggle_color(ColorChoice::Bucket(ColorBucket::Blue));
        bar.toggle_color(ColorChoice::Transparent);
        bar.toggle_tag("a");
        bar.toggle_folder(Path::new("/x"));
        bar.toggle_color(ColorChoice::Bucket(ColorBucket::Blue));
        bar.toggle_color(ColorChoice::Transparent);
        bar.toggle_tag("a");
        bar.toggle_folder(Path::new("/x"));
        assert_eq!(bar, FilterBar::default());
    }

    #[test]
    fn transparent_sets_flag_not_colors() {
        let mut bar = FilterBar::default();
        bar.toggle_color(ColorChoice::Transparent);
        assert!(bar.state.transparent);
        assert!(bar.state.colors.is_empty());
    }

    #[test]
    fn order_follows_activation_and_removal_keeps_rest() {
        let mut bar = FilterBar::default();
        for t in ["z", "a", "m"] {
            bar.toggle_tag(t);
        }
        for f in ["/z", "/a", "/m"] {
            bar.toggle_folder(Path::new(f));
        }
        bar.toggle_tag("a");
        bar.toggle_folder(Path::new("/a"));
        assert_eq!(bar.tag_order(), ["z", "m"]);
        assert_eq!(bar.folder_order(), [Path::new("/z"), Path::new("/m")]);
    }

    #[test]
    fn prune_folders_drops_stale_chips_in_both_lists() {
        let mut bar = FilterBar::default();
        for f in ["/a", "/b", "/c"] {
            bar.toggle_folder(Path::new(f));
        }
        assert!(!bar.prune_folders(&["/a".into(), "/b".into(), "/c".into()]));
        assert!(bar.prune_folders(&["/a".into(), "/c".into()]));
        assert_eq!(bar.folder_order(), [Path::new("/a"), Path::new("/c")]);
        assert_eq!(bar.state.folders.len(), 2);
    }

    fn tag_bar(tags: &[&str]) -> FilterBar {
        let mut bar = FilterBar::default();
        for t in tags {
            bar.toggle_tag(t);
        }
        bar
    }

    #[test]
    fn rename_tag_keeps_the_chip_place() {
        let mut bar = tag_bar(&["a", "b", "c"]);
        assert!(bar.rename_tag("b", "z"));
        assert_eq!(bar.tag_order(), ["a", "z", "c"]);
        assert!(bar.state.tags.contains("z") && !bar.state.tags.contains("b"));
        assert_eq!(bar.state.tags.len(), 3);
    }

    #[test]
    fn rename_inactive_tag_changes_nothing() {
        let mut bar = tag_bar(&["a", "b"]);
        let before = bar.clone();
        assert!(!bar.rename_tag("x", "y"));
        assert_eq!(bar, before);
    }

    #[test]
    fn rename_onto_active_tag_drops_only_the_old_chip() {
        let mut bar = tag_bar(&["a", "b", "c"]);
        assert!(bar.rename_tag("a", "c"));
        assert_eq!(bar.tag_order(), ["b", "c"]);
        assert_eq!(bar.state.tags.len(), 2);
        assert!(!bar.state.tags.contains("a"));
    }

    #[test]
    fn rename_to_the_same_name_keeps_the_chip() {
        let mut bar = tag_bar(&["a", "b"]);
        let before = bar.clone();
        assert!(!bar.rename_tag("a", "a"));
        assert_eq!(bar, before);
    }

    #[test]
    fn prune_tags_drops_stale_chips_in_both_lists() {
        let mut bar = tag_bar(&["a", "b", "c"]);
        assert!(bar.prune_tags(&["a".into()]));
        assert_eq!(bar.tag_order(), ["a"]);
        assert_eq!(bar.state.tags.len(), 1);
        assert!(!bar.prune_tags(&["a".into()]));
    }

    #[test]
    fn active_count_tracks_chips() {
        let mut bar = FilterBar::default();
        // (step, Global count, Folder count) after the step.
        let steps: [Step; 5] = [
            (
                |b| b.toggle_color(ColorChoice::Bucket(ColorBucket::Red)),
                1,
                1,
            ),
            (|b| b.toggle_color(ColorChoice::Transparent), 2, 2),
            (|b| b.toggle_tag("a"), 3, 3),
            (|b| b.toggle_folder(Path::new("/x")), 4, 3),
            (|b| b.toggle_folder(Path::new("/x")), 3, 3),
        ];
        for (step, global, folder) in steps {
            step(&mut bar);
            assert_eq!(bar.state.active_count(FilterScope::Global), global);
            assert_eq!(bar.state.active_count(FilterScope::Folder), folder);
        }
    }

    #[test]
    fn re_added_tag_goes_last() {
        let mut bar = FilterBar::default();
        bar.toggle_tag("a");
        bar.toggle_tag("b");
        bar.toggle_tag("a");
        bar.toggle_tag("a");
        assert_eq!(bar.tag_order(), ["b", "a"]);
    }

    #[test]
    fn clear_colors_keeps_tags_and_folders() {
        let mut bar = FilterBar::default();
        bar.toggle_color(ColorChoice::Bucket(ColorBucket::Red));
        bar.toggle_color(ColorChoice::Transparent);
        bar.toggle_tag("a");
        bar.toggle_folder(Path::new("/x"));
        bar.clear_colors();
        assert!(bar.state.colors.is_empty() && !bar.state.transparent);
        assert_eq!(bar.tag_order(), ["a"]);
        assert_eq!(bar.folder_order(), [Path::new("/x")]);
    }
}
