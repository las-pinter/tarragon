//! The running application and its eframe frame loop.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use eframe::egui::{Context, Event, Ui, ViewportCommand};
use tarragon_cache::store::CacheStore;
use tarragon_db::Database;
use tarragon_db::settings::{SettingKey, SettingValue, Settings, SettingsError, WindowState};
use tarragon_ui::Shell;
use tarragon_ui::shell::{LogPanelEvents, LogView};

use crate::data_dir;
use crate::instance::Guard;
use crate::logging::LogControl;
use crate::logging::ring::LogRing;
use crate::logview::LogCache;
use crate::pipeline::{Config, Pipeline, Repaint};
use crate::thumbs::{ExitWait, Thumbs};
use crate::window::{self, Geometry};

/// How often the open Log panel looks for new lines. The ring cannot wake the
/// UI, and egui otherwise repaints only on input.
const LOG_REFRESH: Duration = Duration::from_millis(250);

/// What the command line asked for.
pub struct Startup {
    /// The folder to open, or none for an empty window.
    pub folder: Option<PathBuf>,
    /// Close the window once every tile has a texture or an error. `main` sets
    /// it from the `TARRAGON_CLOSE_WHEN_LOADED` environment variable, which the
    /// smoke test uses.
    pub close_when_loaded: bool,
    /// Lets `main` wait for the pipeline after the window closes.
    pub exit_wait: ExitWait,
}

/// The running application: one database, one window state, one shell.
pub struct App {
    /// The open database. The pipeline threads share it.
    db: Arc<Database>,
    /// The retained window state, including the sort mode.
    window_state: WindowState,
    /// The window frame and its gallery.
    shell: Shell,
    /// The in-memory log lines.
    ring: LogRing,
    /// The level switch.
    control: LogControl,
    /// The UI-side copy of the log lines.
    logs: LogCache,
    /// The Debug checkbox state.
    debug: bool,
    /// The last known window rectangle.
    geometry: Option<Geometry>,
    /// Follows the window rectangle across frames.
    tracker: window::Tracker,
    /// The single-instance guard. It stays alive as long as the app.
    guard: Option<Arc<Guard>>,
    /// The folder pipeline, when a folder was given.
    thumbs: Option<Thumbs>,
    /// The window context for worker threads, set once the window exists.
    repaint_ctx: Arc<OnceLock<Context>>,
    /// Close the window when the grid is done.
    close_when_loaded: bool,
    /// Lets `main` wait for the pipeline after the window closes.
    exit_wait: ExitWait,
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
    pub fn open(
        path: &Path,
        ring: LogRing,
        control: LogControl,
        guard: Option<Arc<Guard>>,
        startup: Startup,
    ) -> Result<Self, Box<dyn Error>> {
        let db = Database::open(path)?;
        db.initialize()?;
        let db = Arc::new(db);
        let settings = Settings::new(&db);
        let mut debug = false;
        match settings.get(SettingKey::DebugMode) {
            Ok(SettingValue::Bool(value)) => debug = value,
            Ok(_) => {}
            Err(error) => tracing::warn!("cannot read the debug setting: {error}"),
        }
        control.set_debug(debug);
        let (window_state, repaired) = settings.window_state_repaired()?.unwrap_or_default();
        if repaired {
            tracing::warn!("the stored window state was invalid and was repaired");
        }
        let shell = Shell::new().with_sort_mode(window_state.sort_mode());
        let repaint_ctx = Arc::new(OnceLock::new());
        let thumbs = match startup.folder {
            Some(folder) => Some(start_thumbs(path, &db, folder, &repaint_ctx)?),
            None => None,
        };
        Ok(Self {
            geometry: window_state.geometry,
            tracker: window::Tracker::new(window_state.geometry),
            db,
            window_state,
            shell,
            ring,
            control,
            logs: LogCache::default(),
            debug,
            guard,
            thumbs,
            repaint_ctx,
            close_when_loaded: startup.close_when_loaded,
            exit_wait: startup.exit_wait,
        })
    }

    /// The stored window rectangle, if any.
    pub fn geometry(&self) -> Option<Geometry> {
        self.geometry
    }

