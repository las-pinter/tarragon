//! Binary entry point for the tarragon image browser.

use std::path::PathBuf;

use tarragon_db::Database;
use tarragon_db::settings::{Settings, WindowState};
use tarragon_ui::Shell;

/// The running application: one database, one window state, one shell.
struct App {
    /// The open database.
    db: Database,
    /// The retained window state, including the sort mode.
    window_state: WindowState,
    /// The window frame and its gallery.
    shell: Shell,
}

impl App {
    /// Open the database at `path`, restore the window state, and build the
    /// shell with the stored sort mode.
    ///
    /// # Errors
    ///
    /// Returns the database error when the file cannot open or initialize.
    fn open(path: &PathBuf) -> Result<Self, tarragon_db::DbError> {
        let db = Database::open(path)?;
        db.initialize()?;
        let settings = Settings::new(&db);
        let window_state = settings.window_state_value()?.unwrap_or_default();
        let shell = Shell::new().with_sort_mode(window_state.sort_mode());
        Ok(Self {
            db,
            window_state,
            shell,
        })
    }

    /// Persist a sort change to the window state, if one happened.
    ///
    /// # Errors
    ///
    /// Returns the settings error when the write fails.
    fn persist_sort_if_changed(&mut self) -> Result<(), tarragon_db::settings::SettingsError> {
        if let Some(mode) = self.shell.take_sort_changed() {
            self.window_state.set_sort_mode(mode);
            let settings = Settings::new(&self.db);
            settings.set_window_state_value(&self.window_state)?;
        }
        Ok(())
    }
}

fn main() {
    let db_path =
        std::env::var("TARRAGON_DB").map_or_else(|_| PathBuf::from("tarragon.db"), PathBuf::from);

    match App::open(&db_path) {
        Ok(mut app) => {
            // The event loop calls `persist_sort_if_changed` after each frame.
            let _ = app.persist_sort_if_changed();
            println!("tarragon {}", env!("CARGO_PKG_VERSION"));
        }
        Err(error) => {
            eprintln!("tarragon: {error}");
        }
    }
}
