//! Library sidebar: the folder tree and the favorites list.

use std::path::{Component, Path, PathBuf};

pub mod favorite;
pub mod lister;
pub mod paint;
pub mod panel;
pub mod reorder;
pub mod row;
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
    /// Whether the folder is gone from disk; the caller refreshes it, so drawing never stats.
    pub missing: bool,
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

/// The text each favorite row shows, in list order.
///
/// A stored label that is not blank wins. Otherwise a row shows its folder
/// name. When several favorites without a label share a folder name, each of
/// them shows its path below the folder their parents all share, such as
/// `clientA/2026-10` and `clientB/2026-10`. A folder with no name, such as
/// `/`, shows its full path, and so do namesakes that share no folder, such as
/// two drives. Callers pass `/`-separated paths, and the output joins parts
/// with `/`.
#[must_use]
pub fn favorite_labels(favorites: &[FavoriteRow]) -> Vec<String> {
    let stored = |fav: &FavoriteRow| {
        fav.label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .map(ToOwned::to_owned)
    };
    let name = |path: &Path| {
        path.file_name().map_or_else(
            || path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    };
    favorites
        .iter()
        .map(|fav| {
            if let Some(label) = stored(fav) {
                return label;
            }
            let own = name(&fav.path);
            let twins: Vec<&Path> = favorites
                .iter()
                .filter(|other| stored(other).is_none() && name(&other.path) == own)
                .map(|other| other.path.as_path())
                .collect();
            if twins.len() < 2 || fav.path.file_name().is_none() {
                return own;
            }
            let below: Vec<_> = fav.path.components().skip(shared_parent(&twins)).collect();
            if below
                .iter()
                .any(|part| !matches!(part, Component::Normal(_)))
            {
                // Paths on two drives, or a relative and an absolute one,
                // share no folder, so the row shows the full path.
                return fav.path.to_string_lossy().into_owned();
            }
            let parts: Vec<_> = below
                .iter()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect();
            parts.join("/")
        })
        .collect()
}

/// How many leading components the parent folders of `paths` all share.
fn shared_parent(paths: &[&Path]) -> usize {
    let parents: Vec<Vec<_>> = paths
        .iter()
        .map(|path| {
            path.parent()
                .map_or_else(Vec::new, |parent| parent.components().collect())
        })
        .collect();
    let shortest = parents.iter().map(Vec::len).min().unwrap_or(0);
    (0..shortest)
        .take_while(|&index| {
            parents
                .iter()
                .all(|parent| parent[index] == parents[0][index])
        })
        .count()
}
