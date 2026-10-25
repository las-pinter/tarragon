//! Binary entry point for the tarragon image browser.

mod data_dir;

use std::path::PathBuf;
use std::process::ExitCode;

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
    /// Returns the settings error when the window state cannot be read.
    fn open(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Database::open(path)?;
        db.initialize()?;
        let settings = Settings::new(&db);
        let (window_state, repaired) = settings.window_state_repaired()?.unwrap_or_default();
        if repaired {
            eprintln!("tarragon: the stored window state was invalid and was repaired");
        }
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

fn main() -> ExitCode {
    let db_path = match data_dir::db_path(std::env::var_os("TARRAGON_DB")) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("tarragon: {error}");
            return ExitCode::FAILURE;
        }
    };

    match App::open(&db_path) {
        Ok(mut app) => {
            // No event loop runs yet, so this one call stands in for the
            // per-frame persist.
            if let Err(error) = app.persist_sort_if_changed() {
                eprintln!("tarragon: cannot save the window state: {error}");
            }
            println!("tarragon {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("tarragon: {error}");
            ExitCode::FAILURE
        }
    }
}
