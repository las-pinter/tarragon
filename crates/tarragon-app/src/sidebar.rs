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

/// Take the queued sidebar events and act on them.
///
/// Returns whether any event was handled, so the caller can repaint: an answer
/// lands after the shell drew and no input event follows it.
pub fn handle_events(shell: &mut Shell, db: &Database) -> bool {
    let events = shell.take_sidebar_events();
    let handled = !events.is_empty();
    apply(shell, db, events);
    handled
}

/// Act on sidebar events; the view changes only after the database write worked.
pub fn apply(shell: &mut Shell, db: &Database, events: Vec<SidebarEvent>) {
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
            // Navigate and AddCurrent have no handler yet; the arm drains the queue.
            SidebarEvent::AddCurrent | SidebarEvent::Navigate(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temporary folder that removes itself on drop.
    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn new_dir() -> TempDir {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-app-sidebar-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        TempDir(dir)
    }

    /// A database in `dir` with favorites `a`, `b`, and `c`, of which only `a` and `b` exist.
    fn db_with_favorites(dir: &TempDir) -> Database {
        let db = Database::open(dir.0.join("t.db")).expect("open");
        db.initialize().expect("initialize");
        for name in ["a", "b"] {
            std::fs::create_dir_all(dir.0.join(name)).expect("create a favorite");
        }
        for (order, name) in ["a", "b", "c"].iter().enumerate() {
            let path = dir.0.join(name);
            let order = i64::try_from(order).unwrap();
            db.add_favorite(&path.to_string_lossy(), None, order)
                .expect("add");
        }
        db
    }

    fn names(shell: &mut Shell) -> Vec<String> {
        shell
            .sidebar_mut()
            .favorites()
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_roots_hold_home_and_an_outside_folder_once() {
        let home = Path::new("/home/u");
        let outside = Path::new("/data/pics");
        let expected = [home.to_path_buf(), outside.to_path_buf()];
        assert_eq!(roots(Some(home), Some(outside)), expected);
        assert_eq!(roots(Some(home), Some(Path::new("/home/u/pics"))), [home]);
        assert_eq!(roots(Some(home), Some(home)), [home]);
        assert_eq!(roots(Some(home), None), [home]);
        assert_eq!(roots(None, Some(outside)), [outside]);
    }

    #[test]
    fn init_sets_the_roots_the_favorites_and_the_open_folder() {
        let dir = new_dir();
        let db = db_with_favorites(&dir);
        let mut shell = Shell::new();
        let home = dir.0.join("home");
        let outside = dir.0.join("a");
        init(&mut shell, &db, Some(&home), Some(&outside));
        let sidebar = shell.sidebar_mut();
        assert_eq!(sidebar.roots(), [home, outside.clone()]);
        assert_eq!(sidebar.open_folder(), Some(outside.as_path()));
        assert_eq!(names(&mut shell), ["a", "b", "c"]);
    }

    #[test]
    fn a_missing_favorite_is_flagged() {
        let dir = new_dir();
        let db = db_with_favorites(&dir);
        let mut shell = Shell::new();
        init(&mut shell, &db, None, None);
        let flags: Vec<bool> = shell
            .sidebar_mut()
            .favorites()
            .iter()
            .map(|f| f.missing)
            .collect();
        assert_eq!(flags, [false, false, true]);
    }

    #[test]
    fn remove_deletes_from_the_db_and_the_list() {
        let dir = new_dir();
        let db = db_with_favorites(&dir);
        let mut shell = Shell::new();
        init(&mut shell, &db, None, None);
        apply(&mut shell, &db, vec![SidebarEvent::Remove(dir.0.join("b"))]);
        assert_eq!(names(&mut shell), ["a", "c"]);
        assert_eq!(db.list_favorites().unwrap().len(), 2);
    }

    #[test]
    fn reorder_persists_in_the_db_and_shows_in_the_list() {
        let dir = new_dir();
        let db = db_with_favorites(&dir);
        let mut shell = Shell::new();
        init(&mut shell, &db, None, None);
        let order = ["c", "a", "b"].map(|n| dir.0.join(n)).to_vec();
        apply(&mut shell, &db, vec![SidebarEvent::Reorder(order)]);
        assert_eq!(names(&mut shell), ["c", "a", "b"]);
        let stored: Vec<i64> = db
            .list_favorites()
            .unwrap()
            .iter()
            .map(|f| f.sort_order)
            .collect();
        assert_eq!(stored, [0, 1, 2]);
    }

    #[test]
    fn a_list_request_is_answered_with_the_real_subfolders() {
        let dir = new_dir();
        for name in ["x", "y"] {
            std::fs::create_dir_all(dir.0.join(name)).expect("create a subfolder");
        }
        std::fs::write(dir.0.join("file.txt"), b"").expect("create a file");
        let db = db_with_favorites(&dir);
        let mut shell = Shell::new();
        init(&mut shell, &db, Some(&dir.0), None);
        handle_events(&mut shell, &db);
        // The open folder shows its parent open, so the rows hold the children.
        shell.sidebar_mut().set_open_folder(Some(dir.0.join("x")));
        let rows: Vec<PathBuf> = shell
            .sidebar_mut()
            .tree_rows()
            .into_iter()
            .map(|row| row.path)
            .collect();
        assert!(rows.contains(&dir.0.join("x")) && rows.contains(&dir.0.join("y")));
        assert!(!rows.contains(&dir.0.join("file.txt")));
    }

    #[test]
    fn a_dot_dot_folder_is_normalized_for_the_roots_and_the_open_folder() {
        let dir = new_dir();
        let db = db_with_favorites(&dir);
        let mut shell = Shell::new();
        let home = Path::new("/home/u");
        init(
            &mut shell,
            &db,
            Some(home),
            Some(Path::new("/data/pics/../x/./y")),
        );
        let wanted = PathBuf::from("/data/x/y");
        assert_eq!(
            shell.sidebar_mut().roots(),
            [home.to_path_buf(), wanted.clone()]
        );
        assert_eq!(shell.sidebar_mut().open_folder(), Some(wanted.as_path()));
        assert_eq!(normalize(Path::new("/a/../../b")), Path::new("/b"));
    }

    #[test]
    fn home_comes_from_the_first_absolute_value() {
        let os = |s: &str| Some(OsString::from(s));
        assert_eq!(home_from(os("/h"), os("/p")), Some(PathBuf::from("/h")));
        assert_eq!(home_from(os(""), os("/p")), Some(PathBuf::from("/p")));
        assert_eq!(home_from(os("rel"), os("/p")), Some(PathBuf::from("/p")));
        assert_eq!(home_from(None, os("/p")), Some(PathBuf::from("/p")));
        assert_eq!(home_from(os(""), None), None);
        assert_eq!(home_from(os("rel"), os("")), None);
    }

    #[test]
    fn handle_events_reports_whether_it_handled_any() {
        let dir = new_dir();
        let db = db_with_favorites(&dir);
        let mut shell = Shell::new();
        init(&mut shell, &db, Some(&dir.0), None);
        assert!(handle_events(&mut shell, &db));
        assert!(!handle_events(&mut shell, &db));
    }

    #[test]
    fn a_list_request_for_a_missing_path_is_answered_with_nothing_and_not_asked_again() {
        let dir = new_dir();
        let db = db_with_favorites(&dir);
        let mut shell = Shell::new();
        init(&mut shell, &db, Some(&dir.0.join("gone")), None);
        assert!(handle_events(&mut shell, &db));
        let rows = shell.sidebar_mut().tree_rows();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].has_caret, "the answer marks the root as listed");
        shell.sidebar_mut().set_roots(vec![dir.0.join("gone")]);
        assert_eq!(shell.take_sidebar_events(), Vec::<SidebarEvent>::new());
    }

    #[test]
    fn a_database_error_leaves_the_view_unchanged() {
        let dir = new_dir();
        // No `initialize`, so there is no favorites table and every call fails.
        let db = Database::open(dir.0.join("bare.db")).expect("open");
        let mut shell = Shell::new();
        let row = |name: &str, order| FavoriteRow {
            path: dir.0.join(name),
            label: None,
            sort_order: order,
            missing: false,
        };
        let rows = vec![row("a", 0), row("b", 1)];
        shell.sidebar_mut().set_favorites(rows.clone());
        let events = vec![
            SidebarEvent::Remove(dir.0.join("a")),
            SidebarEvent::Reorder(vec![dir.0.join("b"), dir.0.join("a")]),
        ];
        apply(&mut shell, &db, events);
        assert_eq!(shell.sidebar_mut().favorites(), rows);
    }
}
