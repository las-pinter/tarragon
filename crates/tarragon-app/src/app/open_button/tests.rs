//! Tests for the open button state. Default app names come from a stand-in
//! lookup, so no desktop tool runs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::filtering::tests::seeded;
use crate::pipeline::Repaint;

/// A state whose default app is always `name`, or unknown with `None`.
fn state(name: Option<&'static str>) -> OpenState {
    let repaint: Repaint = Arc::new(|| {});
    OpenState::new(AppNames::with_lookup(repaint, move |_: &Path| {
        Ok::<_, ()>(name.map(str::to_owned))
    }))
}

/// Poll until the name lookup has answered, and return the new button.
fn answered(state: &mut OpenState) -> Option<OpenButton> {
    let start = Instant::now();
    loop {
        if state.poll() {
            return state.button();
        }
        assert!(start.elapsed() < Duration::from_secs(5), "no answer");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn with_no_row_the_button_names_the_default_app_once_known() {
    let (db, _dir) = seeded();
    let mut state = state(Some("GIMP"));
    let button = state.select(&db, &[PathBuf::from("/a/x.png")]);
    assert_eq!(
        button,
        Some(OpenButton {
            label: OPEN_TEXT.to_owned(),
            choices: vec![DEFAULT_APP.to_owned()],
        })
    );
    assert_eq!(
        answered(&mut state),
        Some(OpenButton {
            label: "Open in GIMP".to_owned(),
            choices: vec!["GIMP".to_owned()],
        })
    );
}

#[test]
fn an_unknown_default_app_keeps_the_plain_button() {
    let (db, _dir) = seeded();
    let mut state = state(None);
    state.select(&db, &[PathBuf::from("/a/x.png")]);
    let button = answered(&mut state).expect("a button");
    assert_eq!(button.label, OPEN_TEXT);
}

#[test]
fn an_editor_row_names_the_button_and_leads_the_menu() {
    let (db, _dir) = seeded();
    db.upsert_editor_association("psd", "krita {file}")
        .expect("upsert");
    let mut state = state(None);
    let button = state
        .select(&db, &[PathBuf::from("/a/x.psd")])
        .expect("a button");
    assert_eq!(button.label, "Open in Krita");
    assert_eq!(button.choices, ["Krita", DEFAULT_APP]);
}

#[test]
fn a_default_app_named_like_an_editor_row_shows_once() {
    let (db, _dir) = seeded();
    db.upsert_editor_association("psd", "krita {file}")
        .expect("upsert");
    let mut state = state(Some("Krita"));
    state.select(&db, &[PathBuf::from("/a/x.psd")]);
    let button = answered(&mut state).expect("a button");
    assert_eq!(button.label, "Open in Krita");
    assert_eq!(button.choices, ["Krita"]);
}

#[test]
fn many_files_with_one_editor_row_open_all_in_that_app() {
    let (db, _dir) = seeded();
    db.upsert_editor_association("*", "krita {file}")
        .expect("upsert");
    let mut state = state(Some("GIMP"));
    let button = state.select(&db, &[PathBuf::from("/a/x.png"), PathBuf::from("/a/y.psd")]);
    assert_eq!(button, None, "many files have no single button");
    assert_eq!(state.open_all_app().as_deref(), Some("Krita"));
}

#[test]
fn many_files_that_open_in_different_apps_name_none() {
    let (db, _dir) = seeded();
    db.upsert_editor_association("psd", "krita {file}")
        .expect("upsert");
    let mut state = state(Some("GIMP"));
    state.select(&db, &[PathBuf::from("/a/x.png"), PathBuf::from("/a/y.psd")]);
    // The first ask sends the lookup of the PNG default app.
    assert_eq!(state.open_all_app(), None);
    answered(&mut state);
    assert_eq!(state.open_all_app(), None, "GIMP and Krita differ");
}

#[test]
fn many_files_of_the_default_app_take_its_name_once_known() {
    let (db, _dir) = seeded();
    let mut state = state(Some("GIMP"));
    state.select(&db, &[PathBuf::from("/a/x.png"), PathBuf::from("/a/y.PNG")]);
    assert_eq!(state.open_all_app(), None, "the name is not known yet");
    answered(&mut state);
    assert_eq!(state.open_all_app().as_deref(), Some("GIMP"));
}

#[test]
fn a_single_file_names_no_open_all_app() {
    let (db, _dir) = seeded();
    db.upsert_editor_association("*", "krita {file}")
        .expect("upsert");
    let mut state = state(None);
    state.select(&db, &[PathBuf::from("/a/x.png")]);
    assert_eq!(state.open_all_app(), None);
}

#[test]
fn no_single_file_gives_no_button_and_ignores_clicks() {
    let (db, _dir) = seeded();
    let mut state = state(None);
    assert_eq!(state.select(&db, &[]), None);
    assert!(state.apply(&[PreviewEvent::Open], None).is_none());
}

#[cfg(unix)]
#[test]
fn a_menu_row_opens_the_file_with_that_app() {
    let (db, dir) = seeded();
    let out = dir.0.join("out.txt");
    let template = format!("sh -c \"echo $0 > {}\" {{file}}", out.display());
    db.upsert_editor_association("*", &template)
        .expect("upsert");
    let mut state = state(None);
    state.select(&db, &[PathBuf::from("/a/x.png")]);
    assert!(state.apply(&[PreviewEvent::OpenWith(0)], None).is_none());
    let start = Instant::now();
    let text = loop {
        if let Ok(text) = std::fs::read_to_string(&out)
            && text.ends_with('\n')
        {
            break text;
        }
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "the app did not run"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(text, "/a/x.png\n");
}
