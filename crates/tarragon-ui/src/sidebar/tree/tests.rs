//! Tests for the folder tree rows.

use super::*;

/// Build paths from string slices.
fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

/// A tree with one root `/r` and the given child lists, all open.
fn model(listed: &[(&str, &[&str])]) -> TreeModel {
    let mut tree = TreeModel::default();
    tree.set_roots(&paths(&["/r"]));
    tree.set_expanded(Path::new("/r"), true);
    for (parent, kids) in listed {
        tree.set_children(Path::new(parent), paths(kids));
        for kid in *kids {
            tree.set_expanded(Path::new(kid), true);
        }
    }
    tree
}

/// The depth and name of each row.
fn shape(tree: &TreeModel) -> Vec<(usize, String)> {
    tree.rows().into_iter().map(|r| (r.depth, r.name)).collect()
}

#[test]
fn rows_reach_depth_three() {
    let tree = model(&[
        ("/r", &["/r/a"]),
        ("/r/a", &["/r/a/b"]),
        ("/r/a/b", &["/r/a/b/c"]),
    ]);
    let want: Vec<_> = ["r", "a", "b", "c"]
        .iter()
        .enumerate()
        .map(|(depth, name)| (depth, (*name).to_owned()))
        .collect();
    assert_eq!(shape(&tree), want);
}

#[test]
fn closed_node_hides_its_children() {
    let mut tree = model(&[("/r", &["/r/a"])]);
    tree.toggle(Path::new("/r"));
    assert_eq!(shape(&tree), [(0, "r".to_owned())]);
    tree.toggle(Path::new("/r"));
    assert_eq!(tree.rows().len(), 2);
}

#[test]
fn listed_empty_node_has_no_caret() {
    let tree = model(&[("/r", &[])]);
    assert!(!tree.rows()[0].has_caret);
}

#[test]
fn unlisted_node_has_a_caret() {
    let tree = model(&[]);
    assert!(tree.rows()[0].has_caret);
}

#[test]
fn two_roots_keep_their_order() {
    let mut tree = TreeModel::default();
    tree.set_roots(&paths(&["/z", "/a"]));
    let names: Vec<_> = tree.rows().into_iter().map(|r| r.name).collect();
    assert_eq!(names, ["z", "a"]);
}

#[test]
fn root_slash_shows_a_slash() {
    let mut tree = TreeModel::default();
    tree.set_roots(&paths(&["/"]));
    assert_eq!(tree.rows()[0].name, "/");
}

#[test]
fn replacing_children_keeps_a_surviving_child_open() {
    let mut tree = model(&[("/r", &["/r/a", "/r/b"])]);
    tree.set_children(Path::new("/r"), paths(&["/r/b", "/r/c"]));
    let rows = tree.rows();
    let open: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.expanded)).collect();
    assert_eq!(open, [("r", true), ("b", true), ("c", false)]);
}

#[test]
fn a_removed_child_is_forgotten() {
    let mut tree = model(&[("/r", &["/r/a"])]);
    tree.set_children(Path::new("/r"), paths(&["/r/b"]));
    tree.set_children(Path::new("/r"), paths(&["/r/a"]));
    assert!(!tree.rows()[1].expanded);
}

#[test]
fn unknown_path_is_ignored() {
    let mut tree = model(&[]);
    tree.set_children(Path::new("/nope"), paths(&["/nope/x"]));
    tree.toggle(Path::new("/nope"));
    tree.set_expanded(Path::new("/nope"), true);
    assert_eq!(tree.rows().len(), 1);
    assert_eq!(tree.nodes.len(), 1);
}

#[test]
fn a_forgotten_grandchild_comes_back_closed() {
    let mut tree = model(&[("/r", &["/r/a"]), ("/r/a", &["/r/a/b"])]);
    tree.set_children(Path::new("/r"), paths(&[]));
    tree.set_children(Path::new("/r"), paths(&["/r/a"]));
    tree.set_children(Path::new("/r/a"), paths(&["/r/a/b"]));
    assert!(!tree.nodes[Path::new("/r/a")].expanded);
    assert!(!tree.nodes[Path::new("/r/a/b")].expanded);
}

