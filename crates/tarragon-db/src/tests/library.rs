//! Library file list tests.

use crate::repository::{LibraryFile, ThumbnailWrite};

use super::support::{TempDb, file};

/// Store a rendered row with a 320 px cache path.
fn render(temp: &TempDb, path: &str, mtime: i64, size: i64) {
    temp.db
        .upsert_thumbnail(&ThumbnailWrite {
            path: path.to_owned(),
            mtime,
            size,
            width: 4,
            height: 4,
            cache_uuid: "u".to_owned(),
            thumbnail_cache_path: Some("u/a.jpg".to_owned()),
            preview_cache_path: None,
            full_cache_path: None,
            last_accessed: 1,
        })
        .expect("store the rendered row");
}

fn sorted(temp: &TempDb) -> Vec<LibraryFile> {
    let mut files = temp.db.library_files().expect("list the library");
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

#[test]
fn rows_from_three_folders_all_come_back() {
    let temp = TempDb::new();
    let rows: Vec<_> = ["/a", "/b", "/c/deep"]
        .iter()
        .flat_map(|folder| [file(folder, "x.png", 1, 10), file(folder, "y.png", 2, 20)])
        .collect();
    temp.db.write_scan_group(&rows).expect("write the scan");
    let files = sorted(&temp);
    assert_eq!(files.len(), 6);
    let count = temp.db.count_thumbnails().expect("count the rows");
    assert_eq!(i64::try_from(files.len()).expect("fits"), count);
}

#[test]
fn cached_is_true_only_with_a_cache_path() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("/a", "plain.png", 1, 10)])
        .expect("write the scan");
    render(&temp, "/a/done.png", 1, 10);
    let flags: Vec<_> = sorted(&temp).iter().map(|f| f.cached).collect();
    assert_eq!(flags, [true, false]);
}

#[test]
fn an_empty_database_gives_an_empty_list() {
    let temp = TempDb::new();
    assert_eq!(sorted(&temp), Vec::<LibraryFile>::new());
}

#[test]
fn fields_equal_the_stored_row() {
    let temp = TempDb::new();
    render(&temp, "/a/done.png", 77, 1234);
    assert_eq!(
        sorted(&temp),
        [LibraryFile {
            path: "/a/done.png".to_owned(),
            mtime: 77,
            size: 1234,
            cached: true,
        }]
    );
}

#[test]
fn a_scanned_row_with_no_render_is_listed_uncached() {
    let temp = TempDb::new();
    temp.db
        .write_scan_group(&[file("/a", "new.png", 5, 50)])
        .expect("write the scan");
    let files = sorted(&temp);
    assert_eq!(files.len(), 1);
    assert!(!files[0].cached);
    assert_eq!((files[0].mtime, files[0].size), (5, 50));
}
