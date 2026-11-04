//! Tests that a changed store epoch makes the app read the Preview again.

use std::path::PathBuf;
use std::time::Instant;

use eframe::App as _;
use eframe::Frame;
use eframe::egui::{Color32, Context};
use tarragon_core::color::ColorBucket;
use tarragon_core::entity::{FileInfo, TagSource};
use tarragon_db::Database;
use tarragon_ui::gallery::Scope;
use tarragon_ui::grid::GridItem;
use tarragon_ui::preview::{ColorState, Meta};
use tarragon_ui::swatch;

use super::tests::new_dir;
use super::*;
use crate::test_ui::click_text;

const FILE: &str = "/r/one/a.png";

/// The fill of each Preview colour square one app frame paints.
fn square_fills(ctx: &Context, app: &mut App, frame: &mut Frame) -> Vec<Color32> {
    crate::test_ui::square_fills(ctx, &mut |ui| app.ui(ui, frame))
}

#[test]
fn a_changed_epoch_reads_the_colours_of_the_selected_file_again() {
    let dir = new_dir();
    let path = dir.0.join("tarragon.db");
    let db = Database::open(&path).expect("open");
    db.initialize().expect("initialize");
    let info = FileInfo {
        path: PathBuf::from(FILE),
        size: 1,
        mtime: 1,
        width: 0,
        height: 0,
    };
    db.write_scan_group(&[info]).expect("write");
    let (_filter, control) = LogControl::new_filter();
    let startup = Startup {
        folder: None,
        close_when_loaded: false,
        exit_wait: ExitWait::default(),
        crash_dir: None,
    };
    let mut app = App::open(&path, LogRing::new(), control, None, startup).expect("open the app");
    app.shell.preview_mut().set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    let gallery = app.shell.gallery_mut();
    gallery.set_items([GridItem::new(PathBuf::from(FILE), 1, None)].into());
    gallery.set_scope(Scope::All);
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut frame = Frame::_new_kittest();
    click_text(&ctx, "a.png", &mut |ui| app.ui(ui, &mut frame));
    assert_eq!(app.shell.selected_paths(), [PathBuf::from(FILE)]);
    let none = swatch::bucket_color(ColorBucket::Red).gamma_multiply(ColorState::None.opacity());
    assert_eq!(square_fills(&ctx, &mut app, &mut frame)[0], none);

    // The writer stores the colour after the selection was read.
    let red = db.ensure_tag("red", TagSource::AutoColor).expect("tag");
    db.add_tags_to_files_batch(&[FILE.to_owned()], &[red])
        .expect("attach");
    assert_eq!(square_fills(&ctx, &mut app, &mut frame)[0], none);
    app.watch_store(&ctx, 1, Instant::now());
    assert_eq!(
        square_fills(&ctx, &mut app, &mut frame)[0],
        swatch::bucket_color(ColorBucket::Red)
    );
}
