//! Library sidebar: the folder tree and the favorites list.

use std::path::{Path, PathBuf};

pub mod lister;
pub mod tree;

#[cfg(test)]
mod tests;

/// One favorite folder as the sidebar shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteRow {
    /// The folder path, `/`-separated as the database stores it.
    pub path: PathBuf,
    /// The stored label, or none when the user set no name.
    pub label: Option<String>,
    /// The position in the favorites list.
    pub sort_order: i64,
}

/// The sidebar data: favorites, tree roots, and the selected folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarState {
    /// The favorites in the order the list shows them.
    favorites: Vec<FavoriteRow>,
    /// The tree roots, so a label can show a path below its root.
    roots: Vec<PathBuf>,
    /// The highlighted folder, or none.
    selected: Option<PathBuf>,
}

impl SidebarState {
    /// The favorites in list order.
    #[must_use]
    pub fn favorites(&self) -> &[FavoriteRow] {
        &self.favorites
    }

    /// Replace the favorites list.
    pub fn set_favorites(&mut self, favorites: Vec<FavoriteRow>) {
        self.favorites = favorites;
    }

    /// The tree roots.
    #[must_use]
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Replace the tree roots.
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
    }

    /// The selected folder, or none.
    #[must_use]
    pub fn selected(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    /// Set the selected folder.
    pub fn set_selected(&mut self, selected: Option<PathBuf>) {
        self.selected = selected;
    }
}

/// The text a favorite row shows.
///
/// A stored label that is not blank wins. Otherwise the row shows the path
/// below the longest root that contains it, so nested folders stay short.
/// Callers pass `/`-separated paths, and the output joins parts with `/`.
#[must_use]
pub fn display_label(path: &Path, label: Option<&str>, roots: &[PathBuf]) -> String {
    if let Some(stored) = label.filter(|stored| !stored.trim().is_empty()) {
        return stored.to_owned();
    }
    let full = || path.to_string_lossy().into_owned();
    let nearest = roots
        .iter()
        .filter(|root| !root.as_os_str().is_empty() && path.starts_with(root))
        .max_by_key(|root| root.components().count());
    let Some(root) = nearest else {
        return full();
    };
    if path == root {
        // A root without a last component, such as `/`, shows the full path.
        let Some(name) = root.file_name() else {
            return full();
        };
        return name.to_string_lossy().into_owned();
    }
    let Ok(below) = path.strip_prefix(root) else {
        return full();
    };
    let parts: Vec<_> = below.iter().map(|part| part.to_string_lossy()).collect();
    parts.join("/")
}
