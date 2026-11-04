//! Tests for applying the Preview tag events.

mod color;

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui::{Context, Event, Modifiers, PointerButton, Ui, vec2};
use tarragon_core::entity::{Tag, TagSource};
use tarragon_db::{Database, DbError};
use tarragon_ui::Shell;
use tarragon_ui::gallery::Scope;
use tarragon_ui::grid::GridItem;
use tarragon_ui::preview::{ColorState, Meta, PreviewEvent, TagPill};
use tarragon_ui::swatch::Swatch;

use super::{Store, apply_events, write_events};
use crate::filtering::tests::seeded;
use crate::logging::ring::{LogRing, RingLayer};
use crate::test_ui::{click_text, frame};

const A: &str = "/r/one/a.png";
const B: &str = "/r/one/sub/b.png";
const C: &str = "/r/two/c.png";

fn applied(name: &str) -> PreviewEvent {
    PreviewEvent::TagApplied(name.to_owned())
}

fn removed(name: &str) -> PreviewEvent {
    PreviewEvent::TagRemoved(name.to_owned())
}

/// Click the tile named `label`, with `modifiers` held, then draw once more.
fn click_tile(ctx: &Context, shell: &mut Shell, label: &str, modifiers: Modifiers) {
    let mut run = |ui: &mut Ui| shell.gallery_mut().show(ui);
    let at = frame(ctx, Vec::new(), &mut run)
        .texts
        .into_iter()
        .find(|t| t.0 == label)
        .expect(label)
        .1
        + vec2(3.0, 3.0);
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    };
    frame(ctx, vec![Event::PointerMoved(at)], &mut run);
    // egui learns the held keys from this event, not from the click event.
    frame(ctx, vec![Event::ModifiersChanged(modifiers)], &mut run);
    frame(ctx, vec![button(true)], &mut run);
    frame(ctx, vec![button(false)], &mut run);
    frame(ctx, Vec::new(), &mut run);
}

/// A shell that shows the three seeded files `a`, `b` and `c`, with `a` and
/// `b` selected and a file in the Preview.
fn selected_shell() -> (Context, Shell) {
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
    click_tile(&ctx, &mut shell, "a.png", Modifiers::NONE);
    click_tile(&ctx, &mut shell, "b.png", Modifiers::COMMAND);
    assert_eq!(shell.selected_paths(), [PathBuf::from(A), PathBuf::from(B)]);
    (ctx, shell)
}

/// Every text the Preview paints in one frame.
fn pill_texts(ctx: &Context, shell: &mut Shell) -> Vec<String> {
    let mut run = |ui: &mut Ui| shell.preview_mut().show(ui);
    let texts = frame(ctx, Vec::new(), &mut run).texts;
    texts.into_iter().map(|t| t.0).collect()
}

/// The names of the user tags on a file.
fn user_tags(db: &Database, path: &str) -> Vec<String> {
    let stored = [path.to_owned()];
    let map = db.tags_for_files(&stored).expect("read");
    let mut names: Vec<String> = map
        .into_values()
        .flatten()
        .filter(|tag| tag.source == TagSource::User)
        .map(|tag| tag.name)
        .collect();
    names.sort();
    names
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
    Never,
    Ensure,
    Attach,
    Lookup,
    Detach,
    EnsureAuto,
    Read,
}

fn fail() -> DbError {
    DbError::LockPoisoned
}

impl Store for Failing {
    fn ensure_user(&self, name: &str) -> Result<Tag, DbError> {
        if *self == Failing::Ensure {
            return Err(fail());
        }
        Ok(Tag {
            id: 7,
            name: name.to_owned(),
            source: TagSource::User,
        })
    }

    fn attach(&self, _paths: &[String], _tag: Tag) -> Result<usize, DbError> {
        if *self == Failing::Attach {
            Err(fail())
        } else {
            Ok(1)
        }
    }

    fn user_tag_id(&self, _name: &str) -> Result<Option<i64>, DbError> {
        if *self == Failing::Lookup {
            Err(fail())
        } else {
            Ok(Some(7))
        }
    }

    fn detach(&self, _paths: &[String], _tag_id: i64) -> Result<usize, DbError> {
        if *self == Failing::Detach {
            Err(fail())
        } else {
            Ok(1)
        }
    }

