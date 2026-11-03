//! Tests for the filter menu lists.

use eframe::egui::{Context, Ui};
use tarragon_core::entity::TagSource;
use tarragon_core::sorting::SortMode;
use tarragon_db::DbError;
use tarragon_ui::gallery::{Gallery, Scope};

use super::{init, refresh_with};
use crate::filtering::tests::seeded;
use crate::logging::ring::{LogRing, RingLayer};
use crate::test_ui::{click_text, frame, open_manager};

#[test]
fn a_failed_read_logs_a_warning_and_keeps_the_lists_and_chips() {
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    gallery.set_filter_lists(vec!["night".into()], vec!["/r/one".into()]);
    let chips = |gallery: &mut Gallery| {
        let texts = frame(&ctx, Vec::new(), &mut |ui: &mut Ui| gallery.show(ui)).texts;
        texts.iter().filter(|t| t.0 == "/r/one").count()
    };
    click_text(&ctx, "Filter Folders", &mut |ui: &mut Ui| gallery.show(ui));
    click_text(&ctx, "/r/one", &mut |ui: &mut Ui| gallery.show(ui));
    let state = gallery.take_filter_change().expect("a chip was selected").0;
    assert_eq!(state.folders.len(), 1);
    let before = chips(&mut gallery);
    let ring = LogRing::new();
    let layer = RingLayer::new(ring.clone(), time::UtcOffset::UTC);
    let subscriber =
        tracing_subscriber::layer::SubscriberExt::with(tracing_subscriber::registry(), layer);
    tracing::subscriber::with_default(subscriber, || {
        refresh_with(&mut gallery, || Err(DbError::LockPoisoned));
    });
    assert!(gallery.take_filter_change().is_none());
    assert_eq!(
        gallery.filter_lists(),
        (&["night".to_owned()][..], &["/r/one".to_owned()][..])
    );
    assert_eq!(chips(&mut gallery), before);
    let lines = ring.snapshot();
    assert!(lines.iter().any(
        |l| l.level == tracing::Level::WARN && l.text.contains("cannot read the filter lists")
    ));
}

#[test]
fn the_manager_rows_count_the_users_of_each_user_tag() {
    let (db, _dir) = seeded();
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    init(&mut gallery, &db);
    let shown = open_manager(&ctx, &mut |ui: &mut Ui| gallery.show(ui));
    assert!(shown.contains(&"3 images".to_owned()));
    // A new use of the tag shows after the next read; colour tags never have a row.
    let night = db.ensure_tag("night", TagSource::User).expect("tag");
    db.add_tags_to_files_batch(&["/r/two/d.png".to_owned()], &[night])
        .expect("attach");
    super::refresh_lists(&mut gallery, &db);
    let shown = frame(&ctx, Vec::new(), &mut |ui: &mut Ui| gallery.show(ui)).texts;
    let shown: Vec<String> = shown.into_iter().map(|t| t.0).collect();
    assert!(shown.contains(&"4 images".to_owned()) && !shown.contains(&"3 images".to_owned()));
    assert!(!shown.contains(&"red".to_owned()));
}

#[test]
fn init_fills_the_lists_before_any_menu_opens() {
    let (db, _dir) = seeded();
    let mut gallery = Gallery::new(SortMode::Natural);
    init(&mut gallery, &db);
    let (tags, folders) = gallery.filter_lists();
    assert_eq!(tags, ["night"]);
    assert_eq!(folders, ["/r/one", "/r/one/sub", "/r/three", "/r/two"]);
}
