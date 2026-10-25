//! Binary entry point for the tarragon image browser.

mod data_dir;
mod instance;
mod logging;

use std::path::PathBuf;
use std::process::ExitCode;

use tarragon_db::Database;
use tarragon_db::settings::{SettingKey, SettingValue, Settings, WindowState};
use tarragon_ui::Shell;

use logging::LogControl;

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
    /// Open the database at `path`, apply the stored debug setting to the log
    /// level, restore the window state, and build the shell with the stored
    /// sort mode.
    ///
    /// # Errors
    ///
    /// Returns the database error when the file cannot open or initialize.
    /// Returns the settings error when the window state cannot be read.
    fn open(path: &PathBuf, log: &LogControl) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Database::open(path)?;
        db.initialize()?;
        let settings = Settings::new(&db);
        match settings.get(SettingKey::DebugMode) {
            Ok(SettingValue::Bool(debug)) => log.set_debug(debug),
            Ok(_) => {}
            Err(error) => tracing::warn!("cannot read the debug setting: {error}"),
        }
        let (window_state, repaired) = settings.window_state_repaired()?.unwrap_or_default();
        if repaired {
            tracing::warn!("the stored window state was invalid and was repaired");
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
    // The offset lookup must run before any other thread starts.
    let offset = logging::local_offset();
    let override_db = std::env::var_os("TARRAGON_DB");
    let db_path = match data_dir::db_path(override_db.clone()) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("tarragon: {error}");
            return ExitCode::FAILURE;
        }
    };

    // Logging is best effort: a missing location only means no file log.
    let location = data_dir::log_location(override_db);
    let logging = logging::init(location.as_ref().ok().map(|l| l.log_file.as_path()), offset);
    match &location {
        Ok(found) => logging::install_panic_hook(found.crash_dir.clone()),
        Err(error) => tracing::warn!("no log file or crash reports: {error}"),
    }

    // A second start must end here, before it opens the database.
    let _instance = match instance::acquire(&db_path) {
        instance::Outcome::Primary(guard) => Some(guard),
        instance::Outcome::Forwarded => {
            tracing::info!("another instance is running; asked it to show its window");
            return ExitCode::SUCCESS;
        }
        instance::Outcome::Unguarded(reason) => {
            tracing::warn!("starting without a single-instance guard: {reason}");
            None
        }
    };

    match App::open(&db_path, &logging.control) {
        Ok(mut app) => {
            // No event loop runs yet, so this one call stands in for the
            // per-frame persist.
            if let Err(error) = app.persist_sort_if_changed() {
                tracing::error!("cannot save the window state: {error}");
            }
            println!("tarragon {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(error) => {
            if logging.installed {
                tracing::error!("cannot start: {error}");
            } else {
                // No logger is running, so stderr is the only way to tell
                // the user.
                eprintln!("tarragon: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
