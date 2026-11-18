//! Tests for the favorite label rule.

use super::*;

/// Build roots from string slices.
fn roots(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

/// Run the rule on string inputs.
fn shown(path: &str, label: Option<&str>, list: &[&str]) -> String {
    display_label(Path::new(path), label, &roots(list))
}

#[test]
fn stored_label_wins() {
    assert_eq!(shown("/a/b/c", Some("Mine"), &["/a/b"]), "Mine");
}

#[test]
fn empty_label_is_ignored() {
    assert_eq!(shown("/a/b/c", Some(""), &["/a/b"]), "c");
}

#[test]
fn nested_roots_choose_the_longest() {
    let list = ["/a", "/a/b", "/x"];
    assert_eq!(
        shown("/a/b/sketches/2026-10", None, &list),
        "sketches/2026-10"
    );
}

#[test]
fn root_match_needs_a_component_boundary() {
    assert_eq!(shown("/a/bc", None, &["/a/b"]), "/a/bc");
}

#[test]
fn path_equal_to_root_shows_its_last_component() {
    assert_eq!(shown("/a/b", None, &["/a/b"]), "b");
}

#[test]
fn no_root_match_shows_the_full_path() {
    assert_eq!(shown("/q/r", None, &["/a/b"]), "/q/r");
}

#[test]
fn root_with_trailing_slash_matches() {
    assert_eq!(shown("/a/b/c/d", None, &["/a/b/"]), "c/d");
    assert_eq!(shown("/a/b", None, &["/a/b/"]), "b");
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
    state.set_roots(roots(&["/a"]));
    state.set_selected(Some(PathBuf::from("/a")));
    assert_eq!(state.favorites(), [row]);
    assert_eq!(state.roots(), roots(&["/a"]));
    assert_eq!(state.selected(), Some(Path::new("/a")));
}

#[test]
fn blank_label_is_ignored() {
    assert_eq!(shown("/a/b/c", Some("  \t"), &["/a/b"]), "c");
}

#[test]
fn root_slash_strips_only_the_slash() {
    assert_eq!(shown("/", None, &["/"]), "/");
    assert_eq!(shown("/a/b", None, &["/"]), "a/b");
}

#[test]
fn relative_path_does_not_match_an_absolute_root() {
    assert_eq!(shown("a/b/c", None, &["/a/b"]), "a/b/c");
}

#[test]
fn drive_letter_paths_work_with_slashes() {
    assert_eq!(shown("C:/x/y", None, &["C:/x"]), "y");
}

#[test]
fn trailing_and_double_slashes_in_the_path_collapse() {
    assert_eq!(shown("/a/b/c/", None, &["/a/b"]), "c");
    assert_eq!(shown("/a//b//c", None, &["/a"]), "b/c");
}

#[test]
fn empty_root_is_ignored() {
    assert_eq!(shown("/a/b", None, &[""]), "/a/b");
    assert_eq!(shown("/a/b/c", None, &["", "/a"]), "b/c");
}

#[cfg(unix)]
#[test]
fn non_utf8_part_shows_the_replacement_character() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let path = Path::new(OsStr::from_bytes(b"/a/b/\xff"));
    assert_eq!(display_label(path, None, &roots(&["/a/b"])), "\u{fffd}");
}