    fn ensure_auto(&self, name: &str) -> Result<Tag, DbError> {
        if *self == Failing::EnsureAuto {
            return Err(fail());
        }
        Ok(Tag {
            id: 8,
            name: name.to_owned(),
            source: TagSource::AutoColor,
        })
    }

    fn auto_tag_id(&self, _name: &str) -> Result<Option<i64>, DbError> {
        Ok(Some(8))
    }

    fn file_tags(&self, paths: &[String]) -> Result<HashMap<String, Vec<Tag>>, DbError> {
        match self {
            Failing::Read => Err(fail()),
            // Every file already holds the red colour, so a click detaches it.
            Failing::Detach => Ok(paths
                .iter()
                .map(|path| {
                    let red = Tag {
                        id: 8,
                        name: "red".to_owned(),
                        source: TagSource::AutoColor,
                    };
                    (path.clone(), vec![red])
                })
                .collect()),
            _ => Ok(HashMap::new()),
        }
    }
}

#[test]
fn applying_a_new_tag_writes_every_selected_file_and_shows_it_on_all() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell();
    assert!(apply_events(&mut shell, &db, vec![applied("cat")]));
    assert_eq!(user_tags(&db, A), ["cat", "night"]);
    assert_eq!(user_tags(&db, B), ["cat", "night"]);
    assert_eq!(user_tags(&db, C), ["night"]);
    assert!(pill_texts(&ctx, &mut shell).contains(&"cat".to_owned()));
}

#[test]
fn the_retry_and_editor_events_write_no_tags() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    let events = vec![PreviewEvent::RetryRender, PreviewEvent::OpenInEditor];
    assert!(!apply_events(&mut shell, &db, events));
    assert_eq!(user_tags(&db, A), ["night"]);
    assert_eq!(user_tags(&db, B), ["night"]);
}

#[test]
fn applying_a_partial_tag_adds_only_the_missing_files() {
    let (db, _dir) = seeded();
    let solo = db.ensure_tag("solo", TagSource::User).expect("tag");
    db.add_tags_to_files_batch(&[A.to_owned()], &[solo])
        .expect("attach");
    let (_ctx, mut shell) = selected_shell();
    assert!(apply_events(&mut shell, &db, vec![applied("solo")]));
    assert_eq!(user_tags(&db, A), ["night", "solo"]);
    assert_eq!(user_tags(&db, B), ["night", "solo"]);
    assert_eq!(user_tags(&db, C), ["night"]);
    let usage = db.all_tags().expect("read");
    let solo = usage.iter().find(|u| u.tag.name == "solo").expect("solo");
    assert_eq!(solo.usage_count, 2);
    // Both selected files carry it now, so the pill reads All.
    let by_file = db
        .tags_for_files(&[A.to_owned(), B.to_owned()])
        .expect("read");
    let pills = crate::tags::pills(&by_file, 2);
    assert!(pills.contains(&TagPill {
        name: "solo".to_owned(),
        state: ColorState::All
    }));
}

#[test]
fn removing_a_tag_takes_it_off_the_selected_files_only() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    assert!(apply_events(&mut shell, &db, vec![removed("night")]));
    assert_eq!(user_tags(&db, A), Vec::<String>::new());
    assert_eq!(user_tags(&db, B), Vec::<String>::new());
    assert_eq!(user_tags(&db, C), ["night"]);
}

#[test]
fn removing_refreshes_the_pills() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell();
    crate::tags::refresh_preview(&mut shell, &db);
    assert!(pill_texts(&ctx, &mut shell).contains(&"night".to_owned()));
    assert!(apply_events(&mut shell, &db, vec![removed("night")]));
    assert!(!pill_texts(&ctx, &mut shell).contains(&"night".to_owned()));
}

#[test]
fn removing_an_unknown_or_colour_tag_writes_nothing() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    let before = db.tags_for_files(&[A.to_owned()]).expect("read");
    let events = vec![removed("nope"), removed("red")];
    assert!(!apply_events(&mut shell, &db, events));
    assert_eq!(db.tags_for_files(&[A.to_owned()]).expect("read"), before);
}

