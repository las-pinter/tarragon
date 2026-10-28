use eframe::egui::{RawInput, ViewportCommand, ViewportId};
use std::time::Instant;

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
    let dir = std::env::temp_dir().join(format!("tarragon-app-raise-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    TempDir(dir)
}

#[test]
fn a_reorder_event_persists_across_a_restart() {
    use tarragon_ui::sidebar::panel::SidebarEvent;

    let dir = new_dir();
    let db_path = dir.0.join("tarragon.db");
    let open = || {
        let (_filter, control) = LogControl::new_filter();
        let startup = Startup {
            folder: None,
            close_when_loaded: false,
            exit_wait: ExitWait::default(),
        };
        App::open(&db_path, LogRing::new(), control, None, startup).expect("open the app")
    };
    let paths: Vec<PathBuf> = ["a", "b", "c"].map(|n| dir.0.join(n)).to_vec();
    let mut app = open();
    for (order, path) in paths.iter().enumerate() {
        let order = i64::try_from(order).unwrap();
        let stored = app.db.add_favorite(&path.to_string_lossy(), None, order);
        stored.expect("add a favorite");
    }
    let wanted = vec![paths[2].clone(), paths[0].clone(), paths[1].clone()];
    let events = vec![SidebarEvent::Reorder(wanted.clone())];
    sidebar::apply(&mut app.shell, &app.db, events);
    drop(app);

    let mut app = open();
    let shown: Vec<PathBuf> = app
        .shell
        .sidebar_mut()
        .favorites()
        .iter()
        .map(|f| f.path.clone())
        .collect();
    assert_eq!(shown, wanted);
}

/// Open an app on a fresh database in `dir` with the given start folder.
fn app_at(dir: &TempDir, folder: Option<PathBuf>) -> App {
    let (_filter, control) = LogControl::new_filter();
    let startup = Startup {
        folder,
        close_when_loaded: false,
        exit_wait: ExitWait::default(),
    };
    App::open(&dir.0.join("t.db"), LogRing::new(), control, None, startup).expect("open")
}

#[test]
fn an_app_without_a_folder_opens_and_runs_a_frame() {
    let dir = new_dir();
    let mut app = app_at(&dir, None);
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    let mut frame = Frame::_new_kittest();
    let mut output = ctx.run_ui(RawInput::default(), |ui| app.ui(ui, &mut frame));
    output.textures_delta.clear();
    assert_eq!(app.shell.gallery_mut().folder(), None);
}

#[test]
fn a_dot_dot_folder_reaches_thumbs_and_sidebar_as_the_same_path() {
    let dir = new_dir();
    for name in ["a", "b"] {
        std::fs::create_dir_all(dir.0.join(name)).expect("create a folder");
    }
    let raw = dir.0.join("a").join("..").join("b");
    let wanted = dir.0.join("b");
    let mut app = app_at(&dir, Some(raw));
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    let mut frame = Frame::_new_kittest();
    let started = Instant::now();
    while app.shell.gallery_mut().folder().is_none() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the open never finished"
        );
        let mut output = ctx.run_ui(RawInput::default(), |ui| app.ui(ui, &mut frame));
        output.textures_delta.clear();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.shell.gallery_mut().folder(), Some(wanted.as_path()));
    assert_eq!(
        app.shell.sidebar_mut().open_folder(),
        Some(wanted.as_path())
    );
}

