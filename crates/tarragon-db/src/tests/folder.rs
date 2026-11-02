use super::support::{TempDb, file};

/// An empty folder gives the path `/a.png`.
fn folders_of(files: &[(&str, &str)]) -> Vec<String> {
    let temp = TempDb::new();
    let rows: Vec<_> = files
        .iter()
        .map(|(folder, name)| file(folder, name, 1, 10))
        .collect();
    temp.db.write_scan_group(&rows).expect("write the group");
    temp.db.distinct_folders().expect("list the folders")
}

#[test]
fn two_files_in_one_folder_give_one_entry() {
    assert_eq!(
        folders_of(&[("/refs", "a.png"), ("/refs", "b.png")]),
        ["/refs"]
    );
}

#[test]
fn nested_folders_each_appear() {
    let folders = folders_of(&[("/refs", "a.png"), ("/refs/deep", "b.png")]);
    assert_eq!(folders, ["/refs", "/refs/deep"]);
}

#[test]
fn library_root_file_yields_the_root_folder() {
    assert_eq!(folders_of(&[("/lib", "a.png")]), ["/lib"]);
}

#[test]
fn bare_root_file_yields_no_entry() {
    assert_eq!(folders_of(&[("", "a.png")]), Vec::<String>::new());
}

#[test]
fn empty_table_gives_an_empty_list() {
    assert_eq!(folders_of(&[]), Vec::<String>::new());
}

#[test]
fn order_is_natural() {
    // A case-only difference breaks the tie with the raw code points.
    let folders = folders_of(&[("/a10", "x.png"), ("/a2", "y.png"), ("/A2", "z.png")]);
    assert_eq!(folders, ["/A2", "/a2", "/a10"]);
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn counter() -> impl FnMut() -> String {
    let mut next = 0;
    move || {
        next += 1;
        format!("new-{next}")
    }
}

#[test]
fn batch_keeps_stored_ids_and_creates_missing_ones() {
    let temp = TempDb::new();
    temp.db.upsert_folder_uuid("/a", "stored-a").expect("store");
    let ids = temp
        .db
        .get_or_create_folder_uuids(&names(&["/a", "/b"]), counter())
        .expect("batch");
    assert_eq!(ids.len(), 2);
    assert_eq!(ids["/a"], "stored-a");
    assert_eq!(ids["/b"], "new-1");
    assert_eq!(
        temp.db.folder_uuid("/b").expect("read"),
        Some("new-1".into())
    );
}

#[test]
fn batch_ids_stay_stable_on_a_second_call() {
    let temp = TempDb::new();
    let folders = names(&["/a", "/b", "/c"]);
    let first = temp
        .db
        .get_or_create_folder_uuids(&folders, counter())
        .expect("first");
    let mut calls = 0;
    let second = temp
        .db
        .get_or_create_folder_uuids(&folders, || {
            calls += 1;
            "other".to_owned()
        })
        .expect("second");
    assert_eq!(first, second);
    assert_eq!(calls, 0);
}

#[test]
fn batch_of_no_folders_gives_an_empty_map() {
    let temp = TempDb::new();
    let ids = temp
        .db
        .get_or_create_folder_uuids(&[], || panic!("no id needed"))
        .expect("batch");
    assert!(ids.is_empty());
}

#[test]
fn batch_creates_one_row_for_each_missing_folder() {
    let temp = TempDb::new();
    let folders: Vec<String> = (0..50).map(|n| format!("/f{n}")).collect();
    let ids = temp
        .db
        .get_or_create_folder_uuids(&folders, counter())
        .expect("batch");
    assert_eq!(ids.len(), 50);
    for folder in &folders {
        assert_eq!(
            temp.db.folder_uuid(folder).expect("read"),
            Some(ids[folder].clone())
        );
    }
    let mut unique: Vec<_> = ids.values().collect();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 50);
}