#[test]
fn a_readded_child_is_unlisted_again() {
    let mut tree = model(&[("/r", &["/r/a"]), ("/r/a", &[])]);
    assert!(!tree.rows()[1].has_caret);
    tree.set_children(Path::new("/r"), paths(&[]));
    tree.set_children(Path::new("/r"), paths(&["/r/a"]));
    assert!(tree.rows()[1].has_caret);
}

#[test]
fn set_roots_keeps_a_surviving_root() {
    let mut tree = model(&[("/r", &["/r/a"])]);
    tree.set_roots(&paths(&["/q", "/r"]));
    let rows = tree.rows();
    assert_eq!((rows[0].name.as_str(), rows[0].expanded), ("q", false));
    assert_eq!((rows[1].name.as_str(), rows[1].expanded), ("r", true));
    assert_eq!(rows[2].name, "a");
}

#[test]
fn a_dropped_root_is_forgotten() {
    let mut tree = model(&[("/r", &["/r/a"])]);
    tree.set_roots(&paths(&["/q"]));
    assert_eq!(tree.nodes.len(), 1);
    tree.set_roots(&paths(&["/r"]));
    assert!(!tree.rows()[0].expanded);
}

/// Roots `/home` and `/home/dev/cli`, with `/home/dev` between them.
fn nested() -> TreeModel {
    let mut tree = TreeModel::default();
    tree.set_roots(&paths(&["/home", "/home/dev/cli"]));
    tree.set_children(Path::new("/home"), paths(&["/home/dev"]));
    tree.set_children(Path::new("/home/dev"), paths(&["/home/dev/cli"]));
    tree.set_expanded(Path::new("/home/dev/cli"), true);
    tree
}

#[test]
fn a_nested_root_survives_dropping_its_ancestor_root() {
    let mut tree = nested();
    tree.set_roots(&paths(&["/home/dev/cli"]));
    let rows = tree.rows();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].expanded);
}

#[test]
fn a_nested_root_survives_a_relist_that_drops_its_parent() {
    let mut tree = nested();
    tree.set_children(Path::new("/home"), paths(&[]));
    assert!(tree.nodes.contains_key(Path::new("/home/dev/cli")));
    assert!(tree.rows().iter().any(|r| r.name == "cli" && r.expanded));
}

#[test]
fn a_child_under_two_parents_stays_while_one_lists_it() {
    let mut tree = TreeModel::default();
    tree.set_roots(&paths(&["/a", "/a/b"]));
    tree.set_children(Path::new("/a"), paths(&["/a/b/c"]));
    tree.set_children(Path::new("/a/b"), paths(&["/a/b/c"]));
    tree.set_expanded(Path::new("/a/b/c"), true);
    tree.set_children(Path::new("/a"), paths(&[]));
    assert!(tree.nodes[Path::new("/a/b/c")].expanded);
}

#[test]
fn children_outside_the_parent_and_repeats_are_dropped() {
    let mut tree = model(&[]);
    tree.set_children(Path::new("/r"), paths(&["/r", "/", "/x", "/r/a", "/r/a"]));
    let names: Vec<_> = tree.rows().into_iter().map(|r| r.name).collect();
    assert_eq!(names, ["r", "a"]);
}

#[test]
fn set_expanded_sets_the_flag_directly() {
    let mut tree = model(&[("/r", &["/r/a"])]);
    tree.set_expanded(Path::new("/r"), false);
    assert!(!tree.rows()[0].expanded);
    tree.set_expanded(Path::new("/r"), true);
    assert!(tree.rows()[0].expanded);
}

#[test]
fn is_listed_is_true_only_for_a_listed_known_folder() {
    let mut tree = TreeModel::default();
    tree.set_roots(&paths(&["/r"]));
    assert!(!tree.is_listed(Path::new("/r")));
    tree.set_children(Path::new("/r"), paths(&["/r/a"]));
    assert!(tree.is_listed(Path::new("/r")));
    assert!(!tree.is_listed(Path::new("/r/a")));
    assert!(!tree.is_listed(Path::new("/nope")));
}
