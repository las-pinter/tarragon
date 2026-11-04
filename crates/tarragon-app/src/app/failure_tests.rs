//! Tests that the app shows the error card for a file that failed to render
//! and acts on its buttons. Every frame is headless.

use std::time::{Duration, Instant};

use eframe::App as _;
use eframe::Frame;
use eframe::egui::{Context, Event, Modifiers, PointerButton, vec2};
use image::RgbaImage;

use super::tests::{TempDir, new_dir};
use super::*;
use crate::test_ui;

/// Room for the pipeline threads to finish; each wait ends at this limit.
const LIMIT: Duration = Duration::from_secs(10);

/// The title line of the card.
const TITLE: &str = "Preview failed";

struct Rig {
    dir: TempDir,
    app: App,
    ctx: Context,
    frame: Frame,
    bad: PathBuf,
}

impl Rig {
    /// An app on a folder with a corrupt `bad.png` and a good `a_good.png`,
    /// after the corrupt file has failed.
    fn new() -> Self {
        let dir = new_dir();
        let folder = dir.0.join("pics");
        std::fs::create_dir_all(&folder).expect("create the folder");
        RgbaImage::new(8, 8)
            .save(folder.join("a_good.png"))
            .expect("write the good file");
        std::fs::write(folder.join("bad.png"), b"not a png").expect("write the bad file");
        let (_filter, control) = LogControl::new_filter();
        let startup = Startup {
            folder: Some(folder),
            close_when_loaded: false,
            exit_wait: ExitWait::default(),
            crash_dir: None,
        };
        let app = App::open(&dir.0.join("t.db"), LogRing::new(), control, None, startup)
            .expect("open the app");
        let ctx = Context::default();
        tarragon_ui::install(&ctx);
        tarragon_ui::apply_theme(&ctx);
        let bad = dir.0.join("pics").join("bad.png");
        let mut rig = Self {
            dir,
            app,
            ctx,
            frame: Frame::_new_kittest(),
            bad,
        };
        rig.until("the bad file fails", |rig| {
            let bad = rig.bad.clone();
            rig.app.thumbs.failure(&bad).is_some()
        });
        rig
    }

    /// Draw one frame with `events`. Returns the texts and the picture count.
    fn frame(&mut self, events: Vec<Event>) -> (Vec<String>, usize) {
        let (app, frame) = (&mut self.app, &mut self.frame);
        let painted = test_ui::frame(&self.ctx, events, &mut |ui| app.ui(ui, frame));
        (
            painted.texts.into_iter().map(|t| t.0).collect(),
            painted.pictures,
        )
    }

    /// Draw frames until `done` holds, or fail after [`LIMIT`].
    fn until(&mut self, what: &str, done: impl Fn(&mut Self) -> bool) {
        let end = Instant::now() + LIMIT;
        while !done(self) {
            assert!(Instant::now() < end, "timed out: {what}");
            self.frame(Vec::new());
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Click the tile `label`, holding the control key when `add` is set.
    fn click(&mut self, label: &str, add: bool) {
        let first = self.frame(Vec::new());
        assert!(first.0.iter().any(|t| t == label), "{label} is not drawn");
        let first = test_ui::frame(&self.ctx, Vec::new(), &mut |ui| {
            self.app.ui(ui, &mut self.frame);
        });
        let at = first
            .texts
            .iter()
            .filter(|t| t.0 == label)
            .min_by(|a, b| a.1.y.total_cmp(&b.1.y))
            .expect(label)
            .1
            + vec2(2.0, 2.0);
        let modifiers = if add {
            Modifiers::CTRL
        } else {
            Modifiers::NONE
        };
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        let held = Event::ModifiersChanged(modifiers);
        self.frame(vec![
            held,
            Event::PointerMoved(at),
            button(true),
            button(false),
        ]);
        self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)]);
    }

    /// Click the card button `label`.
    fn press(&mut self, label: &str) {
        let (app, frame) = (&mut self.app, &mut self.frame);
        test_ui::click_text(&self.ctx, label, &mut |ui| app.ui(ui, frame));
    }

    /// The render error text of the bad file.
    fn message(&self) -> String {
        self.app.thumbs.failure(&self.bad).expect("failed").message
    }

    fn card_shown(&mut self) -> bool {
        self.frame(Vec::new()).0.iter().any(|t| t == TITLE)
    }
}

#[test]
fn selecting_the_failed_file_shows_the_card_and_a_good_file_clears_it() {
    let mut rig = Rig::new();
    let message = rig.message();
    assert_ne!(message, "", "the render error has a text");
    assert!(!rig.card_shown(), "nothing is selected yet");
    rig.click("bad.png", false);
    let texts = rig.frame(Vec::new()).0;
    assert!(texts.contains(&message), "the card holds the render error");
    assert!(texts.iter().any(|t| t == TITLE));

    rig.click("a_good.png", false);
    assert!(!rig.card_shown());
}

#[test]
fn a_multi_selection_with_a_failed_file_shows_no_card() {
    let mut rig = Rig::new();
    rig.click("bad.png", false);
    assert!(rig.card_shown());
    rig.click("a_good.png", true);
    assert_eq!(rig.app.shell.selected_paths().len(), 2);
    assert!(!rig.card_shown());
}

#[test]
fn a_retry_that_works_removes_the_card_and_gives_a_texture() {
    let mut rig = Rig::new();
    rig.click("bad.png", false);
    assert!(rig.card_shown());
    // The next render of the file works, as a fixed file would.
    RgbaImage::new(9, 9)
        .save(&rig.bad)
        .expect("fix the bad file");
    rig.press("Retry");
    rig.until("the card goes", |rig| !rig.card_shown());
    assert!(rig.app.thumbs.failure(&rig.bad).is_none());
    // The good tile has a texture; the fixed tile makes a second one.
    rig.until("the fixed tile has a texture", |rig| {
        rig.frame(Vec::new()).1 >= 2
    });
    assert_eq!(rig.app.shell.selected_paths(), [rig.bad.clone()]);
}

#[test]
fn a_retry_that_works_shows_the_size_of_the_new_file() {
    let mut rig = Rig::new();
    rig.click("bad.png", false);
    assert!(rig.card_shown());
    RgbaImage::new(30, 20)
        .save(&rig.bad)
        .expect("fix the bad file");
    let new_size = std::fs::metadata(&rig.bad).expect("stat").len();
    // Both files are under 1 KiB, so the row reads as whole bytes.
    assert!(new_size > 9 && new_size < 1024, "{new_size}");
    let want = format!("{new_size} B");
    let old = "9 B".to_owned();
    rig.press("Retry");
    rig.until("the size row shows the new size", |rig| {
        rig.frame(Vec::new()).0.contains(&want)
    });
    assert!(!rig.frame(Vec::new()).0.contains(&old));
}

#[cfg(unix)]
#[test]
fn open_in_editor_runs_the_template_and_keeps_the_selection() {
    let mut rig = Rig::new();
    let out = rig.dir.0.join("opened.txt");
    let template = format!("sh -c \"echo $0 > {}\" {{file}}", out.display());
    rig.app
        .db
        .upsert_editor_association("*", &template)
        .expect("store the template");
    rig.click("bad.png", false);
    let selected = rig.app.shell.selected_paths();
    assert_eq!(selected.len(), 1);
    rig.press("Open in editor");
    let want = format!("{}\n", selected[0].display());
    rig.until("the editor wrote the path", |_| {
        std::fs::read_to_string(&out).is_ok_and(|text| text == want)
    });
    assert_eq!(rig.app.shell.selected_paths(), selected);
}
