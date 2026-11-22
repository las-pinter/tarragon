//! The eviction order of thumbnail rows when access times are equal.

use super::support::{TempDb, file};

#[test]
fn rows_with_the_same_access_time_are_listed_in_path_order() {
    let temp = TempDb::new();
    // Written out of path order, so insertion order cannot pass the test.
    let files = [
        file("/refs", "c.png", 1, 10),
        file("/refs", "a.png", 1, 10),
        file("/refs", "b.png", 1, 10),
    ];
    temp.db.write_scan_group(&files).expect("write the rows");
    let paths: Vec<String> = files
        .iter()
        .map(|info| info.path.to_string_lossy().into_owned())
        .collect();
    temp.db.touch_last_accessed(&paths, 10).expect("touch all");

    let ordered = temp
        .db
        .thumbnails_by_last_accessed()
        .expect("list the rows");
    let listed: Vec<&str> = ordered.iter().map(|row| row.path.as_str()).collect();
    assert_eq!(listed, vec!["/refs/a.png", "/refs/b.png", "/refs/c.png"]);
}
