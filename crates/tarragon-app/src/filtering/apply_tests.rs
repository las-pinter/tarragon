//! Tests for applying a filter change to the gallery.

use std::collections::HashSet;
use std::path::PathBuf;

use eframe::egui::{Context, Ui};
use tarragon_core::sorting::SortMode;
use tarragon_db::{Database, DbError};
use tarragon_ui::gallery::{Gallery, Scope};
use tarragon_ui::grid::GridItem;

use super::tests::seeded;
use super::{apply_change, apply_with};
use crate::logging::ring::{LogRing, RingLayer};
use crate::test_ui::click_text;

/// A context with fonts and theme, and a global-scope gallery with the five
/// seeded files and one tag in the tag list.
fn setup() -> (Context, Gallery) {
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    let paths = ["one/a", "one/sub/b", "two/c", "two/d", "three/e"];
    let items = paths.map(|p| GridItem::new(PathBuf::from(format!("/r/{p}.png")), 1, None));
    gallery.set_items(items.into());
    gallery.set_scope(Scope::All);
    gallery.take_filter_change();
    gallery.set_filter_lists(vec!["night".into()], Vec::new());
    (ctx, gallery)
}

/// Open the tag menu, then toggle `night`, then apply the change.
fn toggle_night(
    ctx: &Context,
    gallery: &mut Gallery,
    db: &Database,
    open_menu: bool,
) -> Vec<String> {
    let mut run = |ui: &mut Ui| gallery.show(ui);
    if open_menu {
        click_text(ctx, "Filter Tags", &mut run);
    }
    let texts = click_text(ctx, "night", &mut run);
    assert!(apply_change(gallery, db));
    texts
}

#[test]
fn a_tag_click_filters_the_gallery_and_removing_it_restores_the_grid() {
    let (db, _dir) = seeded();
    let (ctx, mut gallery) = setup();
    assert_eq!(gallery.visible_len(), 5);
    toggle_night(&ctx, &mut gallery, &db, true);
    assert_eq!(gallery.visible_len(), 3);
    toggle_night(&ctx, &mut gallery, &db, false);
    assert_eq!(gallery.visible_len(), 5);
}

#[test]
fn the_pill_count_follows_each_change() {
    let (db, _dir) = seeded();
    let (ctx, mut gallery) = setup();
    let on = toggle_night(&ctx, &mut gallery, &db, true);
    assert!(on.contains(&"1 filter active".to_owned()));
    let off = toggle_night(&ctx, &mut gallery, &db, false);
    assert!(!off.iter().any(|t| t.ends_with("active")));
}

#[test]
fn a_failed_query_shows_all_items_and_logs_a_warning() {
    let (_ctx, mut gallery) = setup();
    gallery.set_filter_result(Some(HashSet::new()));
    assert_eq!(gallery.visible_len(), 0);
    gallery.set_scope(Scope::Folder);
    gallery.set_scope(Scope::All);
    let ring = LogRing::new();
    let layer = RingLayer::new(ring.clone(), time::UtcOffset::UTC);
    let subscriber =
        tracing_subscriber::layer::SubscriberExt::with(tracing_subscriber::registry(), layer);
    tracing::subscriber::with_default(subscriber, || {
        assert!(apply_with(&mut gallery, |_, _| Err(DbError::LockPoisoned)));
    });
    assert_eq!(gallery.visible_len(), 5);
    let lines = ring.snapshot();
    assert!(lines.iter().any(|l| l.level == tracing::Level::WARN
        && l.text.contains("cannot filter the gallery")
        && l.text.contains("poisoned")));
}
