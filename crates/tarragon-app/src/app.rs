//! The running application and its eframe frame loop.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui::{Context, TextureHandle, Ui, ViewportCommand};
use tarragon_cache::store::CacheStore;
use tarragon_db::Database;
use tarragon_db::settings::{SettingKey, SettingValue, Settings, SettingsError, WindowState};
use tarragon_ui::Shell;
use tarragon_ui::gallery::Scope;
use tarragon_ui::shell::{LogPanelEvents, LogView};

use crate::crashcard;
use crate::data_dir;
use crate::filtering;
use crate::instance::Guard;
use crate::logging::LogControl;
use crate::logging::ring::LogRing;
use crate::logview::LogCache;
use crate::pipeline::{Config, Pipeline, Repaint};
use crate::preview_feed::PreviewFeed;
use crate::preview_loader::PreviewLoader;
use crate::preview_source;
use crate::sidebar;
use crate::storewatch::StoreWatch;
use crate::tags;
use crate::thumbs::{ExitClear, ExitWait, Thumbs};
use crate::window;

/// How often the open Log panel looks for new lines. The ring cannot wake the
/// UI, and egui otherwise repaints only on input.
const LOG_REFRESH: Duration = Duration::from_millis(250);

/// The mosaic size when the `max_multi_preview` setting cannot be read.
const DEFAULT_MULTI_PREVIEW: usize = 9;

