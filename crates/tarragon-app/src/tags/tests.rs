//! Tests for the Preview tag refresh.

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui::{Context, Ui};
use tarragon_core::entity::{Tag, TagSource};
use tarragon_db::{Database, DbError};
use tarragon_ui::preview::{ColorState, Meta, Preview, TagPill};

mod colors;

use super::{known_tags, pills, refresh_with, selection_changed};
use crate::filtering::tests::seeded;
use crate::logging::ring::{LogRing, RingLayer};
use crate::test_ui::{click_text, frame};

/// The seeded files `a` and `b`, as the selection gives them.
fn selection() -> Vec<PathBuf> {
    ["/r/one/a.png", "/r/one/sub/b.png"]
        .map(PathBuf::from)
        .to_vec()
}

/// Add `solo` to `a` only and an `unused` user tag to no file.
fn add_user_tags(db: &Database) {
    let solo = db.ensure_tag("solo", TagSource::User).expect("tag");
    db.add_tags_to_files_batch(&["/r/one/a.png".to_owned()], &[solo])
        .expect("attach");
    db.ensure_tag("unused", TagSource::User).expect("tag");
}

fn pill(name: &str, state: ColorState) -> TagPill {
    TagPill {
        name: name.to_owned(),
        state,
    }
}

/// A Preview that shows a file, so the pills draw.
fn preview() -> Preview {
    let mut preview = Preview::default();
    preview.set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    preview
}

/// Every text one frame of the Preview paints.
fn painted(ctx: &Context, preview: &mut Preview) -> Vec<String> {
    let texts = frame(ctx, Vec::new(), &mut |ui: &mut Ui| preview.show(ui)).texts;
    texts.into_iter().map(|t| t.0).collect()
}

fn read_from(db: &Database) -> impl FnOnce(&[String]) -> Result<super::Read, DbError> + '_ {
    |stored| Ok((db.tags_for_files(stored)?, db.all_tags()?))
}

#[test]
fn a_tag_on_all_files_is_all_and_a_tag_on_some_is_some() {
    let (db, _dir) = seeded();
    add_user_tags(&db);
    let stored: Vec<String> = selection()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let by_file = db.tags_for_files(&stored).expect("read");
    assert_eq!(
        pills(&by_file, 2),
        [
            pill("night", ColorState::All),
            pill("solo", ColorState::Some)
        ]
    );
}

fn user_tag(name: &str) -> Tag {
    Tag {
        id: 1,
        name: name.to_owned(),
        source: TagSource::User,
    }
}

#[test]
fn a_repeated_tag_on_one_file_counts_once() {
    let by_file = HashMap::from([
        ("/r/a.png".to_owned(), vec![user_tag("x"), user_tag("x")]),
        ("/r/b.png".to_owned(), Vec::new()),
    ]);
    assert_eq!(pills(&by_file, 2), [pill("x", ColorState::Some)]);
}

#[test]
fn a_requested_file_missing_from_the_map_keeps_a_tag_partial() {
    let by_file = HashMap::from([("/r/a.png".to_owned(), vec![user_tag("x")])]);
    assert_eq!(pills(&by_file, 2), [pill("x", ColorState::Some)]);
}

#[test]
fn colour_tags_are_no_pills_and_not_known() {
    let (db, _dir) = seeded();
    add_user_tags(&db);
    let stored = vec!["/r/one/a.png".to_owned()];
    let by_file = db.tags_for_files(&stored).expect("read");
    // File `a` also has red and transparent.
    let names: Vec<String> = pills(&by_file, 1).into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["night", "solo"]);
    assert_eq!(
        known_tags(db.all_tags().expect("read")),
        ["night", "solo", "unused"]
    );
}

#[test]
fn an_empty_selection_gives_no_pills() {
    let (db, _dir) = seeded();
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut preview = preview();
    preview.set_tags(vec![pill("stale", ColorState::All)]);
    refresh_with(&mut preview, &[], true, read_from(&db));
    assert!(!painted(&ctx, &mut preview).contains(&"stale".to_owned()));
}

#[test]
fn a_refresh_draws_the_pills_of_the_selection() {
    let (db, _dir) = seeded();
    add_user_tags(&db);
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut preview = preview();
    refresh_with(&mut preview, &selection(), true, read_from(&db));
    let texts = painted(&ctx, &mut preview);
    assert!(texts.contains(&"night".to_owned()));
    assert!(texts.contains(&"solo".to_owned()));
    assert!(!texts.contains(&"red".to_owned()));
}

#[test]
fn a_refresh_fills_the_add_menu_with_the_user_tags_only() {
    let (db, _dir) = seeded();
    add_user_tags(&db);
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut preview = preview();
    refresh_with(&mut preview, &selection(), true, read_from(&db));
    let mut show = |ui: &mut Ui| preview.show(ui);
    // `unused` is on no file, so only the known list can draw it.
    let texts = click_text(&ctx, "+ add", &mut show);
    assert!(texts.contains(&"unused".to_owned()));
    assert!(!texts.contains(&"blue".to_owned()));
    assert!(!texts.contains(&"transparent".to_owned()));
}

#[test]
fn a_failed_read_logs_a_warning_and_keeps_the_pills() {
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut preview = preview();
    preview.set_tags(vec![pill("kept", ColorState::Some)]);
    let ring = LogRing::new();
    let layer = RingLayer::new(ring.clone(), time::UtcOffset::UTC);
    let subscriber =
        tracing_subscriber::layer::SubscriberExt::with(tracing_subscriber::registry(), layer);
    tracing::subscriber::with_default(subscriber, || {
        refresh_with(&mut preview, &selection(), true, |_| {
            Err(DbError::LockPoisoned)
        });
    });
    assert!(painted(&ctx, &mut preview).contains(&"kept".to_owned()));
    let lines = ring.snapshot();
    assert!(lines.iter().any(|l| {
        l.level == tracing::Level::WARN && l.text.contains("cannot read the tags of the selection")
    }));
}

#[test]
fn the_refresh_is_due_once_per_selection_change() {
    let mut seen = 0;
    assert!(!selection_changed(&mut seen, 0));
    assert!(selection_changed(&mut seen, 1));
    assert!(!selection_changed(&mut seen, 1));
    assert!(!selection_changed(&mut seen, 1));
    assert!(selection_changed(&mut seen, 2));
    assert_eq!(seen, 2);
}
