//! Tests that the busy flag follows the pipeline into the gallery.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::App as _;
use eframe::Frame;
use eframe::egui::{Context, Ui};
use image::RgbaImage;

use crate::app::tests::{TempDir, new_dir};
use crate::app::{App, ExitWait, Startup};
use crate::logging::LogControl;
use crate::logging::ring::LogRing;
use crate::sidebar;
use crate::test_ui::frame;

/// The strip text of the gallery.
const STRIP: &str = "Working...";

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

    /// The texts of one more frame.
    fn texts(&mut self) -> Vec<String> {
        let mut run = |ui: &mut Ui| self.app.ui(ui, &mut self.frame);
        let painted = frame(&self.ctx, Vec::new(), &mut run);
        painted.texts.into_iter().map(|t| t.0).collect()
    }

    /// Whether one more frame shows the strip.
    fn strip(&mut self) -> bool {
        self.texts().iter().any(|text| text == STRIP)
    }

    /// Draw frames until the pipeline is idle. The flag must hold in every
    /// frame before that, so no gap shows between two requests.
    fn settle(&mut self) {
        let start = Instant::now();
        while !self.app.thumbs.is_idle() {
            assert!(self.app.thumbs.is_busy(), "busy until the pipeline is idle");
            self.texts();
            assert!(start.elapsed() < Duration::from_secs(30), "the load hangs");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn an_open_is_busy_from_the_request_to_the_end() {
    let mut rig = Rig::new();
    assert!(!rig.app.thumbs.is_busy());
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    assert!(rig.app.thumbs.is_busy());
    assert!(
        rig.strip(),
        "the first frame after the request shows the strip"
    );
    rig.settle();
    assert!(!rig.app.thumbs.is_busy());
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
    assert!(rig.app.thumbs.is_busy());
    assert!(rig.strip());
    rig.settle();
    assert!(!rig.app.thumbs.is_busy());
    assert!(!rig.strip());
}

#[test]
fn a_purge_asked_during_an_open_keeps_the_flag_until_both_end() {
    let mut rig = Rig::new();
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    assert!(rig.app.thumbs.is_busy());
    let root = rig.app.data_root.clone();
    rig.app.thumbs.purge(root);
    assert!(rig.app.thumbs.is_busy());
    assert!(rig.strip());
    // `settle` asserts the flag in each frame, so the open ending and the
    // purge starting leave no gap.
    rig.settle();
    assert!(!rig.app.thumbs.is_busy());
    assert!(!rig.strip());
}

#[test]
fn a_failed_open_ends_with_the_flag_false() {
    let mut rig = Rig::new();
    let missing = sidebar::normalize(&rig.folder.join("gone"));
    rig.app.navigate(&missing);
    assert!(rig.app.thumbs.is_busy());
    assert!(rig.strip());
    rig.settle();
    assert!(!rig.app.thumbs.is_busy());
    assert!(!rig.strip());
}

/// Whether one frame shows an info bar text that starts with `name`.
fn info_starts_with(texts: &[String], name: &str) -> bool {
    texts.iter().any(|t| t.starts_with(&format!("{name} - ")))
}

#[test]
fn the_first_open_names_the_folder_in_the_info_bar_before_it_ends() {
    let mut rig = Rig::new();
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    let texts = rig.texts();
    assert!(rig.app.thumbs.is_busy(), "the open still runs");
    assert!(info_starts_with(&texts, "pics"), "{texts:?}");
    assert!(!texts.iter().any(|t| t.starts_with("No folder")));
}

#[test]
fn a_failed_open_leaves_the_info_bar_on_the_old_folder() {
    let mut rig = Rig::new();
    let folder = rig.folder.clone();
    rig.app.navigate(&folder);
    rig.settle();
    rig.app.navigate(Path::new("/nonexistent/x"));
    let during = rig.texts();
    assert!(info_starts_with(&during, "x"), "{during:?}");
    rig.settle();
    let after = rig.texts();
    assert!(info_starts_with(&after, "pics"), "{after:?}");
    assert!(!info_starts_with(&after, "x"));
}

#[test]
fn the_startup_eviction_is_not_busy() {
    let mut rig = Rig::open();
    assert!(!rig.app.thumbs.is_idle(), "the eviction waits or runs");
    assert!(!rig.app.thumbs.is_busy());
    assert!(!rig.strip(), "no strip while the eviction runs");
}
