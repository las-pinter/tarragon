//! The gallery scope and the cached view: which items show.

use tarragon_core::filter_state::FilterScope;

use super::Gallery;

/// The gallery scope: one folder, or every image in the library.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// Show the current folder only.
    #[default]
    Folder,
    /// Show every image.
    All,
}

impl Scope {
    /// The tab label for this scope.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Folder => "Folder",
            Self::All => "All Images",
        }
    }
}

impl Gallery {
    /// The active scope tab.
    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Set the scope tab and drop a selection that leaves the view.
    pub fn set_scope(&mut self, scope: Scope) {
        if scope == self.scope {
            return;
        }
        self.scope = scope;
        self.removed_at = None;
        self.filter_events.changed = true;
        self.refresh_view();
    }

    /// The filter scope that matches the scope tab.
    pub(super) fn filter_scope(&self) -> FilterScope {
        match self.scope {
            Scope::Folder => FilterScope::Folder,
            Scope::All => FilterScope::Global,
        }
    }

    /// Rebuild the cached display order and drop a hidden selection.
    ///
    /// The call runs only on a filter input change, so a quiet frame reads the
    /// cache instead of rebuilding it.
    pub(super) fn refresh_view(&mut self) {
        let visible: Vec<usize> = self
            .order
            .iter()
            .copied()
            .filter(|&index| self.in_scope(index) && self.matches(index))
            .collect();
        self.visible = visible;
        // A display position is stale once the view is rebuilt.
        self.scroll_to = None;
        self.selection.retain_from(&self.visible);
        self.sync_selection_revision();
    }

    /// Return whether the item belongs to the active scope.
    ///
    /// The item parent may equal the canonical folder or the folder as given,
    /// and the item path is compared as given. So the check does no file I/O,
    /// and a symlinked image stays in the folder that holds the link.
    fn in_scope(&self, index: usize) -> bool {
        if self.scope == Scope::All {
            return true;
        }
        let Some(folder) = self.folder.as_deref() else {
            return false;
        };
        self.items
            .get(index)
            .and_then(|item| item.path.parent())
            .is_some_and(|parent| parent == folder || self.raw_folder.as_deref() == Some(parent))
    }

    /// Return whether the item passes the applied search and the path set.
    fn matches(&self, index: usize) -> bool {
        let Some(item) = self.items.get(index) else {
            return false;
        };
        let text = &self.bar.state.text;
        (text.is_empty() || item.name_lower().contains(text))
            && self
                .filter_result
                .as_ref()
                .is_none_or(|set| set.contains(&item.path))
    }
}
