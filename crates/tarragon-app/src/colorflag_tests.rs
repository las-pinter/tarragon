use tarragon_db::Database;
use tarragon_db::settings::{SettingKey, SettingValue, Settings};
use tarragon_ui::Shell;

use crate::colorflag;
use crate::tags;

/// A database in a temp folder that removes itself on drop.
struct TempDb {
    dir: std::path::PathBuf,
    db: Database,
}

impl TempDb {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-colorflag-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        let db = Database::open(dir.join("tarragon.db")).expect("open");
        db.initialize().expect("initialize");
        Self { dir, db }
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Store the colour tags setting.
fn store(db: &Database, enabled: bool) {
    Settings::new(db)
        .set(SettingKey::ColorTagEnabled, SettingValue::Bool(enabled))
        .expect("store the setting");
}

/// The Preview and the Gallery flags.
fn flags(shell: &mut Shell) -> (bool, bool) {
    (
        shell.preview_mut().colors_enabled(),
        shell.gallery_mut().colors_enabled(),
    )
}

#[test]
fn load_reads_a_stored_off_setting_into_both_flags() {
    let temp = TempDb::new();
    store(&temp.db, false);
    let mut shell = Shell::new();

    colorflag::load(&mut shell, &temp.db);

    assert_eq!(flags(&mut shell), (false, false));
}

#[test]
fn load_with_no_stored_setting_turns_both_flags_on() {
    let temp = TempDb::new();
    let mut shell = Shell::new();
    colorflag::apply(&mut shell, false);

    colorflag::load(&mut shell, &temp.db);

    assert_eq!(flags(&mut shell), (true, true));
}

#[test]
fn a_preview_refresh_does_not_read_the_setting_again() {
    let temp = TempDb::new();
    let mut shell = Shell::new();
    colorflag::load(&mut shell, &temp.db);
    store(&temp.db, false);

    tags::refresh_preview(&mut shell, &temp.db);

    assert_eq!(flags(&mut shell), (true, true));
}
