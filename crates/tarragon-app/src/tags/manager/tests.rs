//! Tests for applying the Manage tags events.

use std::path::PathBuf;

use eframe::egui::{Context, Ui};
use tarragon_core::entity::TagSource;
use tarragon_db::{Database, DbError};
use tarragon_ui::Shell;
use tarragon_ui::gallery::{Scope, TagManagerEvent};
use tarragon_ui::grid::GridItem;
use tarragon_ui::preview::Meta;

use super::{Store, apply_events, write_events};
use crate::filtering::tests::seeded;
use crate::logging::ring::{LogRing, RingLayer};
use crate::test_ui::{click_text, frame, open_manager};

const A: &str = "/r/one/a.png";
const B: &str = "/r/one/sub/b.png";
const C: &str = "/r/two/c.png";

fn rename_event(id: i64, name: &str) -> TagManagerEvent {
    TagManagerEvent::Rename {
        id,
        new_name: name.to_owned(),
    }
}

/// The id of the tag with this name.
fn id_of(db: &Database, name: &str) -> i64 {
    let all = db.all_tags().expect("read");
    all.iter().find(|u| u.tag.name == name).expect(name).tag.id
}

/// The names of every tag, in the order the database lists them.
fn names(db: &Database) -> Vec<String> {
    let all = db.all_tags().expect("read");
    all.into_iter().map(|u| u.tag.name).collect()
}

/// A shell that shows the three seeded files, with `a` selected.
fn selected_shell(db: &Database) -> (Context, Shell) {
    let ctx = Context::default();
    tarragon_ui::install(&ctx);
    tarragon_ui::apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview_mut().set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    let items = [A, B, C].map(|p| GridItem::new(PathBuf::from(p), 1, None));
    shell.set_items(items.into());
    shell.gallery_mut().set_scope(Scope::All);
    crate::filtering::refresh_lists(shell.gallery_mut(), db);
    click_text(&ctx, "a.png", &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    assert_eq!(shell.selected_paths(), [PathBuf::from(A)]);
    crate::tags::refresh_preview(&mut shell, db);
    // Setting up must not leave a filter change behind.
    shell.gallery_mut().take_filter_change();
    (ctx, shell)
}

/// Draw the gallery and return its texts.
fn gallery_texts(ctx: &Context, shell: &mut Shell) -> Vec<String> {
    let texts = frame(ctx, Vec::new(), &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    })
    .texts;
    texts.into_iter().map(|t| t.0).collect()
}

/// Every text the Preview paints in one frame.
fn pill_texts(ctx: &Context, shell: &mut Shell) -> Vec<String> {
    let texts = frame(ctx, Vec::new(), &mut |ui: &mut Ui| {
        shell.preview_mut().show(ui);
    })
    .texts;
    texts.into_iter().map(|t| t.0).collect()
}

/// The warnings that `run` logs.
fn warnings(run: impl FnOnce()) -> Vec<String> {
    let ring = LogRing::new();
    let layer = RingLayer::new(ring.clone(), time::UtcOffset::UTC);
    let subscriber =
        tracing_subscriber::layer::SubscriberExt::with(tracing_subscriber::registry(), layer);
    tracing::subscriber::with_default(subscriber, run);
    ring.snapshot()
        .into_iter()
        .filter(|line| line.level == tracing::Level::WARN)
        .map(|line| line.text)
        .collect()
}

/// A store in which one chosen call fails.
#[derive(PartialEq)]
enum Failing {
    Lookup,
    Rename,
    Delete,
}

impl Store for Failing {
    fn tag_name(&self, _id: i64) -> Result<Option<String>, DbError> {
        if *self == Failing::Lookup {
            Err(DbError::LockPoisoned)
        } else {
            Ok(Some("old".to_owned()))
        }
    }

    fn rename(&self, _id: i64, new_name: &str) -> Result<String, DbError> {
        if *self == Failing::Rename {
            Err(DbError::LockPoisoned)
        } else {
            Ok(new_name.to_owned())
        }
    }

    fn delete(&self, _id: i64) -> Result<usize, DbError> {
        if *self == Failing::Delete {
            Err(DbError::LockPoisoned)
        } else {
            Ok(1)
        }
    }
}

