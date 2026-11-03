//! Tests for the colour square clicks.
//!
//! The seeded database has `red` on `a` and `b`, `transparent` on `a` only,
//! and `blue` on `c`.

use std::path::PathBuf;

use tarragon_core::entity::TagSource;
use tarragon_db::Database;
use tarragon_ui::preview::{ColorState, ColorStates, PreviewEvent};
use tarragon_ui::swatch::Swatch;

use super::{A, B, C, Failing, applied, selected_shell, user_tags, warnings};
use crate::filtering::tests::seeded;
use crate::tags::color_states;
use crate::tags::events::{apply_events, write_events};

const RED: usize = 0;
const GREEN: usize = 3;
const TRANSPARENT: usize = 11;

fn click(index: usize) -> PreviewEvent {
    PreviewEvent::ColorClicked(Swatch::ALL[index])
}

/// The colour tag names on a file.
fn colours(db: &Database, path: &str) -> Vec<String> {
    let map = db.tags_for_files(&[path.to_owned()]).expect("read");
    let mut names: Vec<String> = map
        .into_values()
        .flatten()
        .filter(|tag| tag.source == TagSource::AutoColor)
        .map(|tag| tag.name)
        .collect();
    names.sort();
    names
}

/// The squares for the selected files `a` and `b`.
fn squares(db: &Database) -> ColorStates {
    let by_file = db
        .tags_for_files(&[A.to_owned(), B.to_owned()])
        .expect("read");
    color_states(&by_file, 2)
}

fn states(set: &[(usize, ColorState)]) -> ColorStates {
    let mut states = ColorStates::default();
    for (index, state) in set {
        states[*index] = *state;
    }
    states
}

#[test]
fn a_click_on_files_without_the_colour_puts_it_on_both() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    assert!(apply_events(&mut shell, &db, vec![click(GREEN)]));
    assert_eq!(colours(&db, A), ["green", "red", "transparent"]);
    assert_eq!(colours(&db, B), ["green", "red"]);
    assert_eq!(colours(&db, C), ["blue"]);
    let want = states(&[
        (RED, ColorState::All),
        (GREEN, ColorState::All),
        (TRANSPARENT, ColorState::Some),
    ]);
    assert_eq!(squares(&db), want);
}

#[test]
fn a_click_when_one_file_has_the_colour_puts_it_on_the_other() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    assert_eq!(squares(&db)[TRANSPARENT], ColorState::Some);
    assert!(apply_events(&mut shell, &db, vec![click(TRANSPARENT)]));
    assert_eq!(squares(&db)[TRANSPARENT], ColorState::All);
    assert_eq!(colours(&db, B), ["red", "transparent"]);
}

#[test]
fn a_click_when_both_have_the_colour_removes_it_and_keeps_other_tags() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    assert!(apply_events(&mut shell, &db, vec![click(RED)]));
    assert_eq!(colours(&db, A), ["transparent"]);
    assert_eq!(colours(&db, B), Vec::<String>::new());
    assert_eq!(user_tags(&db, A), ["night"]);
    assert_eq!(user_tags(&db, B), ["night"]);
    // A file outside the selection keeps its tags.
    assert_eq!(colours(&db, "/r/two/d.png"), ["red", "transparent"]);
}

#[test]
fn the_transparent_square_toggles_on_then_off() {
    let (db, _dir) = seeded();
    let (_ctx, mut shell) = selected_shell();
    assert!(apply_events(&mut shell, &db, vec![click(TRANSPARENT)]));
    assert!(apply_events(&mut shell, &db, vec![click(TRANSPARENT)]));
    assert_eq!(squares(&db)[TRANSPARENT], ColorState::None);
    assert_eq!(colours(&db, A), ["red"]);
}

#[test]
fn a_failed_colour_write_logs_and_the_next_event_still_runs() {
    let paths = [PathBuf::from(A)];
    let cases = [Failing::Read, Failing::EnsureAuto, Failing::Detach];
    for store in cases {
        let lines = warnings(|| {
            assert!(write_events(
                &store,
                &paths,
                vec![click(RED), applied("cat")]
            ));
        });
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("cannot change the tags"));
    }
}
