//! Filter state for colors, tags, and folders.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::color::ColorBucket;

/// Which gallery scope the filters run in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterScope {
    /// The all-images tab. Folder chips apply.
    Global,
    /// A single folder tab. Folder chips do not apply.
    Folder,
}

/// The active filters for the gallery.
///
/// The filter semantics are tags AND, colors AND, folders OR, filename
/// substring. This type only holds the state and counts it. The query runs
/// in `tarragon-db`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterState {
    /// Filename substring, used as-is. Only the empty string is inactive, so
    /// a whitespace-only string counts as one filter.
    pub text: String,
    /// Dominant-color buckets. The query matches all of them.
    pub colors: BTreeSet<ColorBucket>,
    /// Require the `transparent` tag, like one more color.
    pub transparent: bool,
    /// User tag names. The query matches all of them.
    pub tags: BTreeSet<String>,
    /// Folder paths. The query matches any of them. Global scope only.
    pub folders: BTreeSet<PathBuf>,
}

impl FilterState {
    /// Count the active filters.
    ///
    /// The text counts once. Each color, the transparent flag, each tag, and
    /// each folder counts once. Folder chips count in global scope only. This
    /// is the single filter-count source for the info bar. The text is used as-is, so a whitespace-only
    /// text still counts once.
    #[must_use]
    pub fn active_count(&self, scope: FilterScope) -> usize {
        let text = usize::from(!self.text.is_empty());
        let folders = match scope {
            FilterScope::Global => self.folders.len(),
            FilterScope::Folder => 0,
        };
        text + self.colors.len() + usize::from(self.transparent) + self.tags.len() + folders
    }

    /// Return true when no filter is active in the scope.
    #[must_use]
    pub fn is_empty(&self, scope: FilterScope) -> bool {
        self.active_count(scope) == 0
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use crate::color::ColorBucket;
    use crate::filter_state::{FilterScope, FilterState};

    fn tagged_state() -> FilterState {
        FilterState {
            text: "sky".to_owned(),
            colors: BTreeSet::from([ColorBucket::Blue]),
            transparent: false,
            tags: BTreeSet::from(["night".to_owned()]),
            folders: BTreeSet::from([PathBuf::from("/refs/one"), PathBuf::from("/refs/two")]),
        }
    }

    #[test]
    fn empty_state_has_no_active_filter() {
        let state = FilterState::default();
        assert_eq!(state.active_count(FilterScope::Global), 0);
        assert!(state.is_empty(FilterScope::Global));
    }

    #[test]
    fn whitespace_only_text_counts_as_one_filter() {
        let state = FilterState {
            text: "   ".to_owned(),
            ..FilterState::default()
        };
        assert_eq!(state.active_count(FilterScope::Global), 1);
        assert!(!state.is_empty(FilterScope::Global));
    }

    #[test]
    fn every_whitespace_char_still_counts_as_text() {
        let state = FilterState {
            text: "\t\n".to_owned(),
            ..FilterState::default()
        };
        assert_eq!(state.active_count(FilterScope::Folder), 1);
    }

    #[test]
    fn every_active_filter_counts_once() {
        let state = tagged_state();
        // text 1 + color 1 + tag 1 + folders 2 = 5.
        assert_eq!(state.active_count(FilterScope::Global), 5);
    }

    #[test]
    fn transparent_counts_as_one_filter() {
        let state = FilterState {
            transparent: true,
            ..tagged_state()
        };
        assert_eq!(state.active_count(FilterScope::Folder), 4);
        let alone = FilterState {
            transparent: true,
            ..FilterState::default()
        };
        assert!(!alone.is_empty(FilterScope::Folder));
    }

    #[test]
    fn folder_chips_count_in_global_scope_only() {
        let state = tagged_state();
        // text 1 + color 1 + tag 1 = 3. The two folders do not count.
        assert_eq!(state.active_count(FilterScope::Folder), 3);
    }

    #[test]
    fn folder_only_state_is_empty_in_folder_scope() {
        let state = FilterState {
            folders: BTreeSet::from([PathBuf::from("/refs/one")]),
            ..FilterState::default()
        };
        assert_eq!(state.active_count(FilterScope::Global), 1);
        assert!(state.is_empty(FilterScope::Folder));
    }
}