#[test]
fn a_rename_reaches_the_db_the_lists_the_rows_the_chip_and_the_pills() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell(&db);
    let id = id_of(&db, "night");
    // The chip to rename.
    click_text(&ctx, "Filter Tags", &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    click_text(&ctx, "night", &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    assert!(shell.gallery_mut().take_filter_change().is_some());
    assert!(pill_texts(&ctx, &mut shell).contains(&"night".to_owned()));

    assert!(apply_events(
        &mut shell,
        &db,
        vec![rename_event(id, "dusk")]
    ));

    assert_eq!(id_of(&db, "dusk"), id);
    assert_eq!(shell.gallery_mut().filter_lists().0, ["dusk"]);
    let (state, _) = shell.gallery_mut().take_filter_change().expect("rerun");
    assert!(state.tags.contains("dusk") && !state.tags.contains("night"));
    let pills = pill_texts(&ctx, &mut shell);
    assert!(pills.contains(&"dusk".to_owned()) && !pills.contains(&"night".to_owned()));
    // The window opens on the new name: the menu entry and the row.
    let shown = open_manager(&ctx, &mut |ui: &mut Ui| shell.gallery_mut().show(ui));
    assert!(shown.contains(&"3 images".to_owned()));
    assert!(!shown.contains(&"night".to_owned()));
}

#[test]
fn a_rename_onto_an_existing_name_changes_nothing_and_shows_the_text() {
    let (db, _dir) = seeded();
    let solo = db.ensure_tag("solo", TagSource::User).expect("tag");
    let (ctx, mut shell) = selected_shell(&db);
    let before = names(&db);
    open_manager(&ctx, &mut |ui: &mut Ui| shell.gallery_mut().show(ui));
    let id = id_of(&db, "night");
    assert!(!apply_events(
        &mut shell,
        &db,
        vec![rename_event(id, &solo.name)]
    ));
    assert_eq!(names(&db), before);
    gallery_texts(&ctx, &mut shell);
    assert!(gallery_texts(&ctx, &mut shell).contains(&"Name already used".to_owned()));
    assert!(shell.gallery_mut().take_filter_change().is_none());
}

#[test]
fn a_refused_rename_shows_its_text_for_each_reason() {
    let (db, _dir) = seeded();
    let id = id_of(&db, "night");
    let long = "x".repeat(65);
    let cases = [
        (id, "red", "Name is reserved"),
        (id, "  ", "Enter a tag name"),
        (id, long.as_str(), "Name is longer than 64 characters"),
    ];
    for (target, name, text) in cases {
        let (ctx, mut shell) = selected_shell(&db);
        open_manager(&ctx, &mut |ui: &mut Ui| shell.gallery_mut().show(ui));
        assert!(!write_events(
            &db,
            shell.gallery_mut(),
            vec![rename_event(target, name)]
        ));
        gallery_texts(&ctx, &mut shell);
        let shown = gallery_texts(&ctx, &mut shell);
        assert!(shown.contains(&text.to_owned()), "{text}: {shown:?}");
        assert_eq!(id_of(&db, "night"), id);
    }
}

#[test]
fn a_built_in_or_missing_tag_cannot_be_renamed_and_has_its_own_text() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell(&db);
    let before = names(&db);
    let blue = id_of(&db, "blue");
    // A built-in tag is refused with no refresh. A missing one asks for the
    // refresh, so a dead row can go.
    for (target, refresh) in [(blue, false), (9999, true)] {
        let events = vec![rename_event(target, "sky")];
        assert_eq!(write_events(&db, shell.gallery_mut(), events), refresh);
    }
    assert_eq!(names(&db), before);
    // Neither has a row to show the text on, so the mapping is checked alone.
    let protected = db.rename_tag(blue, "sky").expect_err("built in");
    assert_eq!(
        super::rename_text(&protected),
        "Built-in tags cannot be changed"
    );
    let missing = db.rename_tag(9999, "sky").expect_err("missing");
    assert_eq!(super::rename_text(&missing), "This tag no longer exists");
}

#[test]
fn a_rename_to_the_same_name_writes_nothing() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell(&db);
    let id = id_of(&db, "night");
    assert!(!apply_events(
        &mut shell,
        &db,
        vec![rename_event(id, "night")]
    ));
    assert!(shell.gallery_mut().take_filter_change().is_none());
}

#[test]
fn deleting_a_used_tag_drops_its_rows_chip_and_pill() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell(&db);
    let id = id_of(&db, "night");
    click_text(&ctx, "Filter Tags", &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    click_text(&ctx, "night", &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    assert!(shell.gallery_mut().take_filter_change().is_some());

    assert!(apply_events(
        &mut shell,
        &db,
        vec![TagManagerEvent::Delete { id }]
    ));

    assert!(!names(&db).contains(&"night".to_owned()));
    let left = db
        .tags_for_files(&[A.to_owned(), B.to_owned(), C.to_owned()])
        .expect("read");
    assert!(left.values().flatten().all(|tag| tag.name != "night"));
    assert_eq!(shell.gallery_mut().filter_lists().0, Vec::<String>::new());
    let (state, _) = shell.gallery_mut().take_filter_change().expect("rerun");
    assert_eq!(state.tags.len(), 0);
    assert!(!pill_texts(&ctx, &mut shell).contains(&"night".to_owned()));
}

#[test]
fn deleting_a_tag_that_is_already_gone_refreshes_quietly() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell(&db);
    let lines = warnings(|| {
        assert!(apply_events(
            &mut shell,
            &db,
            vec![TagManagerEvent::Delete { id: 9999 }]
        ));
    });
    assert_eq!(lines, Vec::<String>::new());
    assert!(shell.gallery_mut().take_filter_change().is_some());
    assert_eq!(shell.gallery_mut().filter_lists().0, ["night"]);
    // The window still draws after the refresh.
    assert!(gallery_texts(&ctx, &mut shell).contains(&"Filter Tags".to_owned()));
}