/// The cache cap in GiB when the `cache_size_cap_gb` setting cannot be read.
const DEFAULT_CACHE_CAP_GB: i64 = 20;

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
    /// The folder of the crash reports, or none when it is unknown.
    pub crash_dir: Option<PathBuf>,
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
    /// The single-instance guard. It stays alive as long as the app.
    guard: Option<Arc<Guard>>,
    /// The folder pipeline. It idles until a folder is opened.
    thumbs: Thumbs,
    /// The folder of the database file. No cache clear may delete it.
    data_root: PathBuf,
    /// The folder of the thumbnail cache.
    cache_root: PathBuf,
    /// The window context for worker threads, set once the window exists.
    repaint_ctx: Arc<OnceLock<Context>>,
    /// Close the window when the grid is done.
    close_when_loaded: bool,
    /// Lets `main` wait for the pipeline after the window closes.
    exit_wait: ExitWait,
    /// The last selection revision the Preview tags were read for.
    seen_selection: u64,
    /// Limits how often a changed store epoch makes the app read again.
    store_watch: StoreWatch,
    /// The Preview textures and the plan for the selection.
    preview_feed: PreviewFeed<TextureHandle>,
    /// The thread that decodes Preview pictures. Dropping the app joins it.
    preview_loader: PreviewLoader,
    /// How many pictures the Preview mosaic shows at most, read once at start.
    max_multi_preview: usize,
    /// How many files the last Preview read selected.
    preview_total: usize,
    /// The folder of the crash reports.
    crash_dir: Option<PathBuf>,
    /// The report the crash card shows, until the user dismisses it.
    crash_path: Option<PathBuf>,
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
        let mut shell = Shell::new().with_sort_mode(window_state.sort_mode());
        filtering::init(shell.gallery_mut(), &db);
        // Log only the size: the stored text is not useful in a log line.
        if let Some(text) = &window_state.layout
            && !shell.restore_layout(text)
        {
            tracing::warn!(
                "the stored dock layout ({} bytes) is invalid and was ignored",
                text.len()
            );
        }
        // Normalize once, so the pipeline and the sidebar see the same path.
        let folder = startup.folder.as_deref().map(sidebar::normalize);
        let home = sidebar::home_dir();
        sidebar::init(&mut shell, &db, home.as_deref(), folder.as_deref());
        let repaint_ctx = Arc::new(OnceLock::new());
        let (mut thumbs, cache_root) = start_thumbs(path, &db, folder, &repaint_ctx)?;
        // Queued before any folder open, so the open sees the evicted cache.
        thumbs.evict(cache_cap_gb(&db));
        let preview_loader = PreviewLoader::new(repaint_of(&repaint_ctx))?;
        let max_multi_preview = max_multi_preview(&db);
        shell.preview_mut().set_max_multi_preview(max_multi_preview);
        let crash_path = startup
            .crash_dir
            .as_deref()
            .and_then(crashcard::newest_report)
            .map(|(path, report)| {
                shell.show_crash_report(report);
                path
            });
        Ok(Self {
            db,
            window_state,
            shell,
            ring,
            control,
            logs: LogCache::default(),
            debug,
            guard,
            thumbs,
            data_root: data_dir::data_root(path),
            cache_root,
            repaint_ctx,
            close_when_loaded: startup.close_when_loaded,
            exit_wait: startup.exit_wait,
            seen_selection: 0,
            store_watch: StoreWatch::default(),
            preview_feed: PreviewFeed::new(),
            preview_loader,
            max_multi_preview,
            preview_total: 0,
            crash_dir: startup.crash_dir,
            crash_path,
        })
    }

    /// Whether the window was maximized at the last exit. A fresh state reads
    /// as not maximized.
    pub fn maximized(&self) -> bool {
        self.window_state.maximized.unwrap_or(false)
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

    /// Open another folder. The path is normalized here, once, so the
    /// pipeline, the gallery, and the sidebar see the same path.
    fn navigate(&mut self, path: &Path) {
        self.thumbs.navigate(sidebar::normalize(path));
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

    /// Read the Preview and re-run an active filter when the writer committed
    /// since the last read, at most once per [`crate::storewatch::INTERVAL`].
    ///
    /// A change held back by the throttle schedules a repaint, so the poll
    /// runs again without input.
    fn watch_store(&mut self, ctx: &Context, epoch: u64, now: Instant) {
        if self.store_watch.poll(epoch, now) {
            tags::refresh_preview(&mut self.shell, &self.db);
            self.feed_preview();
            // An empty filter returns early, so no query runs without one.
            self.shell.gallery_mut().refresh_filter();
            ctx.request_repaint();
        } else if let Some(left) = self.store_watch.wait(epoch, now) {
            ctx.request_repaint_after(left);
        }
    }

    /// Read the selection from the database and plan its Preview pictures.
    /// An empty selection clears the details and the pictures.
    fn feed_preview(&mut self) {
        let selected = self.shell.selected_paths();
        if selected.is_empty() {
            self.preview_total = 0;
            self.shell.preview_mut().set_meta(None);
            self.preview_feed.forget_all();
            return;
        }
        let source = preview_source::read(&self.db, &selected, self.max_multi_preview);
        self.preview_total = source.total;
        self.shell.preview_mut().set_meta(source.meta.clone());
        self.preview_feed.select(&source, &self.preview_loader);
    }

    /// The `full` tier clear for the exit, when the setting asks for it now.
    /// An unreadable setting clears nothing, because a delete needs a clear yes.
    fn exit_clear(&self) -> Option<ExitClear> {
        clear_flag(Settings::new(&self.db).get(SettingKey::ClearFullResOnExit)).then(|| ExitClear {
            cache_root: self.cache_root.clone(),
            data_root: self.data_root.clone(),
        })
    }

    /// Write the maximized flag, dock layout, and sort mode to the database.
    fn save_window_state(&mut self) {
        self.window_state.layout = Some(self.shell.layout_json());
        let settings = Settings::new(&self.db);
        if let Err(error) = settings.set_window_state_value(&self.window_state) {
            tracing::warn!("cannot save the window state: {error}");
        }
    }
}

/// Build the pipeline, and open `folder` on the first frame when there is one.
/// The cache sits where the `cache_dir` setting says, else beside the
/// database. The cache folder comes back with the pipeline.
fn start_thumbs(
    db_path: &Path,
    db: &Arc<Database>,
    folder: Option<PathBuf>,
    repaint_ctx: &Arc<OnceLock<Context>>,
) -> Result<(Thumbs, PathBuf), Box<dyn Error>> {
    let setting = Settings::new(db)
        .get(SettingKey::CacheDir)
        .unwrap_or_else(|error| {
            tracing::warn!("cannot read the cache folder setting: {error}");
            SettingValue::None
        });
    let cache_root = data_dir::cache_root(db_path, &setting);
    let cache = CacheStore::new(cache_root.clone());
    let repaint = repaint_of(repaint_ctx);
    let (pipeline, results) =
        Pipeline::new(Config::new(cache), Arc::clone(db), Arc::clone(&repaint))?;
    Ok((Thumbs::new(pipeline, results, folder, repaint), cache_root))
}

/// The `cache_size_cap_gb` setting. A failed read logs and gives the default.
fn cache_cap_gb(db: &Database) -> i64 {
    cap_or_default(Settings::new(db).get(SettingKey::CacheSizeCapGb))
}

/// The cap in GiB from a setting read. Any other value or a failed read gives
/// the default.
fn cap_or_default<E: std::fmt::Display>(read: Result<SettingValue, E>) -> i64 {
    match read {
        Ok(SettingValue::Int(cap)) => cap,
        Ok(_) => DEFAULT_CACHE_CAP_GB,
        Err(error) => {
            tracing::warn!("cannot read the cache size cap: {error}");
            DEFAULT_CACHE_CAP_GB
        }
    }
}

/// Whether a setting read says to clear the `full` tier. Only a clear true
/// does; any other value or a failed read clears nothing.
fn clear_flag<E: std::fmt::Display>(read: Result<SettingValue, E>) -> bool {
    match read {
        Ok(SettingValue::Bool(on)) => on,
        Ok(_) => false,
        Err(error) => {
            tracing::warn!("cannot read the clear on exit setting: {error}");
            false
        }
    }
}

/// A callback that repaints the window once it exists.
fn repaint_of(repaint_ctx: &Arc<OnceLock<Context>>) -> Repaint {
    let ctx = Arc::clone(repaint_ctx);
    // Before the window exists there is nothing to repaint.
    Arc::new(move || {
        if let Some(ctx) = ctx.get() {
            ctx.request_repaint();
        }
    })
}

/// The `max_multi_preview` setting. A failed read logs and gives the default.
fn max_multi_preview(db: &Database) -> usize {
    match Settings::new(db).get(SettingKey::MaxMultiPreview) {
        Ok(SettingValue::Int(cap)) => usize::try_from(cap).unwrap_or(DEFAULT_MULTI_PREVIEW),
        Ok(_) => DEFAULT_MULTI_PREVIEW,
        Err(error) => {
            tracing::warn!("cannot read the multi preview setting: {error}");
            DEFAULT_MULTI_PREVIEW
        }
    }
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
        self.window_state.maximized = ctx
            .input(|input| window::maximized_from(input.viewport(), self.window_state.maximized));

        // Pull before the draw and hold no lock during it, so a line logged
        // mid-frame cannot deadlock.
        if self.shell.log_visible() {
            self.logs.pull(&self.ring);
            ctx.request_repaint_after(LOG_REFRESH);
        }
        self.thumbs.before_draw(&ctx, self.shell.gallery_mut());
        // An open ended: the scope and the highlight follow the shown folder.
        if let Some(folder) = self.thumbs.take_folder_change() {
            self.shell.set_folder(Some(folder));
        }
        self.show_failure();
        let source = self.logs.source();
        let view = LogView {
            source: &source,
            debug: self.debug,
        };
        let events = self.shell.show_with_log(ui, &view);
        self.apply_crash_events();
        let revision = self.shell.selection_revision();
        if tags::selection_changed(&mut self.seen_selection, revision) {
            tags::refresh_preview(&mut self.shell, &self.db);
            self.feed_preview();
            self.show_failure();
            // The Preview was drawn before the refresh; egui repaints only on input.
            ctx.request_repaint();
        }
        let preview = self.shell.preview_mut();
        let total = self.preview_total;
        if self
            .preview_feed
            .poll(&ctx, &self.preview_loader, preview, total)
        {
            ctx.request_repaint();
        }
        // The folder open replaced the items, so a library reload follows it.
        if self.shell.gallery_mut().scope() == Scope::All {
            self.thumbs.show_library();
        }
        self.apply_log_events(events);
        let handled = sidebar::handle_events(&mut self.shell, &self.db);
        if let Some(path) = handled.navigate {
            self.navigate(&path);
        }
        if handled.any {
            ctx.request_repaint();
        }
        let events = self.shell.take_preview_events();
        self.apply_card_events(&events);
        if tags::apply_preview_events(&mut self.shell, &self.db, events) {
            ctx.request_repaint();
        }
        if tags::apply_manager_events(&mut self.shell, &self.db) {
            ctx.request_repaint();
        }
        self.watch_store(&ctx, self.thumbs.store_epoch(), Instant::now());
        if filtering::update(self.shell.gallery_mut(), &self.db) {
            ctx.request_repaint();
        }
        self.thumbs.after_draw(self.shell.gallery_mut());
        if self.close_when_loaded
            && let Some(summary) = self.thumbs.loaded_once()
        {
            tracing::info!("{summary}");
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        if let Err(error) = self.persist_sort_if_changed() {
            tracing::error!("cannot save the window state: {error}");
        }
    }

    // The argument exists because the eframe `glow` feature is on.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_window_state();
        let exit = self.exit_clear();
        self.thumbs.shutdown(&self.exit_wait, exit);
    }
}

mod card;
mod crash;
#[cfg(test)]
mod crash_tests;
#[cfg(test)]
mod exit_tests;
#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod filter_tests;
#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod tests;
