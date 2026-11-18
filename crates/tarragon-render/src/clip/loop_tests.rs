//! A b-tree page that points back at itself must fail, not loop.

use std::io::Cursor;

use super::sqlite::Database;

/// Build a 512-byte database whose only page is an interior table page. The
/// page lists itself as its child and as its right-most child.
fn self_referencing_database() -> Vec<u8> {
    let mut db = vec![0_u8; 512];
    db[..16].copy_from_slice(b"SQLite format 3\0");
    db[16..18].copy_from_slice(&512_u16.to_be_bytes());
    db[28..32].copy_from_slice(&1_u32.to_be_bytes());
    db[100] = 5; // interior table page after the database header
    db[103..105].copy_from_slice(&1_u16.to_be_bytes()); // one cell
    db[105..107].copy_from_slice(&500_u16.to_be_bytes()); // content start
    db[108..112].copy_from_slice(&1_u32.to_be_bytes()); // right-most child
    db[112..114].copy_from_slice(&500_u16.to_be_bytes()); // the cell pointer
    db[500..504].copy_from_slice(&1_u32.to_be_bytes()); // the cell's child
    db
}

#[test]
fn a_self_referencing_schema_page_is_an_error() {
    let mut database = Database::open(Cursor::new(self_referencing_database()), 0).expect("open");
    let error = database.lookup_table("CanvasPreview").err();
    assert!(format!("{error:?}").contains("loop"), "{error:?}");
}

#[test]
fn a_self_referencing_table_page_is_an_error() {
    let mut database = Database::open(Cursor::new(self_referencing_database()), 0).expect("open");
    let error = database.first_row(1).err();
    assert!(format!("{error:?}").contains("loop"), "{error:?}");
}
