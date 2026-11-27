//! Tests for the favorite label rule.

use super::*;

/// A favorite at `path` with an optional stored label.
fn fav(path: &str, label: Option<&str>) -> FavoriteRow {
    FavoriteRow {
        path: PathBuf::from(path),
        label: label.map(ToOwned::to_owned),
        sort_order: 0,
        missing: false,
    }
}

/// The labels of favorites given as paths with no stored label.
fn shown(paths: &[&str]) -> Vec<String> {
    let rows: Vec<_> = paths.iter().map(|path| fav(path, None)).collect();
    favorite_labels(&rows)
}

#[test]
fn a_favorite_shows_its_folder_name() {
    assert_eq!(
        shown(&["/home/u/refs/sketches", "/home/u/art/2026-10"]),
        ["sketches", "2026-10"]
    );
}

#[test]
fn a_stored_label_wins() {
    let rows = [fav("/a/b/c", Some("Mine"))];
    assert_eq!(favorite_labels(&rows), ["Mine"]);
}

#[test]
fn a_blank_label_is_ignored() {
    let rows = [fav("/a/b/c", Some("")), fav("/a/b/d", Some("  \t"))];
    assert_eq!(favorite_labels(&rows), ["c", "d"]);
}

#[test]
fn shared_names_show_the_path_below_the_folder_their_parents_share() {
    assert_eq!(
        shown(&[
            "/home/u/refs/clientA/2026-10",
            "/home/u/refs/clientB/2026-10",
            "/home/u/refs/clientB/hands",
        ]),
        ["clientA/2026-10", "clientB/2026-10", "hands"]
    );
}

#[test]
fn three_shared_names_use_the_folder_all_three_share() {
    assert_eq!(
        shown(&["/p/a/x/day", "/p/a/y/day", "/p/b/day"]),
        ["a/x/day", "a/y/day", "b/day"]
    );
}

#[test]
fn a_shared_name_in_sibling_folders_keeps_one_parent_part() {
    assert_eq!(shown(&["/p/a/day", "/p/b/day"]), ["a/day", "b/day"]);
}

#[test]
fn a_folder_inside_a_namesake_keeps_its_parent_parts() {
    assert_eq!(shown(&["/p/day", "/p/day/day"]), ["day", "day/day"]);
}

#[test]
fn a_labelled_favorite_does_not_count_as_a_namesake() {
    let rows = [fav("/p/a/day", Some("Today")), fav("/p/b/day", None)];
    assert_eq!(favorite_labels(&rows), ["Today", "day"]);
}

#[test]
fn a_folder_with_no_name_shows_its_full_path() {
    assert_eq!(shown(&["/", "/a"]), ["/", "a"]);
}

#[test]
fn trailing_and_double_slashes_collapse() {
    assert_eq!(shown(&["/a//x/c/", "/a/y//c"]), ["x/c", "y/c"]);
}

#[test]
fn drive_letter_paths_work_with_slashes() {
    assert_eq!(shown(&["C:/x/a/day", "C:/x/b/day"]), ["a/day", "b/day"]);
    assert_eq!(shown(&["C:/x/day", "D:/x/day"]), ["C:/x/day", "D:/x/day"]);
}

#[test]
fn namesakes_that_share_no_folder_show_their_full_paths() {
    assert_eq!(shown(&["/a/day", "b/day"]), ["/a/day", "b/day"]);
}

#[test]
fn an_empty_list_gives_no_labels() {
    assert_eq!(favorite_labels(&[]), Vec::<String>::new());
}

#[test]
fn state_stores_and_returns_its_data() {
    let mut state = SidebarState::default();
    let row = FavoriteRow {
        path: PathBuf::from("/a"),
        label: None,
        sort_order: 1,
        missing: false,
    };
    state.set_favorites(vec![row.clone()]);
    state.set_roots(vec![PathBuf::from("/a")]);
    state.set_selected(Some(PathBuf::from("/a")));
    assert_eq!(state.favorites(), [row]);
    assert_eq!(state.roots(), [PathBuf::from("/a")]);
    assert_eq!(state.selected(), Some(Path::new("/a")));
}

#[cfg(unix)]
#[test]
fn a_non_utf8_name_shows_the_replacement_character() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let rows = [FavoriteRow {
        path: PathBuf::from(OsStr::from_bytes(b"/a/b/\xff")),
        label: None,
        sort_order: 0,
        missing: false,
    }];
    assert_eq!(favorite_labels(&rows), ["\u{fffd}"]);
}