/// Run frames until no open runs or waits.
fn run_until_idle(app: &mut App, ctx: &Context, frame: &mut Frame) {
    let started = Instant::now();
    loop {
        let mut output = ctx.run_ui(RawInput::default(), |ui| app.ui(ui, frame));
        output.textures_delta.clear();
        if app.thumbs.is_idle() {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the open hangs"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn make_pngs(folder: &Path, names: &[&str]) {
    std::fs::create_dir_all(folder).expect("create a folder");
    for name in names {
        image::RgbaImage::new(8, 8)
            .save(folder.join(name))
            .expect("write a png");
    }
}

#[test]
fn a_navigate_event_opens_a_folder_in_an_app_without_one() {
    use tarragon_ui::sidebar::panel::SidebarEvent;

    let dir = new_dir();
    let pics = dir.0.join("pics");
    make_pngs(&pics, &["a.png", "b.png"]);
    let mut app = app_at(&dir, None);
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    let mut frame = Frame::_new_kittest();
    // A `..` step must not reach the gallery. `ui` does the rest.
    let raw = dir.0.join("x").join("..").join("pics");
    app.shell
        .sidebar_mut()
        .push_event(SidebarEvent::Navigate(raw));
    run_until_idle(&mut app, &ctx, &mut frame);
    assert_eq!(app.shell.gallery_mut().visible_len(), 2);
    assert_eq!(app.shell.gallery_mut().folder(), Some(pics.as_path()));
    assert_eq!(app.shell.sidebar_mut().open_folder(), Some(pics.as_path()));
}

#[test]
fn navigate_then_add_current_shows_the_new_favorite() {
    use tarragon_ui::sidebar::panel::SidebarEvent;

    let dir = new_dir();
    let pics = dir.0.join("pics");
    make_pngs(&pics, &["a.png"]);
    let mut app = app_at(&dir, None);
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    let mut frame = Frame::_new_kittest();
    app.shell
        .sidebar_mut()
        .push_event(SidebarEvent::Navigate(pics.clone()));
    run_until_idle(&mut app, &ctx, &mut frame);
    app.shell.sidebar_mut().push_event(SidebarEvent::AddCurrent);
    let mut output = ctx.run_ui(RawInput::default(), |ui| app.ui(ui, &mut frame));
    output.textures_delta.clear();
    let shown: Vec<PathBuf> = app
        .shell
        .sidebar_mut()
        .favorites()
        .iter()
        .map(|f| f.path.clone())
        .collect();
    assert_eq!(shown, std::slice::from_ref(&pics));
    assert_eq!(app.db.list_favorites().unwrap().len(), 1);
    assert_eq!(
        app.shell.sidebar_mut().selected_favorite(),
        Some(pics.as_path())
    );
}

#[test]
fn a_failed_navigate_keeps_the_old_folder_and_restores_the_highlight() {
    use tarragon_ui::sidebar::panel::SidebarEvent;

    let dir = new_dir();
    let pics = dir.0.join("pics");
    make_pngs(&pics, &["a.png", "b.png"]);
    let mut app = app_at(&dir, Some(pics.clone()));
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    let mut frame = Frame::_new_kittest();
    run_until_idle(&mut app, &ctx, &mut frame);
    // A click moves the highlight before the open answers.
    let gone = dir.0.join("gone");
    app.shell.sidebar_mut().set_open_folder(Some(gone.clone()));
    app.shell
        .sidebar_mut()
        .push_event(SidebarEvent::Navigate(gone));
    run_until_idle(&mut app, &ctx, &mut frame);
    assert_eq!(app.shell.gallery_mut().folder(), Some(pics.as_path()));
    assert_eq!(app.shell.gallery_mut().visible_len(), 2);
    assert_eq!(app.shell.sidebar_mut().open_folder(), Some(pics.as_path()));
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

/// A layout text with Library hidden and Log shown.
fn changed_layout() -> String {
    use tarragon_ui::layout::{Pane, default_dock, set_visible, to_json};

    let mut dock = default_dock();
    set_visible(&mut dock, Pane::Library, false);
    set_visible(&mut dock, Pane::Log, true);
    to_json(&dock)
}

/// Store `layout` in the window state of a fresh database, then open an app on it.
fn app_with_stored_layout(dir: &TempDir, layout: &str) -> App {
    let mut app = app_at(dir, None);
    app.window_state.layout = Some(layout.to_owned());
    Settings::new(&app.db)
        .set_window_state_value(&app.window_state)
        .expect("store the window state");
    drop(app);
    app_at(dir, None)
}

#[test]
fn a_stored_layout_restores_into_the_shell() {
    let dir = new_dir();
    let app = app_with_stored_layout(&dir, &changed_layout());
    assert!(!app.shell.library_visible());
    assert!(app.shell.log_visible());
}

#[test]
fn a_corrupt_stored_layout_gives_the_first_dock() {
    let dir = new_dir();
    let app = app_with_stored_layout(&dir, "{not a layout");
    let first = tarragon_ui::layout::to_json(&tarragon_ui::layout::first_dock());
    assert_eq!(app.shell.layout_json(), first);
    assert!(app.shell.library_visible());
    assert!(!app.shell.log_visible());
}

#[test]
fn the_saved_state_carries_the_dock_and_a_restart_restores_it() {
    let dir = new_dir();
    let mut app = app_at(&dir, None);
    assert!(app.shell.toggle_log());
    app.save_window_state();
    assert_eq!(
        app.window_state.layout.as_deref(),
        Some(app.shell.layout_json().as_str())
    );
    drop(app);

    let mut app = app_at(&dir, None);
    assert!(app.shell.log_visible());
    app.shell.reset_layout();
    app.save_window_state();
    drop(app);

    let app = app_at(&dir, None);
    assert!(!app.shell.log_visible());
    assert!(app.shell.library_visible());
}