    /// Make an activate request repaint the window.
    ///
    /// The waker holds only the context, never the guard, so the guard can
    /// drop on any thread.
    pub fn install_waker(&self, ctx: &Context) {
        // A second call keeps the first context.
        let _ = self.repaint_ctx.set(ctx.clone());
        if let Some(guard) = &self.guard {
            let ctx = ctx.clone();
            guard.set_waker(move || ctx.request_repaint());
        }
    }

    /// Persist a sort change to the window state, if one happened.
    ///
    /// # Errors
    ///
    /// Returns the settings error when the write fails.
    fn persist_sort_if_changed(&mut self) -> Result<(), SettingsError> {
        if let Some(mode) = self.shell.take_sort_changed() {
            self.window_state.set_sort_mode(mode);
            let settings = Settings::new(&self.db);
            settings.set_window_state_value(&self.window_state)?;
        }
        Ok(())
    }

    /// Act on what the user did in the Log panel.
    fn apply_log_events(&mut self, events: LogPanelEvents) {
        if events.clear {
            self.ring.clear();
            self.logs.clear();
        }
        if let Some(debug) = events.debug {
            self.debug = debug;
            self.control.set_debug(debug);
            let settings = Settings::new(&self.db);
            if let Err(error) = settings.set(SettingKey::DebugMode, SettingValue::Bool(debug)) {
                tracing::warn!("cannot save the debug setting: {error}");
            }
        }
    }

    /// Write the window rectangle and sort mode to the database.
    fn save_window_state(&mut self) {
        self.window_state.geometry = self.geometry;
        let settings = Settings::new(&self.db);
        if let Err(error) = settings.set_window_state_value(&self.window_state) {
            tracing::warn!("cannot save the window state: {error}");
        }
    }
}

/// Build the pipeline for `folder`. The cache sits where the `cache_dir`
/// setting says, else beside the database.
fn start_thumbs(
    db_path: &Path,
    db: &Arc<Database>,
    folder: PathBuf,
    repaint_ctx: &Arc<OnceLock<Context>>,
) -> Result<Thumbs, Box<dyn Error>> {
    let setting = Settings::new(db)
        .get(SettingKey::CacheDir)
        .unwrap_or_else(|error| {
            tracing::warn!("cannot read the cache folder setting: {error}");
            SettingValue::None
        });
    let cache = CacheStore::new(data_dir::cache_root(db_path, &setting));
    let ctx = Arc::clone(repaint_ctx);
    // Before the window exists there is nothing to repaint.
    let repaint: Repaint = Arc::new(move || {
        if let Some(ctx) = ctx.get() {
            ctx.request_repaint();
        }
    });
    let (pipeline, results) =
        Pipeline::new(Config::new(cache), Arc::clone(db), Arc::clone(&repaint))?;
    Ok(Thumbs::new(pipeline, results, folder, repaint))
}

/// Raise the window when a second start asked for it.
fn raise_if_requested(ctx: &Context, guard: Option<&Guard>) {
    let requested = guard.is_some_and(Guard::take_activate_request);
    for command in window::activate_commands(requested) {
        ctx.send_viewport_cmd(command);
    }
}

impl eframe::App for App {
    // A minimized or hidden window runs no `ui`, but eframe still calls this,
    // so the activate request is taken here and only here.
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        raise_if_requested(ctx, self.guard.as_deref());
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let old = self.geometry;
        self.geometry = ctx.input(|input| {
            let acted = input.events.iter().any(|event| {
                matches!(
                    event,
                    Event::Key { pressed: true, .. }
                        | Event::Text(_)
                        | Event::PointerButton { pressed: true, .. }
                        | Event::MouseWheel { .. }
                        | Event::Touch { .. }
                )
            });
            let now = Duration::from_secs_f64(input.time.max(0.0));
            self.tracker.track(input.viewport(), old, now, acted)
        });
        if self.tracker.settling() {
            ctx.request_repaint_after(window::SETTLE_REPAINT);
        }

