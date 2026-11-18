use crate::Database;

use super::support::TempDb;

/// Read one integer pragma from the shared connection.
fn int_pragma(db: &Database, name: &str) -> i64 {
    let conn = db.lock().expect("lock the connection");
    conn.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .expect("read the pragma")
}

/// Read the journal mode from the shared connection.
fn journal_mode(db: &Database) -> String {
    let conn = db.lock().expect("lock the connection");
    conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("read the journal mode")
}

/// A freshly opened connection uses NORMAL, not the FULL default.
#[test]
fn open_sets_synchronous_normal() {
    let temp = TempDb::new();
    let db = Database::open(temp.path()).expect("open a second connection");
    assert_eq!(int_pragma(&db, "synchronous"), 1);
}

/// The schema setup does not change the synchronous value.
#[test]
fn synchronous_stays_normal_after_initialize() {
    let temp = TempDb::new();
    let db = Database::open(temp.path()).expect("open a second connection");
    db.initialize().expect("create the schema");
    assert_eq!(int_pragma(&db, "synchronous"), 1);
    assert_eq!(int_pragma(&temp.db, "synchronous"), 1);
}

/// WAL and foreign keys stay on next to the synchronous setting.
#[test]
fn wal_and_foreign_keys_stay_on() {
    let temp = TempDb::new();
    assert_eq!(journal_mode(&temp.db), "wal");
    assert_eq!(int_pragma(&temp.db, "foreign_keys"), 1);
}
