//! Tests that the app wires the filter bar to the database and the settings.

use std::path::PathBuf;

use eframe::App as _;
use eframe::Frame;
use eframe::egui::{Context, Ui};
use tarragon_core::entity::{FileInfo, TagSource};
use tarragon_db::Database;
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