        // Pull before the draw and hold no lock during it, so a line logged
        // mid-frame cannot deadlock.
        if self.shell.log_visible() {
            self.logs.pull(&self.ring);
            ctx.request_repaint_after(LOG_REFRESH);
        }
        if let Some(thumbs) = &mut self.thumbs {
            thumbs.before_draw(&ctx, self.shell.gallery_mut());
        }
        let source = self.logs.source();
        let view = LogView {
            source: &source,
            debug: self.debug,
        };
        let events = self.shell.show_with_log(ui, &view);
        self.apply_log_events(events);
        if let Some(thumbs) = &mut self.thumbs {
            thumbs.after_draw(self.shell.gallery_mut());
            if self.close_when_loaded
                && let Some(summary) = thumbs.loaded_once()
            {
                tracing::info!("{summary}");
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }

        if let Err(error) = self.persist_sort_if_changed() {
            tracing::error!("cannot save the window state: {error}");
        }
    }

    // The argument exists because the eframe `glow` feature is on.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.geometry = self.tracker.at_exit(self.geometry);
        tracing::debug!("window geometry to save: {:?}", self.geometry);
        self.save_window_state();
        if let Some(thumbs) = &mut self.thumbs {
            thumbs.shutdown(&self.exit_wait);
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::{RawInput, ViewportCommand, ViewportId};
    use eframe::{App as _, Frame};

    use super::*;
    use crate::instance::{self, Outcome};
    use crate::logging::LogControl;

    /// A temporary folder that removes itself on drop, also after a failed assert.
    struct TempDir(std::path::PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// An app that owns the guard, and a second start that already asked for the window.
    fn app_with_pending_request(dir: &TempDir) -> App {
        let db_path = dir.0.join("tarragon.db");
        let guard = match instance::acquire_under(&db_path, &dir.0) {
            Outcome::Primary(guard) => guard,
            Outcome::Forwarded => panic!("expected a primary guard, got Forwarded"),
            Outcome::Unguarded(reason) => panic!("expected a primary guard: {reason}"),
        };
        let (_filter, control) = LogControl::new_filter();
        let app = App::open(
            &db_path,
            LogRing::new(),
            control,
            Some(Arc::new(guard)),
            Startup {
                folder: None,
                close_when_loaded: false,
                exit_wait: ExitWait::default(),
            },
        )
        .expect("open the app");
        assert!(matches!(
            instance::acquire_under(&db_path, &dir.0),
            Outcome::Forwarded
        ));
        app
    }

    fn new_dir() -> TempDir {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-app-raise-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        TempDir(dir)
    }

    #[test]
    fn logic_pass_sends_the_raise_commands_after_an_activate_request() {
        let dir = new_dir();
        let mut app = app_with_pending_request(&dir);
        let ctx = Context::default();
        let mut frame = Frame::_new_kittest();

        let output = ctx.run_logic(&RawInput::default(), |c| app.logic(c, &mut frame));
        let commands = output
            .viewport_commands
            .get(&ViewportId::ROOT)
            .expect("root commands");
        assert_eq!(commands.len(), 4, "{commands:?}");
        assert!(matches!(commands[0], ViewportCommand::Minimized(false)));
        assert!(matches!(commands[1], ViewportCommand::Visible(true)));
        assert!(matches!(commands[2], ViewportCommand::Focus));
        assert!(matches!(
            commands[3],
            ViewportCommand::RequestUserAttention(_)
        ));

        let again = ctx.run_logic(&RawInput::default(), |c| app.logic(c, &mut frame));
        assert!(again.viewport_commands.is_empty(), "the flag is taken once");
    }

    #[test]
    fn ui_pass_does_not_take_the_activate_request() {
        let dir = new_dir();
        let mut app = app_with_pending_request(&dir);
        let ctx = Context::default();
        tarragon_ui::install(&ctx);
        let mut frame = Frame::_new_kittest();

        let mut output = ctx.run_ui(RawInput::default(), |ui| app.ui(ui, &mut frame));
        output.textures_delta.clear();
        let commands = &output.viewport_output[&ViewportId::ROOT].commands;
        let raised = commands.iter().any(|command| {
            matches!(
                command,
                ViewportCommand::Minimized(_)
                    | ViewportCommand::Visible(_)
                    | ViewportCommand::Focus
                    | ViewportCommand::RequestUserAttention(_)
            )
        });
        assert!(!raised, "{commands:?}");
        let guard = app.guard.as_ref().expect("guard");
        assert!(guard.take_activate_request(), "the request is still there");
    }
}