#[test]
fn a_forced_delete_of_a_colour_tag_changes_nothing_and_logs() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell(&db);
    open_manager(&ctx, &mut |ui: &mut Ui| shell.gallery_mut().show(ui));
    let before = names(&db);
    let blue = id_of(&db, "blue");
    // A colour tag has no row, so the text is checked on the mapping alone.
    let refused = db.delete_tag(blue).expect_err("built in");
    assert_eq!(
        super::delete_text(&refused),
        "Built-in tags cannot be changed"
    );
    let lines = warnings(|| {
        assert!(!apply_events(
            &mut shell,
            &db,
            vec![TagManagerEvent::Delete { id: blue }]
        ));
    });
    assert_eq!(names(&db), before);
    assert_eq!(lines.len(), 1);
    // Colour tags have no row, so the text has no row to show on.
    assert!(shell.gallery_mut().take_filter_change().is_none());
}

#[test]
fn a_failed_delete_logs_and_shows_the_text_on_the_row() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell(&db);
    open_manager(&ctx, &mut |ui: &mut Ui| shell.gallery_mut().show(ui));
    let id = id_of(&db, "night");
    let lines = warnings(|| {
        let events = vec![TagManagerEvent::Delete { id }];
        assert!(!write_events(&Failing::Delete, shell.gallery_mut(), events));
    });
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("cannot delete tag"));
    gallery_texts(&ctx, &mut shell);
    let shown = gallery_texts(&ctx, &mut shell);
    assert!(shown.contains(&"Cannot delete: database error".to_owned()));
}

#[test]
fn a_database_failure_on_rename_logs_and_shows_the_database_text() {
    let (db, _dir) = seeded();
    let id = id_of(&db, "night");
    for store in [Failing::Lookup, Failing::Rename] {
        let (ctx, mut shell) = selected_shell(&db);
        open_manager(&ctx, &mut |ui: &mut Ui| shell.gallery_mut().show(ui));
        let lines = warnings(|| {
            let events = vec![rename_event(id, "dusk")];
            assert!(!write_events(&store, shell.gallery_mut(), events));
        });
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("cannot rename a tag"));
        gallery_texts(&ctx, &mut shell);
        let shown = gallery_texts(&ctx, &mut shell);
        assert!(shown.contains(&"Cannot rename: database error".to_owned()));
    }
}

#[test]
fn a_batch_keeps_going_after_a_refusal_and_reports_the_write() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell(&db);
    let id = id_of(&db, "night");
    let events = vec![rename_event(id, "red"), rename_event(id, "dusk")];
    assert!(apply_events(&mut shell, &db, events));
    assert_eq!(id_of(&db, "dusk"), id);
}

#[test]
fn a_batch_of_two_writes_flags_one_filter_change() {
    let (db, _dir) = seeded();
    let solo = db.ensure_tag("solo", TagSource::User).expect("tag");
    let (_ctx, mut shell) = selected_shell(&db);
    let events = vec![
        rename_event(id_of(&db, "night"), "dusk"),
        rename_event(solo.id, "moon"),
    ];
    assert!(apply_events(&mut shell, &db, events));
    assert_eq!(shell.gallery_mut().filter_lists().0, ["dusk", "moon"]);
    assert!(shell.gallery_mut().take_filter_change().is_some());
    assert!(shell.gallery_mut().take_filter_change().is_none());
}

#[test]
fn a_refused_row_keeps_its_text_through_the_refresh_of_the_same_batch() {
    let (db, _dir) = seeded();
    let solo = db.ensure_tag("solo", TagSource::User).expect("tag");
    let (ctx, mut shell) = selected_shell(&db);
    open_manager(&ctx, &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    let events = vec![
        rename_event(id_of(&db, "night"), "red"),
        rename_event(solo.id, "moon"),
    ];
    assert!(apply_events(&mut shell, &db, events));
    gallery_texts(&ctx, &mut shell);
    let shown = gallery_texts(&ctx, &mut shell);
    assert!(shown.contains(&"Name is reserved".to_owned()), "{shown:?}");
}

#[test]
fn a_rename_of_a_tag_that_is_gone_drops_its_row() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell(&db);
    let shown = open_manager(&ctx, &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    assert!(shown.contains(&"Rename".to_owned()));
    let id = id_of(&db, "night");
    db.delete_tag(id).expect("delete");
    assert!(apply_events(
        &mut shell,
        &db,
        vec![rename_event(id, "dusk")]
    ));
    gallery_texts(&ctx, &mut shell);
    let shown = gallery_texts(&ctx, &mut shell);
    assert!(!shown.contains(&"Rename".to_owned()), "{shown:?}");
    assert!(!shown.contains(&"This tag no longer exists".to_owned()));
}

#[test]
fn an_empty_queue_writes_nothing() {
    let (db, _dir) = seeded();
    let mut shell = Shell::new();
    assert!(!super::apply_manager_events(&mut shell, &db));
}
