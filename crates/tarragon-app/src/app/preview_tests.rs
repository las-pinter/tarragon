//! Tests that the app feeds the Preview with the details and pictures of the
//! selection. Every frame is headless.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::App as _;
use eframe::Frame;
use eframe::egui::{Context, Event, Modifiers, PointerButton, vec2};
use image::{Rgba, RgbaImage};
use tarragon_db::repository::ThumbnailWrite;
use tarragon_ui::gallery::Scope;
use tarragon_ui::grid::GridItem;

use super::tests::{TempDir, new_dir};
use super::*;
use crate::test_ui;

/// Room for the loader thread to finish; each wait ends at this limit.
const LIMIT: Duration = Duration::from_secs(10);

/// The Dimensions row the Preview shows for the stored rows.
const DIMENSIONS: &str = "64 x 48";

pub(super) struct Rig {
    pub(super) dir: TempDir,
    pub(super) app: App,
    pub(super) ctx: Context,
    frame: Frame,
}

impl Rig {
    /// An app over a database that holds a row for each of `names`. A name
    /// listed in `rendered` also gets its tier file.
    pub(super) fn new(names: &[&str], rendered: &[&str], cap: i64) -> Self {
        let dir = new_dir();
        let path = dir.0.join("tarragon.db");
        let db = Database::open(&path).expect("open");
        db.initialize().expect("initialize");
        Settings::new(&db)
            .set(SettingKey::MaxMultiPreview, SettingValue::Int(cap))
            .expect("set the cap");
        let rig_dir = dir.0.clone();
        for name in names {
            store(&db, &rig_dir, name, rendered.contains(name));
        }
        let (_filter, control) = LogControl::new_filter();
        let startup = Startup {
            folder: None,
            close_when_loaded: false,
            exit_wait: ExitWait::default(),
            crash_dir: None,
        };
        let mut app =
            App::open(&path, LogRing::new(), control, None, startup).expect("open the app");
        let items = names
            .iter()
            .map(|name| GridItem::new(rig_dir.join(name), 1, None))
            .collect();
        app.shell.gallery_mut().set_items(items);
        app.shell.gallery_mut().set_scope(Scope::All);
        // The files have no sources, so a library render would only fail.
        app.thumbs.skip_library();
        let ctx = Context::default();
        tarragon_ui::install(&ctx);
        tarragon_ui::apply_theme(&ctx);
        Self {
            dir,
            app,
            ctx,
            frame: Frame::_new_kittest(),
        }
    }

    /// Draw one frame with `events`. Returns the texts and the picture count.
    pub(super) fn frame(&mut self, events: Vec<Event>) -> (Vec<String>, usize) {
        let (app, frame) = (&mut self.app, &mut self.frame);
        let painted = test_ui::frame(&self.ctx, events, &mut |ui| app.ui(ui, frame));
        (
            painted.texts.into_iter().map(|t| t.0).collect(),
            painted.pictures,
        )
    }

    /// Draw one frame and return how many colour swatches it paints.
    pub(super) fn swatches(&mut self) -> usize {
        let (app, frame) = (&mut self.app, &mut self.frame);
        test_ui::frame(&self.ctx, Vec::new(), &mut |ui| app.ui(ui, frame)).swatches
    }

