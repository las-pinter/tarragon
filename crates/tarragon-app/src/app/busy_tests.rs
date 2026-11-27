//! Tests that the open and the purge show in the status bar from the request
//! to the end.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::App as _;
use eframe::Frame;
use eframe::egui::{Context, Pos2, Ui};
use image::RgbaImage;

use crate::app::tests::{TempDir, new_dir};
use crate::app::{App, ExitWait, Startup};
use crate::logging::LogControl;
use crate::logging::ring::LogRing;
use crate::sidebar;
use crate::test_ui::frame;
use tarragon_ui::status::Job;

/// An app with a window context and one folder of two files.
struct Rig {
    _dir: TempDir,
    app: App,
    ctx: Context,
    frame: Frame,
    folder: PathBuf,
}

impl Rig {
    /// A rig that has run its startup eviction to the end.
    fn new() -> Self {
        let mut rig = Self::open();
        let start = Instant::now();
        while !rig.app.thumbs.is_idle() {
            rig.texts();
            assert!(start.elapsed() < Duration::from_secs(30), "the start hangs");
            std::thread::sleep(Duration::from_millis(10));
        }
        rig
    }

    /// A rig right after `App::open`, with the startup eviction not done.
    fn open() -> Self {
        let dir = new_dir();
        let folder = sidebar::normalize(&dir.0.join("pics"));
        std::fs::create_dir_all(&folder).expect("make the folder");
        for name in ["a.png", "b.png"] {
            RgbaImage::new(8, 8)
                .save(folder.join(name))
                .expect("write a png");
        }
        let (_filter, control) = LogControl::new_filter();
        let startup = Startup {
            folder: None,
            close_when_loaded: false,
            exit_wait: ExitWait::default(),
            crash_dir: None,
            perf_log: None,
            frame_run: false,
        };
        let path = dir.0.join("tarragon.db");
        let app = App::open(&path, LogRing::new(), control, None, startup).expect("open the app");
        let ctx = Context::default();
        tarragon_ui::install(&ctx);
        tarragon_ui::apply_theme(&ctx);
        Self {
            _dir: dir,
            app,
            ctx,
            frame: Frame::_new_kittest(),
            folder,
        }
    }

    /// The texts of one more frame, with where they were painted.
    fn painted_texts(&mut self) -> Vec<(String, Pos2)> {
        let mut run = |ui: &mut Ui| self.app.ui(ui, &mut self.frame);
        frame(&self.ctx, Vec::new(), &mut run).texts
    }

    /// The texts of one more frame.
    fn texts(&mut self) -> Vec<String> {
        self.painted_texts().into_iter().map(|t| t.0).collect()
    }

    /// Whether an open or a purge is among the jobs.
    fn busy(&self) -> bool {
        self.app
            .thumbs
            .jobs()
            .iter()
            .any(|job| matches!(job, Job::Scan { .. } | Job::Library | Job::Purge { .. }))
    }

    /// Whether one more frame shows an open or a purge in the status bar.
    fn strip(&mut self) -> bool {
        self.texts().iter().any(|text| {
            text.starts_with("Scanning ")
                || text.starts_with("Purging cache")
                || text == "Loading library"
        })
    }

    /// Draw frames until the pipeline is idle. The flag must hold in every
    /// frame before that, so no gap shows between two requests.
    fn settle(&mut self) {
        let start = Instant::now();
        while !self.app.thumbs.is_idle() {
            assert!(self.busy(), "a job shows until the pipeline is idle");
            self.texts();
            assert!(start.elapsed() < Duration::from_secs(30), "the load hangs");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn an_open_is_busy_from_the_request_to_the_end() {
    let mut rig = Rig::new();
    assert!(!rig.busy());
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    assert!(rig.busy());
    assert!(
        rig.strip(),
        "the first frame after the request shows the strip"
    );
    rig.settle();
    assert!(!rig.busy());
    assert!(!rig.strip(), "the strip ends with the open");
}

#[test]
fn a_purge_is_busy_from_the_call_to_the_end_of_the_work() {
    let mut rig = Rig::new();
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    rig.settle();
    let root = rig.app.data_root.clone();
    rig.app.thumbs.purge(root);
    assert!(rig.busy());
    assert!(rig.strip());
    rig.settle();
    assert!(!rig.busy());
    assert!(!rig.strip());
}

#[test]
fn a_purge_asked_during_an_open_keeps_the_flag_until_both_end() {
    let mut rig = Rig::new();
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    assert!(rig.busy());
    let root = rig.app.data_root.clone();
    rig.app.thumbs.purge(root);
    assert!(rig.busy());
    assert!(rig.strip());
    // `settle` asserts the flag in each frame, so the open ending and the
    // purge starting leave no gap.
    rig.settle();
    assert!(!rig.busy());
    assert!(!rig.strip());
}

#[test]
fn a_failed_open_ends_with_the_flag_false() {
    let mut rig = Rig::new();
    let missing = sidebar::normalize(&rig.folder.join("gone"));
    rig.app.navigate(&missing);
    assert!(rig.busy());
    assert!(rig.strip());
    rig.settle();
    assert!(!rig.busy());
    assert!(!rig.strip());
}

/// Whether one frame names the folder in the gallery header.
///
/// The name must sit right of the GALLERY tab, so a sidebar row of the same
/// name cannot satisfy it.
fn header_names(painted: &[(String, Pos2)], name: &str) -> bool {
    let Some(gallery) = painted
        .iter()
        .find(|(text, _)| *text == tarragon_ui::shell::DOCK_GALLERY.to_uppercase())
        .map(|(_, pos)| pos.x)
    else {
        return false;
    };
    painted
        .iter()
        .any(|(text, pos)| text == name && pos.x > gallery)
}

#[test]
fn the_first_open_names_the_folder_in_the_header_before_it_ends() {
    let mut rig = Rig::new();
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    let texts = rig.painted_texts();
    assert!(rig.busy(), "the open still runs");
    assert!(header_names(&texts, "pics"), "{texts:?}");
    assert!(!header_names(&texts, "No folder"));
}

#[test]
fn a_failed_open_leaves_the_header_on_the_old_folder() {
    let mut rig = Rig::new();
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    rig.settle();
    rig.app.navigate(Path::new("/nonexistent/x"));
    let during = rig.painted_texts();
    assert!(header_names(&during, "x"), "{during:?}");
    rig.settle();
    let after = rig.painted_texts();
    assert!(header_names(&after, "pics"), "{after:?}");
    assert!(!header_names(&after, "x"));
}

#[test]
fn the_startup_eviction_is_not_busy() {
    let mut rig = Rig::open();
    assert!(!rig.app.thumbs.is_idle(), "the eviction waits or runs");
    assert!(!rig.busy());
    assert!(!rig.strip(), "no strip while the eviction runs");
}

#[test]
fn the_thumbnail_and_color_jobs_end_when_every_tile_settles() {
    let mut rig = Rig::new();
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    rig.settle();
    let start = Instant::now();
    loop {
        let texts = rig.texts();
        let counting = texts
            .iter()
            .any(|t| t.starts_with("Rendering thumbnails") || t.starts_with("Indexing colors"));
        if !counting {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "stuck: {texts:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(rig.app.thumbs.jobs(), []);
}
