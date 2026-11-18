//! Favorite row tests.

use crate::repository::Favorite;

use super::support::TempDb;

/// Return the paths of `rows` in order.
fn paths(rows: &[Favorite]) -> Vec<&str> {
    rows.iter().map(|row| row.path.as_str()).collect()
}

#[test]
fn reorder_favorites_places_listed_first_and_leaves_no_ties() {
    let temp = TempDb::new();
    for (index, path) in ["/f/a", "/f/b", "/f/c", "/f/d"].into_iter().enumerate() {
        let order = i64::try_from(index).expect("small index");
        temp.db.add_favorite(path, None, order).expect("add");
    }
    let listed = ["/f/c", "/f/a", "/f/c", "/f/missing"].map(str::to_owned);
    let placed = temp.db.reorder_favorites(&listed).expect("reorder");
    assert_eq!(placed, 2, "a repeat and a missing path are not placed");

    let rows = temp.db.list_favorites().expect("list");
    assert_eq!(paths(&rows), ["/f/c", "/f/a", "/f/b", "/f/d"]);
    let orders: Vec<i64> = rows.iter().map(|row| row.sort_order).collect();
    assert_eq!(orders, [0, 1, 2, 3]);
}

#[test]
fn add_favorite_then_list_returns_path_label_and_order() {
    let temp = TempDb::new();
    let changed = temp.db.add_favorite("/f/a", Some("Alpha"), 3).expect("add");
    assert_eq!(changed, 1, "a new path inserts one row");

    let rows = temp.db.list_favorites().expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, "/f/a");
    assert_eq!(rows[0].label.as_deref(), Some("Alpha"));
    assert_eq!(rows[0].sort_order, 3);
}

#[test]
fn list_favorites_orders_by_sort_order_not_insert_order() {
    let temp = TempDb::new();
    temp.db.add_favorite("/f/a", None, 2).expect("add a");
    temp.db.add_favorite("/f/b", None, 0).expect("add b");
    temp.db.add_favorite("/f/c", None, 1).expect("add c");

    let rows = temp.db.list_favorites().expect("list");
    assert_eq!(paths(&rows), ["/f/b", "/f/c", "/f/a"]);
}

#[test]
fn add_favorite_twice_keeps_the_first_row() {
    let temp = TempDb::new();
    temp.db
        .add_favorite("/f/a", Some("First"), 0)
        .expect("first add");
    let changed = temp
        .db
        .add_favorite("/f/a", Some("Second"), 5)
        .expect("second add is not an error");
    assert_eq!(changed, 0, "a repeated path changes no row");

    let rows = temp.db.list_favorites().expect("list");
    assert_eq!(rows.len(), 1, "the path must not duplicate");
    assert_eq!(rows[0].label.as_deref(), Some("First"));
    assert_eq!(rows[0].sort_order, 0);
}

#[test]
fn remove_favorite_deletes_the_named_row_only() {
    let temp = TempDb::new();
    temp.db.add_favorite("/f/a", None, 0).expect("add a");
    temp.db.add_favorite("/f/b", None, 1).expect("add b");

    let removed = temp.db.remove_favorite("/f/a").expect("remove");
    assert_eq!(removed, 1);
    let rows = temp.db.list_favorites().expect("list");
    assert_eq!(paths(&rows), ["/f/b"]);
}

#[test]
fn remove_missing_favorite_returns_zero_without_error() {
    let temp = TempDb::new();
    temp.db.add_favorite("/f/a", None, 0).expect("add");

    let removed = temp.db.remove_favorite("/f/missing").expect("no error");
    assert_eq!(removed, 0);
    assert_eq!(temp.db.list_favorites().expect("list").len(), 1);
}

#[test]
fn update_favorite_sort_order_changes_list_order() {
    let temp = TempDb::new();
    temp.db.add_favorite("/f/a", None, 0).expect("add a");
    temp.db.add_favorite("/f/b", None, 1).expect("add b");

    let changed = temp
        .db
        .update_favorite_sort_order("/f/a", 9)
        .expect("update");
    assert_eq!(changed, 1);
    let rows = temp.db.list_favorites().expect("list");
    assert_eq!(paths(&rows), ["/f/b", "/f/a"]);
    assert_eq!(rows[1].sort_order, 9);

    let missing = temp
        .db
        .update_favorite_sort_order("/f/missing", 1)
        .expect("no error");
    assert_eq!(missing, 0, "a missing path changes no row");
}

#[test]
fn favorite_paths_match_across_backslash_and_slash_spellings() {
    let temp = TempDb::new();
    temp.db.add_favorite("C:\\refs\\a", None, 0).expect("add");

    let rows = temp.db.list_favorites().expect("list");
    assert_eq!(paths(&rows), ["C:/refs/a"], "the stored path uses slashes");

    let dup = temp.db.add_favorite("C:/refs/a", None, 1).expect("re-add");
    assert_eq!(dup, 0, "the other spelling hits the same row");

    let updated = temp
        .db
        .update_favorite_sort_order("C:/refs/a", 4)
        .expect("update");
    assert_eq!(updated, 1);
    let removed = temp.db.remove_favorite("C:\\refs\\a").expect("remove");
    assert_eq!(removed, 1);
    let rows = temp.db.list_favorites().expect("list");
    assert_eq!(paths(&rows), Vec::<&str>::new());
}