    /// Click the tile `label`, holding the control key when `add` is set.
    pub(super) fn click(&mut self, label: &str, add: bool) {
        // A click soon after another on the same tile is a double-click, which
        // opens the image. Jumping the clock keeps each click single.
        let later = self.ctx.input(|input| input.time) + 10.0;
        let first = test_ui::frame_at(&self.ctx, Some(later), Vec::new(), &mut |ui| {
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
        // egui reads the keys held from this event, not from the click.
        let held = Event::ModifiersChanged(modifiers);
        self.frame(vec![
            held,
            Event::PointerMoved(at),
            button(true),
            button(false),
        ]);
        self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)]);
    }

    /// Draw frames until the picture count is `want`, or fail after [`LIMIT`].
    pub(super) fn pictures_reach(&mut self, want: usize) {
        let end = Instant::now() + LIMIT;
        loop {
            let (_, pictures) = self.frame(Vec::new());
            if pictures == want {
                return;
            }
            assert!(Instant::now() < end, "pictures {pictures}, wanted {want}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn has_text(&mut self, text: &str) -> bool {
        self.frame(Vec::new()).0.iter().any(|t| t == text)
    }
}

/// Store the row of `name`, with tier files when `rendered`.
fn store(db: &Database, dir: &Path, name: &str, rendered: bool) {
    let tier = dir.join(format!("t-{name}"));
    if rendered {
        RgbaImage::from_pixel(64, 48, Rgba([200, 30, 30, 255]))
            .save_with_format(&tier, image::ImageFormat::Png)
            .expect("write the tier");
    }
    let record = ThumbnailWrite {
        path: dir.join(name).to_string_lossy().into_owned(),
        mtime: 1,
        size: 10,
        width: 64,
        height: 48,
        cache_uuid: "u".into(),
        thumbnail_cache_path: rendered.then(|| tier.to_string_lossy().into_owned()),
        preview_cache_path: None,
        full_cache_path: None,
        last_accessed: 1,
    };
    db.upsert_thumbnail(&record).expect("store the row");
}

fn path_of(rig: &Rig, name: &str) -> PathBuf {
    rig.dir.0.join(name)
}

#[test]
fn a_selected_rendered_file_fills_the_details_and_the_picture() {
    let mut rig = Rig::new(&["a.png"], &["a.png"], 9);
    rig.click("a.png", false);
    assert_eq!(rig.app.shell.selected_paths(), [path_of(&rig, "a.png")]);
    rig.pictures_reach(1);
    assert!(rig.has_text(DIMENSIONS));
}

#[test]
fn an_empty_selection_clears_the_details_and_the_picture() {
    let mut rig = Rig::new(&["a.png"], &["a.png"], 9);
    rig.click("a.png", false);
    rig.pictures_reach(1);
    rig.click("a.png", true);
    assert_eq!(rig.app.shell.selected_paths().len(), 0);
    rig.pictures_reach(0);
    assert!(!rig.has_text(DIMENSIONS));
}

#[test]
fn a_multi_selection_shows_at_most_the_cap_pictures() {
    let mut rig = Rig::new(
        &["a.png", "b.png", "c.png"],
        &["a.png", "b.png", "c.png"],
        2,
    );
    rig.click("a.png", false);
    rig.click("b.png", true);
    rig.click("c.png", true);
    assert_eq!(rig.app.preview_total, 3);
    rig.pictures_reach(2);
}

#[test]
fn feeding_the_same_selection_twice_holds_one_texture() {
    let mut rig = Rig::new(&["a.png"], &["a.png"], 9);
    rig.click("a.png", false);
    rig.app.feed_preview();
    rig.pictures_reach(1);
    let own = Context::default().tex_manager().read().num_allocated();
    assert_eq!(rig.ctx.tex_manager().read().num_allocated() - own, 1);
}

#[test]
fn a_file_without_a_cache_row_shows_details_until_the_store_fires() {
    let mut rig = Rig::new(&["a.png"], &[], 9);
    rig.click("a.png", false);
    rig.pictures_reach(0);
    assert!(rig.has_text(DIMENSIONS));
    let db = Database::open(rig.dir.0.join("tarragon.db")).expect("open");
    store(&db, &rig.dir.0.clone(), "a.png", true);
    rig.app.watch_store(&rig.ctx.clone(), 1, Instant::now());
    rig.pictures_reach(1);
}

#[test]
fn dropping_the_app_with_a_request_in_flight_ends() {
    let mut rig = Rig::new(&["a.png", "b.png"], &["a.png", "b.png"], 9);
    rig.click("a.png", false);
    rig.click("b.png", true);
    let (done, ended) = std::sync::mpsc::channel();
    let app = rig.app;
    std::thread::spawn(move || {
        drop(app);
        let _ = done.send(());
    });
    ended
        .recv_timeout(LIMIT)
        .expect("the drop joined the loader");
}