#[test]
fn a_refused_name_logs_and_writes_no_row() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    let before = db.all_tags().expect("read").len();
    let lines = warnings(|| {
        let events = ["red", "RED", "transparent", "", "   ", &"x".repeat(65)];
        let events = events.iter().map(|name| applied(name)).collect();
        assert!(!apply_events(&mut shell, &db, events));
    });
    assert_eq!(lines.len(), 6);
    assert!(lines.iter().all(|l| l.contains("cannot change the tags")));
    assert_eq!(db.all_tags().expect("read").len(), before);
    assert_eq!(user_tags(&db, A), ["night"]);
}

#[test]
fn a_name_is_trimmed_before_it_is_written() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    assert!(apply_events(&mut shell, &db, vec![applied("  cat ")]));
    assert_eq!(user_tags(&db, A), ["cat", "night"]);
}

#[test]
fn a_write_adds_the_tag_to_the_filter_list() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    assert!(apply_events(&mut shell, &db, vec![applied("cat")]));
    assert_eq!(shell.gallery_mut().filter_lists().0, ["cat", "night"]);
}

#[test]
fn a_write_runs_an_active_tag_filter_again_and_a_refused_one_does_not() {
    let (db, _dir) = seeded();
    let (ctx, mut shell) = selected_shell();
    shell
        .gallery_mut()
        .set_filter_lists(vec!["night".into()], Vec::new());
    click_text(&ctx, "Filter Tags", &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    click_text(&ctx, "night", &mut |ui: &mut Ui| {
        shell.gallery_mut().show(ui);
    });
    assert!(shell.gallery_mut().take_filter_change().is_some());
    assert!(!apply_events(&mut shell, &db, vec![applied("red")]));
    assert!(shell.gallery_mut().take_filter_change().is_none());
    assert!(apply_events(&mut shell, &db, vec![applied("cat")]));
    let (state, _) = shell.gallery_mut().take_filter_change().expect("rerun");
    assert!(state.tags.contains("night"));
}

#[test]
fn a_refused_write_leaves_the_filter_list_alone() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    shell.gallery_mut().set_filter_lists(Vec::new(), Vec::new());
    assert!(!apply_events(&mut shell, &db, vec![applied("red")]));
    assert_eq!(shell.gallery_mut().filter_lists().0, Vec::<String>::new());
}

#[test]
fn one_failed_event_does_not_stop_the_next() {
    let (db, _dir) = seeded();
    let paths = [PathBuf::from(A), PathBuf::from(B)];
    let lines = warnings(|| {
        assert!(write_events(
            &db,
            &paths,
            vec![applied("red"), applied("cat")]
        ));
    });
    assert_eq!(lines.len(), 1);
    assert_eq!(user_tags(&db, B), ["cat", "night"]);
}

#[test]
fn an_empty_selection_writes_nothing() {
    let (db, _dir) = seeded();
    let before = db.all_tags().expect("read").len();
    let click = PreviewEvent::ColorClicked(Swatch::ALL[0]);
    assert!(!write_events(
        &db,
        &[],
        vec![applied("cat"), removed("night"), click]
    ));
    assert_eq!(db.all_tags().expect("read").len(), before);
    assert_eq!(user_tags(&db, A), ["night"]);
}

#[test]
fn a_failed_database_call_logs_a_warning_and_reports_no_write() {
    let paths = [PathBuf::from(A)];
    let cases = [
        (Failing::Ensure, applied("cat")),
        (Failing::Attach, applied("cat")),
        (Failing::Lookup, removed("cat")),
        (Failing::Detach, removed("cat")),
    ];
    for (store, event) in cases {
        let lines = warnings(|| assert!(!write_events(&store, &paths, vec![event])));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("cannot change the tags"));
    }
}

#[test]
fn a_working_store_reports_a_write_for_both_events() {
    let paths = [PathBuf::from(A)];
    assert!(write_events(&Failing::Never, &paths, vec![applied("cat")]));
    assert!(write_events(&Failing::Never, &paths, vec![removed("cat")]));
}

#[test]
fn an_empty_queue_writes_nothing() {
    let (db, _dir) = seeded();
    let mut shell = Shell::new();
    assert!(!super::apply_preview_events(&mut shell, &db));
}
