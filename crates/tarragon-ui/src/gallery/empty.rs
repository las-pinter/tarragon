//! The rule that picks which empty card the gallery shows.

use super::{Gallery, Scope};

/// Which empty card the gallery shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyKind {
    /// No folder is open and there is nothing to list.
    NoFolder,
    /// The open folder holds no image.
    EmptyFolder,
    /// A filter hides every image.
    NoResult,
}

impl Gallery {
    /// Set whether a scan or a query is still running.
    ///
    /// While it runs, no empty card shows.
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// The empty card to show, or none when the grid has cells or work runs.
    #[must_use]
    pub fn empty_kind(&self) -> Option<EmptyKind> {
        if !self.visible.is_empty() || self.loading {
            return None;
        }
        if self.filter_active() {
            return Some(EmptyKind::NoResult);
        }
        let no_source = match self.scope {
            Scope::Folder => self.folder.is_none(),
            Scope::All => self.items.is_empty(),
        };
        Some(if no_source {
            EmptyKind::NoFolder
        } else {
            EmptyKind::EmptyFolder
        })
    }

    /// Return whether the search, a chip, or the database result filters.
    fn filter_active(&self) -> bool {
        !self.bar.state.is_empty(self.filter_scope()) || self.filter_result.is_some()
    }
}
