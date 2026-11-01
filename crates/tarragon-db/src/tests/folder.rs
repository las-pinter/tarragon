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
