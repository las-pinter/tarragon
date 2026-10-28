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
    assert!(handle_events(&mut shell, &db).any);
    assert!(!handle_events(&mut shell, &db).any);
}

#[test]
fn a_list_request_for_a_missing_path_is_answered_with_nothing_and_not_asked_again() {
    let dir = new_dir();
    let db = db_with_favorites(&dir);
    let mut shell = Shell::new();
    init(&mut shell, &db, Some(&dir.0.join("gone")), None);
    assert!(handle_events(&mut shell, &db).any);
    let rows = shell.sidebar_mut().tree_rows();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].has_caret, "the answer marks the root as listed");
    shell.sidebar_mut().set_roots(vec![dir.0.join("gone")]);
    assert_eq!(shell.take_sidebar_events(), Vec::<SidebarEvent>::new());
}

#[test]
fn the_navigate_arm_returns_the_last_target() {
    let dir = new_dir();
    let db = db_with_favorites(&dir);
    let mut shell = Shell::new();
    let events = vec![
        SidebarEvent::Navigate(dir.0.join("a")),
        SidebarEvent::AddCurrent,
        SidebarEvent::Navigate(dir.0.join("b")),
    ];
    assert_eq!(apply(&mut shell, &db, events), Some(dir.0.join("b")));
    assert_eq!(apply(&mut shell, &db, vec![SidebarEvent::AddCurrent]), None);
}

#[test]
fn add_current_adds_the_open_folder_last_and_only_once() {
    let dir = new_dir();
    let db = db_with_favorites(&dir);
    let mut shell = Shell::new();
    init(&mut shell, &db, None, None);
    let open = dir.0.join("new");
    std::fs::create_dir_all(&open).expect("create the folder");
    shell.sidebar_mut().set_open_folder(Some(open.clone()));
    apply(&mut shell, &db, vec![SidebarEvent::AddCurrent]);
    assert_eq!(names(&mut shell), ["a", "b", "c", "new"]);
    let stored = db.list_favorites().unwrap();
    assert_eq!(stored.last().map(|f| f.sort_order), Some(3));
    let fresh = shell.sidebar_mut().favorites().last().cloned().unwrap();
    assert!(!fresh.missing, "the new flag is checked on disk");
    apply(&mut shell, &db, vec![SidebarEvent::AddCurrent]);
    assert_eq!(db.list_favorites().unwrap().len(), 4);
    assert_eq!(names(&mut shell), ["a", "b", "c", "new"]);
}

#[test]
fn add_current_with_no_folder_or_a_missing_one() {
    let dir = new_dir();
    let db = db_with_favorites(&dir);
    let mut shell = Shell::new();
    init(&mut shell, &db, None, None);
    apply(&mut shell, &db, vec![SidebarEvent::AddCurrent]);
    assert_eq!(db.list_favorites().unwrap().len(), 3);
    // A folder that is gone is added and flagged missing.
    shell
        .sidebar_mut()
        .set_open_folder(Some(dir.0.join("gone")));
    apply(&mut shell, &db, vec![SidebarEvent::AddCurrent]);
    let last = shell.sidebar_mut().favorites().last().cloned().unwrap();
    assert!(last.missing);
}

#[test]
fn add_current_orders_after_the_largest_order() {
    let dir = new_dir();
    let db = Database::open(dir.0.join("o.db")).expect("open");
    db.initialize().expect("initialize");
    let mut shell = Shell::new();
    let first = dir.0.join("first");
    shell.sidebar_mut().set_open_folder(Some(first.clone()));
    apply(&mut shell, &db, vec![SidebarEvent::AddCurrent]);
    assert_eq!(db.list_favorites().unwrap()[0].sort_order, 0);
    for (name, order) in [("n", -5), ("p", 7)] {
        let path = dir.0.join(name);
        db.add_favorite(&path.to_string_lossy(), None, order)
            .unwrap();
    }
    let last = dir.0.join("last");
    shell.sidebar_mut().set_open_folder(Some(last.clone()));
    apply(&mut shell, &db, vec![SidebarEvent::AddCurrent]);
    let stored = db.list_favorites().unwrap();
    let added = stored
        .iter()
        .find(|f| f.path == last.to_string_lossy())
        .unwrap();
    assert_eq!(added.sort_order, 8);
}

#[test]
fn a_database_error_during_add_current_leaves_the_view_unchanged() {
    let dir = new_dir();
    // No `initialize`, so there is no favorites table and every call fails.
    let db = Database::open(dir.0.join("bare.db")).expect("open");
    let mut shell = Shell::new();
    let row = FavoriteRow {
        path: dir.0.join("a"),
        label: None,
        sort_order: 0,
        missing: false,
    };
    shell.sidebar_mut().set_favorites(vec![row.clone()]);
    shell.sidebar_mut().set_open_folder(Some(dir.0.join("b")));
    apply(&mut shell, &db, vec![SidebarEvent::AddCurrent]);
    assert_eq!(shell.sidebar_mut().favorites(), [row]);
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
