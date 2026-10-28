//! The glue between the Library sidebar and the database.
//!
//! The panel only queues events. This module turns them into work, writes the
//! database first, and shows the result after the write succeeded.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use tarragon_db::{Database, DbError};
use tarragon_ui::Shell;
use tarragon_ui::sidebar::FavoriteRow;
use tarragon_ui::sidebar::lister::list_subdirs;
use tarragon_ui::sidebar::panel::SidebarEvent;

/// The home directory, or none when the environment does not name one.
pub fn home_dir() -> Option<PathBuf> {
    home_from(std::env::var_os("HOME"), std::env::var_os("USERPROFILE"))
}

/// The first of the two values that is an absolute path; an empty or relative value is skipped.
fn home_from(home: Option<OsString>, profile: Option<OsString>) -> Option<PathBuf> {
    [home, profile]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .find(|path| path.is_absolute())
}

/// Remove `.` and fold `..` without touching the disk.
///
/// A link stays a link, so the path matches the listed children of the tree.
/// A home that is a link next to a physical working folder can show the tree
/// twice; that limit is accepted.
pub(crate) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !path.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// The tree roots: the home directory, plus the command line folder when it is
/// outside home. No home and no folder gives no roots.
pub fn roots(home: Option<&Path>, folder: Option<&Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = home.map(Path::to_path_buf).into_iter().collect();
    if let Some(folder) = folder
        && !roots.iter().any(|root| folder.starts_with(root))
    {
        roots.push(folder.to_path_buf());
    }
    roots
}

/// Set the roots, load the favorites, and mark the open folder.
///
/// The folder is normalized first, so a `..` in it cannot hide it from the tree.
pub fn init(shell: &mut Shell, db: &Database, home: Option<&Path>, folder: Option<&Path>) {
    let folder = folder.map(normalize);
    shell
        .sidebar_mut()
        .set_roots(roots(home, folder.as_deref()));
    reload_favorites(shell, db, false);
    if let Some(folder) = folder {
        shell.sidebar_mut().set_open_folder(Some(folder));
    }
}

/// Read the favorites from the database and show them.
///
/// `missing` is computed here, not while drawing. It costs a stat per favorite,
/// and a dead mount can stall it. With `keep_flags`, a favorite that is already
/// shown keeps its flag, so a reorder or a remove stats nothing.
/// On a database error the view stays as it was.
fn reload_favorites(shell: &mut Shell, db: &Database, keep_flags: bool) {
    let known: HashMap<PathBuf, bool> = if keep_flags {
        shell
            .sidebar_mut()
            .favorites()
            .iter()
            .map(|row| (row.path.clone(), row.missing))
            .collect()
    } else {
        HashMap::new()
    };
    match load_favorites(db, &known) {
        Ok(rows) => shell.sidebar_mut().set_favorites(rows),
        Err(error) => tracing::error!("cannot load the favorites: {error}"),
    }
}

/// The stored favorites, each with its `missing` flag.
fn load_favorites(
    db: &Database,
    known: &HashMap<PathBuf, bool>,
) -> Result<Vec<FavoriteRow>, DbError> {
    Ok(db
        .list_favorites()?
        .into_iter()
        .map(|favorite| {
            let path = PathBuf::from(favorite.path);
            FavoriteRow {
                missing: known.get(&path).copied().unwrap_or_else(|| !path.is_dir()),
                path,
                label: favorite.label,
                sort_order: favorite.sort_order,
            }
        })
        .collect())
}

/// What `handle_events` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Handled {
    /// Whether any event was handled, so the caller can repaint: an answer
    /// lands after the shell drew and no input event follows it.
    pub any: bool,
    /// The folder of the last Navigate event; the caller opens it.
    pub navigate: Option<PathBuf>,
}

/// Take the queued sidebar events and act on them.
pub fn handle_events(shell: &mut Shell, db: &Database) -> Handled {
    let events = shell.take_sidebar_events();
    let any = !events.is_empty();
    let navigate = apply(shell, db, events);
    Handled { any, navigate }
}

/// Act on sidebar events; the view changes only after the database write worked.
///
/// Returns the target of the last Navigate event. The caller opens it.
pub fn apply(shell: &mut Shell, db: &Database, events: Vec<SidebarEvent>) -> Option<PathBuf> {
    let mut navigate = None;
    for event in events {
        match event {
            // This read runs on the UI thread. It happens on a click, never per frame.
            SidebarEvent::ListChildren(path) => {
                let children = list_subdirs(&path);
                shell.sidebar_mut().set_children(&path, children);
            }
            SidebarEvent::Remove(path) => match db.remove_favorite(&path.to_string_lossy()) {
                Ok(_) => reload_favorites(shell, db, true),
                Err(error) => tracing::error!("cannot remove the favorite: {error}"),
            },
            SidebarEvent::Reorder(paths) => {
                let paths: Vec<String> = paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                match db.reorder_favorites(&paths) {
                    Ok(_) => reload_favorites(shell, db, true),
                    Err(error) => tracing::error!("cannot save the favorite order: {error}"),
                }
            }
            SidebarEvent::Navigate(path) => navigate = Some(path),
            SidebarEvent::AddCurrent => add_current(shell, db),
        }
    }
    navigate
}

/// Add the open folder to the favorites, last in the list.
///
/// The folder is the sidebar's open folder: the normalized path that the tree
/// and the favorites use. With no open folder nothing happens. A folder that is
/// already a favorite changes nothing. The view changes only after the write
/// worked. Flags are kept, so only the new path is checked on disk.
fn add_current(shell: &mut Shell, db: &Database) {
    let Some(folder) = shell.sidebar_mut().open_folder().map(Path::to_path_buf) else {
        return;
    };
    let result = db.list_favorites().and_then(|list| {
        let next = list
            .iter()
            .map(|fav| fav.sort_order)
            .max()
            .map_or(0, |max| max.saturating_add(1));
        db.add_favorite(&folder.to_string_lossy(), None, next)
    });
    match result {
        Ok(0) => {}
        Ok(_) => reload_favorites(shell, db, true),
        Err(error) => tracing::error!("cannot add the favorite: {error}"),
    }
}

#[cfg(test)]
mod tests;
