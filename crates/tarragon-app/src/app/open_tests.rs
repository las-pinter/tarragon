//! Tests that the app starts the editor for the image the gallery queued.
//! A shell script stands in for the editor, so no real editor starts.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::App as _;
use eframe::Frame;
use eframe::egui::Context;
use image::RgbaImage;

use crate::app::tests::{TempDir, new_dir};
use crate::app::*;
use crate::test_ui;

/// Room for the pipeline and the child to finish; each wait ends at this limit.
const LIMIT: Duration = Duration::from_secs(10);

/// Long enough for a wrongly started child to write its file.
const SETTLE: Duration = Duration::from_millis(500);

struct Rig {
    /// Held so the folder is removed when the rig ends.
    _dir: TempDir,
    app: App,
    ctx: Context,
    frame: Frame,
    out: PathBuf,
    image: PathBuf,
}

impl Rig {
    /// An app on a folder with one image, with the one image selected.
    fn new() -> Self {
        let dir = new_dir();
        let folder = dir.0.join("pics");
        std::fs::create_dir_all(&folder).expect("create the folder");
        let image = folder.join("one.png");
        RgbaImage::new(8, 8).save(&image).expect("write the image");
        let (_filter, control) = LogControl::new_filter();
        let startup = Startup {
            folder: Some(folder),
            close_when_loaded: false,
            exit_wait: ExitWait::default(),
            crash_dir: None,
        };
        let db_path = dir.0.join("t.db");
        let log = LogRing::new();
        let opened = App::open(&db_path, log, control, None, startup);
        let app = opened.expect("open the app");
        let ctx = Context::default();
        tarragon_ui::install(&ctx);
        tarragon_ui::apply_theme(&ctx);
        let out = dir.0.join("out.txt");
        let mut rig = Self {
            _dir: dir,
            app,
            ctx,
            frame: Frame::_new_kittest(),
            out,
            image,
        };
        rig.select_the_image();
        rig
    }

    /// Draw frames until the folder has loaded, then select its one image.
    fn select_the_image(&mut self) {
        let end = Instant::now() + LIMIT;
        loop {
            self.frame();
            self.app.shell.gallery_mut().select_all_visible();
            if self.app.shell.sole_selected().is_some() {
                break;
            }
            assert!(Instant::now() < end, "timed out: the image loads");
            std::thread::sleep(Duration::from_millis(10));
        }
        // The frames above may have queued nothing, but a stale request would
        // spoil the checks, so the queue starts empty.
        while self.app.shell.take_open_request().is_some() {}
    }

    /// Draw one headless frame.
    fn frame(&mut self) {
        let (app, frame) = (&mut self.app, &mut self.frame);
        test_ui::frame(&self.ctx, Vec::new(), &mut |ui| app.ui(ui, frame));
    }

    /// Store `command` as the template for every extension.
    fn set_template(&self, command: &str) {
        self.app
            .db
            .upsert_editor_association("*", command)
            .expect("store the template");
    }

    /// A template whose program appends its first argument to `out.txt`.
    fn writing_template(&self) -> String {
        // Only double quotes group a template part, so the script has none
        // inside.
        format!("sh -c \"echo $0 >> {}\" {{file}}", self.out.display())
    }

    /// Queue the selected image, as a double-click does.
    fn queue(&mut self) {
        self.app.shell.gallery_mut().open_selected();
    }
}

/// The text of `out`, once it holds `lines` lines, or a failure at [`LIMIT`].
fn wait_for_lines(out: &Path, lines: usize) -> String {
    let end = Instant::now() + LIMIT;
    loop {
        if let Ok(text) = std::fs::read_to_string(out)
            && text.lines().count() >= lines
        {
            return text;
        }
        assert!(Instant::now() < end, "timed out: the editor wrote the path");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_queued_request_starts_the_template_program_once_with_the_path() {
    let mut rig = Rig::new();
    rig.set_template(&rig.writing_template());
    rig.queue();
    rig.app.apply_open_request();
    let text = wait_for_lines(&rig.out, 1);
    assert_eq!(text, format!("{}\n", rig.image.display()));
    // The request is gone, so a second call starts nothing.
    rig.app.apply_open_request();
    std::thread::sleep(SETTLE);
    assert_eq!(std::fs::read_to_string(&rig.out).expect("read"), text);
}

#[test]
fn a_missing_program_does_not_panic_and_the_request_is_consumed() {
    let mut rig = Rig::new();
    rig.set_template("tarragon-no-such-editor-program {file}");
    rig.queue();
    rig.app.apply_open_request();
    // A working template now, so a request left over would start it.
    rig.set_template(&rig.writing_template());
    rig.app.apply_open_request();
    let end = Instant::now() + SETTLE;
    while Instant::now() < end {
        assert!(!rig.out.exists(), "the failed request came back");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_frame_opens_the_queued_image() {
    let mut rig = Rig::new();
    rig.set_template(&rig.writing_template());
    rig.queue();
    rig.frame();
    let text = wait_for_lines(&rig.out, 1);
    assert_eq!(text, format!("{}\n", rig.image.display()));
}

#[test]
fn an_empty_queue_starts_nothing() {
    let mut rig = Rig::new();
    rig.set_template(&rig.writing_template());
    rig.app.apply_open_request();
    std::thread::sleep(SETTLE);
    assert!(!rig.out.exists());
}
