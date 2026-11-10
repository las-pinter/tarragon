//! Tests that the app wires the filter bar to the database and the settings.

use std::path::PathBuf;

use eframe::App as _;
use eframe::Frame;
use eframe::egui::{Context, Ui};
use tarragon_core::entity::{FileInfo, TagSource};
use tarragon_db::Database;
use tarragon_db::repository::ThumbnailWrite;
use tarragon_db::settings::{SettingKey, SettingValue, Settings};
use tarragon_ui::gallery::Scope;
use tarragon_ui::grid::GridItem;

use super::tests::{TempDir, new_dir};
use super::*;
use crate::test_ui::{click_text, frame};

/// Open the app on `dir`, with no folder and no guard.
fn open(dir: &TempDir) -> App {
    let (_filter, control) = LogControl::new_filter();
    let startup = Startup {
        folder: None,
        close_when_loaded: false,
        exit_wait: ExitWait::default(),
        crash_dir: None,
    };
    let path = dir.0.join("tarragon.db");
    App::open(&path, LogRing::new(), control, None, startup).expect("open the app")
}

/// A context with fonts and theme.
fn context() -> Context {
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    ctx
}

/// Draw frames until the library load is over. The busy strip moves the
/// layout while it shows, and a click must not land in that shift.
fn settle(app: &mut App, ctx: &Context) {
    let mut eframe_frame = Frame::_new_kittest();
    let start = std::time::Instant::now();
    while !app.thumbs.is_idle() {
        frame(ctx, Vec::new(), &mut |ui| app.ui(ui, &mut eframe_frame));
        assert!(start.elapsed().as_secs() < 30, "the load hangs");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    frame(ctx, Vec::new(), &mut |ui| app.ui(ui, &mut eframe_frame));
}

#[test]
fn a_tag_click_in_the_app_filters_the_gallery() {
    let dir = new_dir();
    let db = Database::open(dir.0.join("tarragon.db")).expect("open");
    db.initialize().expect("initialize");
    let names = ["a", "b", "c"].map(|n| format!("/r/{n}.png"));
    let info = |p: &String| FileInfo {
        path: PathBuf::from(p),
        size: 1,
        mtime: 1,
        width: 0,
        height: 0,
    };
    db.write_scan_group(&names.iter().map(info).collect::<Vec<_>>())
        .expect("write");
    let tag = db.ensure_tag("night", TagSource::User).expect("tag");
    db.add_tags_to_files_batch(&names[..2], &[tag])
        .expect("attach");
    drop(db);

    let mut app = open(&dir);
    let gallery = app.shell.gallery_mut();
    let items = names.map(|p| GridItem::new(PathBuf::from(p), 1, None));
    gallery.set_items(items.into());
    gallery.set_scope(Scope::All);
    gallery.set_filter_lists(vec!["night".into()], Vec::new());
    let ctx = context();
    settle(&mut app, &ctx);
    let mut eframe_frame = Frame::_new_kittest();
    let mut run = |ui: &mut Ui| app.ui(ui, &mut eframe_frame);
    click_text(&ctx, "Filter Tags", &mut run);
    click_text(&ctx, "night", &mut run);
    frame(&ctx, Vec::new(), &mut run);
    assert_eq!(app.shell.gallery_mut().visible_len(), 2);
}

#[test]
fn the_stored_colour_setting_decides_the_swatch_row() {
    let dir = new_dir();
    let ctx = context();
    let mut eframe_frame = Frame::_new_kittest();
    let swatches = |dir: &TempDir, ctx: &Context, eframe_frame: &mut Frame| {
        let mut app = open(dir);
        frame(ctx, Vec::new(), &mut |ui| app.ui(ui, eframe_frame)).swatches
    };
    assert!(swatches(&dir, &ctx, &mut eframe_frame) > 0);
    let db = Database::open(dir.0.join("tarragon.db")).expect("open");
    let off = Settings::new(&db).set(SettingKey::ColorTagEnabled, SettingValue::Bool(false));
    off.expect("save the setting");
    drop(db);
    assert_eq!(swatches(&dir, &ctx, &mut eframe_frame), 0);
}

/// Seed two folders, a user tag, and the colour auto tags, then open the app.
fn seeded_app(dir: &TempDir) -> App {
    let db = Database::open(dir.0.join("tarragon.db")).expect("open");
    db.initialize().expect("initialize");
    let names = ["/r/one/a.png", "/r/two/b.png"].map(String::from);
    let files: Vec<_> = names
        .iter()
        .map(|p| FileInfo {
            path: PathBuf::from(p),
            size: 1,
            mtime: 1,
            width: 0,
            height: 0,
        })
        .collect();
    db.write_scan_group(&files).expect("write");
    for path in &names {
        let record = ThumbnailWrite {
            path: path.clone(),
            mtime: 1,
            size: 1,
            width: 1,
            height: 1,
            cache_uuid: "u".into(),
            thumbnail_cache_path: None,
            preview_cache_path: None,
            full_cache_path: None,
            last_accessed: 1,
        };
        db.upsert_thumbnail(&record).expect("thumbnail");
    }
    for (name, source) in [
        ("night", TagSource::User),
        ("red", TagSource::AutoColor),
        ("transparent", TagSource::AutoColor),
    ] {
        let tag = db.ensure_tag(name, source).expect("tag");
        db.add_tags_to_files_batch(&names, &[tag]).expect("attach");
    }
    drop(db);
    let mut app = open(dir);
    let gallery = app.shell.gallery_mut();
    let items = names.map(|p| GridItem::new(PathBuf::from(p), 1, None));
    gallery.set_items(items.into());
    gallery.set_scope(Scope::All);
    app
}

#[test]
fn the_tag_menu_lists_the_user_tags_and_no_colour_tags() {
    let dir = new_dir();
    let mut app = seeded_app(&dir);
    let ctx = context();
    settle(&mut app, &ctx);
    let mut eframe_frame = Frame::_new_kittest();
    let mut run = |ui: &mut Ui| app.ui(ui, &mut eframe_frame);
    let tags = click_text(&ctx, "Filter Tags", &mut run);
    assert!(tags.contains(&"night".to_owned()));
    for auto in ["red", "transparent"] {
        assert!(!tags.contains(&auto.to_owned()));
    }
}

#[test]
fn the_folder_menu_lists_the_seeded_folders() {
    let dir = new_dir();
    let mut app = seeded_app(&dir);
    let ctx = context();
    settle(&mut app, &ctx);
    let mut eframe_frame = Frame::_new_kittest();
    let mut run = |ui: &mut Ui| app.ui(ui, &mut eframe_frame);
    let folders = click_text(&ctx, "Filter Folders", &mut run);
    assert!(
        ["/r/one", "/r/two"]
            .iter()
            .all(|f| folders.contains(&(*f).to_owned()))
    );
}

#[test]
fn a_tag_added_after_startup_shows_when_the_menu_opens() {
    let dir = new_dir();
    let mut app = seeded_app(&dir);
    let db = Database::open(dir.0.join("tarragon.db")).expect("open");
    db.ensure_tag("dusk", TagSource::User).expect("tag");
    let ctx = context();
    settle(&mut app, &ctx);
    let mut eframe_frame = Frame::_new_kittest();
    let mut run = |ui: &mut Ui| app.ui(ui, &mut eframe_frame);
    assert!(click_text(&ctx, "Filter Tags", &mut run).contains(&"dusk".to_owned()));
}

#[test]
fn a_chip_whose_folder_left_the_database_goes_when_a_menu_opens() {
    let dir = new_dir();
    let mut app = seeded_app(&dir);
    let ctx = context();
    settle(&mut app, &ctx);
    let mut eframe_frame = Frame::_new_kittest();
    let mut run = |ui: &mut Ui| app.ui(ui, &mut eframe_frame);
    click_text(&ctx, "Filter Folders", &mut run);
    click_text(&ctx, "/r/two", &mut run);
    frame(&ctx, Vec::new(), &mut run);
    assert_eq!(app.shell.gallery_mut().visible_len(), 1);
    let db = Database::open(dir.0.join("tarragon.db")).expect("open");
    db.delete_thumbnails_by_folder("/r/two").expect("delete");
    let mut run = |ui: &mut Ui| app.ui(ui, &mut eframe_frame);
    click_text(&ctx, "Filter Tags", &mut run);
    assert_eq!(app.shell.gallery_mut().visible_len(), 2);
}
