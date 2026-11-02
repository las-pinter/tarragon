//! Tests that the All Images scope loads the stored library.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::App as _;
use eframe::Frame;
use eframe::egui::{Context, Ui};
use image::RgbaImage;
use tarragon_core::entity::TagSource;
use tarragon_db::Database;
use tarragon_ui::gallery::Scope;

use super::tests::{TempDir, new_dir};
use super::*;
use crate::test_ui::{click_text, frame};

/// An app with a context, and the folders `a` (two files) and `b` (one file).
/// Folder `b` was opened first, then `a`, so `a` is shown and both are stored.
struct Rig {
    dir: TempDir,
    app: App,
    ctx: Context,
    frame: Frame,
    a: PathBuf,
    b: PathBuf,
}

impl Rig {
    fn new() -> Self {
        let dir = new_dir();
        let make = |name: &str, files: &[&str]| {
            let folder = sidebar::normalize(&dir.0.join(name));
            std::fs::create_dir_all(&folder).expect("make the folder");
            for file in files {
                RgbaImage::new(8, 8)
                    .save(folder.join(file))
                    .expect("write a png");
            }
            folder
        };
        let a = make("a", &["a1.png", "a2.png"]);
        let b = make("b", &["b1.png"]);
        let (_filter, control) = LogControl::new_filter();
        let startup = Startup {
            folder: None,
            close_when_loaded: false,
            exit_wait: ExitWait::default(),
        };
        let path = dir.0.join("tarragon.db");
        let app = App::open(&path, LogRing::new(), control, None, startup).expect("open the app");
        let ctx = Context::default();
        tarragon_ui::install(&ctx);
        tarragon_ui::apply_theme(&ctx);
        let mut rig = Self {
            dir,
            app,
            ctx,
            frame: Frame::_new_kittest(),
            a,
            b,
        };
        for folder in [rig.b.clone(), rig.a.clone()] {
            rig.app.navigate(&folder);
            rig.run_until(|app| app.thumbs.is_idle());
        }
        rig
    }

    /// Draw frames until `done` holds.
    fn run_until(&mut self, done: impl Fn(&App) -> bool) {
        let start = Instant::now();
        loop {
            let mut run = |ui: &mut Ui| self.app.ui(ui, &mut self.frame);
            frame(&self.ctx, Vec::new(), &mut run);
            if done(&self.app) {
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(30), "the load hangs");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Switch the scope and wait until the library is in the gallery.
    fn show_all(&mut self) {
        self.app.shell.gallery_mut().set_scope(Scope::All);
        self.run_until(|app| app.thumbs.library_loaded() && app.thumbs.is_idle());
    }

    fn visible(&mut self) -> usize {
        self.app.shell.gallery_mut().visible_len()
    }

    /// The texts of one more frame.
    fn texts(&mut self) -> Vec<String> {
        let mut run = |ui: &mut Ui| self.app.ui(ui, &mut self.frame);
        let painted = frame(&self.ctx, Vec::new(), &mut run);
        painted.texts.into_iter().map(|t| t.0).collect()
    }

    /// Tag a file with a user tag from outside the app.
    fn tag(&self, file: &std::path::Path, name: &str) {
        let db = Database::open(self.dir.0.join("tarragon.db")).expect("open");
        let tag = db.ensure_tag(name, TagSource::User).expect("tag");
        let path = file.to_string_lossy().into_owned();
        db.add_tags_to_files_batch(&[path], &[tag]).expect("attach");
    }
}

#[test]
fn all_images_shows_the_files_of_both_folders() {
    let mut rig = Rig::new();
    assert_eq!(rig.visible(), 2);
    rig.show_all();
    assert_eq!(rig.visible(), 3);
    assert!(rig.app.shell.gallery_mut().has_item(&rig.b.join("b1.png")));
}

#[test]
fn the_info_bar_count_follows_the_library() {
    let mut rig = Rig::new();
    rig.show_all();
    assert!(rig.texts().contains(&"All Images - 3 files".to_owned()));
}

#[test]
fn a_tag_on_a_file_of_another_folder_filters_all_images_to_it() {
    let mut rig = Rig::new();
    rig.tag(&rig.b.join("b1.png"), "night");
    rig.show_all();
    assert!(!rig.texts().iter().any(|t| t.contains("filter")));
    click_text(&rig.ctx.clone(), "Filter Tags", &mut |ui| {
        rig.app.ui(ui, &mut rig.frame);
    });
    let ctx = rig.ctx.clone();
    click_text(&ctx, "night", &mut |ui| rig.app.ui(ui, &mut rig.frame));
    assert_eq!(rig.visible(), 1);
    let texts = rig.texts();
    assert!(texts.contains(&"All Images - 1 file".to_owned()));
    assert!(texts.contains(&"1 filter active".to_owned()));
}

#[test]
fn folder_scope_hides_the_other_folders_after_a_library_load() {
    let mut rig = Rig::new();
    rig.show_all();
    rig.app.shell.gallery_mut().set_scope(Scope::Folder);
    rig.run_until(|app| app.thumbs.is_idle());
    assert_eq!(rig.visible(), 2);
    // No reload happened: the library items are still there, only hidden.
    assert!(rig.app.thumbs.library_loaded());
    assert!(rig.app.shell.gallery_mut().has_item(&rig.b.join("b1.png")));
}

#[test]
fn a_folder_open_in_all_images_keeps_the_whole_library() {
    let mut rig = Rig::new();
    rig.show_all();
    let b = rig.b.clone();
    rig.app.navigate(&b);
    rig.run_until(|app| app.thumbs.is_idle());
    rig.run_until(|app| app.thumbs.library_loaded() && app.thumbs.is_idle());
    let gallery = rig.app.shell.gallery_mut();
    assert_eq!(gallery.scope(), Scope::All);
    assert_eq!(gallery.folder(), Some(b.as_path()));
    assert_eq!(gallery.visible_len(), 3);
}
