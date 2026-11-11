//! Tests for the one-step undo. The trash is a fake that moves files into a
//! temp folder, so no test touches the real trash.

use std::time::{Duration, Instant};

use image::{Rgba, RgbaImage};
use tarragon_core::entity::TagSource;
use tarragon_ui::gallery::{GridMenuEvent, Scope};

use super::*;
use crate::app::preview_tests::Rig;
use crate::app::tests::new_dir;
use crate::fileops::FileOpError;
use crate::sidebar;

/// Room for a thread to finish; each wait ends at this limit.
const LIMIT: Duration = Duration::from_secs(10);

fn write_png(path: &Path) {
    RgbaImage::from_pixel(8, 8, Rgba([10, 20, 30, 255]))
        .save(path)
        .expect("write the image");
}

/// Draw frames until `done` says yes, or fail after [`LIMIT`].
fn frames_until(rig: &mut Rig, what: &str, mut done: impl FnMut(&mut Rig) -> bool) {
    let end = Instant::now() + LIMIT;
    loop {
        rig.frame(Vec::new());
        if done(rig) {
            return;
        }
        assert!(Instant::now() < end, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Draw frames until a painted text holds `text`, or fail after [`LIMIT`].
fn wait_for_text(rig: &mut Rig, text: &str) {
    let end = Instant::now() + LIMIT;
    loop {
        if rig.frame(Vec::new()).0.iter().any(|t| t.contains(text)) {
            return;
        }
        assert!(Instant::now() < end, "timed out: no text {text:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// An app that shows its temp folder, loaded from disk with the images `names`.
fn folder_rig(names: &[&str]) -> (Rig, PathBuf) {
    let mut rig = Rig::new(&[], &[], 9);
    let dir = sidebar::normalize(&rig.dir.0);
    for name in names {
        write_png(&dir.join(name));
    }
    rig.app.navigate(&dir);
    rig.app.shell.gallery_mut().set_scope(Scope::Folder);
    let last = dir.join(names[names.len() - 1]);
    frames_until(&mut rig, "the folder to load", |rig| {
        rig.app.shell.gallery_mut().has_item(&last)
    });
    (rig, dir)
}

fn selected(rig: &mut Rig) -> Vec<PathBuf> {
    let mut paths = rig.app.shell.gallery_mut().selected_paths();
    paths.sort();
    paths
}

/// A trash that moves each file into `bin`, except the file named `refuse`,
/// which it refuses.
fn fake_trash(bin: PathBuf, refuse: &str) -> impl FnOnce(&[PathBuf]) -> Failures + Send + use<> {
    let refuse = refuse.to_owned();
    move |paths| {
        let mut failures = Vec::new();
        for path in paths {
            if path.file_name().is_some_and(|name| *name == *refuse) {
                failures.push((path.clone(), FileOpError::NotAFile(path.clone())));
            } else {
                std::fs::rename(path, bin.join(path.file_name().expect("a name")))
                    .expect("move into the fake trash");
            }
        }
        failures
    }
}

/// Rename `a.png` to `z.png` through the menu event and wait for the reload to
/// select the new name.
fn rename_a_to_z(rig: &mut Rig, dir: &Path) {
    let event = GridMenuEvent::RenameTo {
        path: dir.join("a.png"),
        new_stem: "z".to_owned(),
    };
    rig.app.apply_menu_event(&rig.ctx.clone(), event);
    let z = dir.join("z.png");
    frames_until(rig, "the new name to be selected", |rig| {
        selected(rig) == [z.clone()]
    });
}

/// Duplicate `names` through the menu event and wait until the slot holds the
/// duplicate. Give the copies.
fn duplicate(rig: &mut Rig, dir: &Path, names: &[&str]) -> Vec<Copied> {
    let paths = names.iter().map(|name| dir.join(name)).collect();
    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Duplicate(paths));
    frames_until(rig, "the duplicate to land", |rig| {
        matches!(rig.app.menu.undo, Some(UndoStep::Duplicate { .. }))
    });
    match rig.app.menu.undo.clone() {
        Some(UndoStep::Duplicate { copies }) => copies,
        other => panic!("no duplicate step: {other:?}"),
    }
}

#[test]
fn undo_after_a_rename_restores_the_name_the_tags_and_the_favorite() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let a = dir.join("a.png");
    let old_key = a.to_string_lossy().into_owned();
    let tag = rig.app.db.ensure_tag("keep", TagSource::User).expect("tag");
    rig.app
        .db
        .add_tags_to_files_batch(std::slice::from_ref(&old_key), &[tag])
        .expect("tag the file");
    rig.app
        .db
        .add_favorite(&old_key, None, 0)
        .expect("favorite the file");
    rename_a_to_z(&mut rig, &dir);
    assert!(rig.app.shell.gallery_mut().undo_available());

    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Undo);

    frames_until(&mut rig, "the old name to be selected", |rig| {
        selected(rig) == [a.clone()]
    });
    assert!(a.exists());
    assert!(!dir.join("z.png").exists());
    let tags = rig
        .app
        .db
        .tags_for_files(std::slice::from_ref(&old_key))
        .expect("tags");
    assert!(tags[&old_key].iter().any(|tag| tag.name == "keep"));
    let favorites = rig.app.db.list_favorites().expect("favorites");
    assert!(favorites.iter().any(|f| f.path == old_key));
    assert!(rig.app.menu.undo.is_none());
    assert!(!rig.app.shell.gallery_mut().undo_available());
    wait_for_text(&mut rig, "Undid the rename: a.png");
}

#[test]
fn a_second_undo_after_a_rename_undo_changes_nothing() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    rename_a_to_z(&mut rig, &dir);
    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Undo);
    assert!(dir.join("a.png").exists());
    assert!(rig.app.menu.undo.is_none(), "the run cleared the slot");
    assert!(!rig.app.shell.gallery_mut().undo_available());

    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Undo);

    assert!(dir.join("a.png").exists());
    assert!(!dir.join("z.png").exists());
    assert!(rig.app.menu.writing.is_none());
}

#[test]
fn undo_after_a_duplicate_of_two_files_trashes_both_copies() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let bin = new_dir();
    let copies = duplicate(&mut rig, &dir, &["a.png", "b.png"]);
    assert_eq!(copies.len(), 2);
    assert!(rig.app.shell.gallery_mut().undo_available());

    rig.app
        .undo_with(&rig.ctx.clone(), fake_trash(bin.0.clone(), ""));

    wait_for_text(&mut rig, "Undid the duplicate: moved 2 copies to the Trash");
    for copy in &copies {
        assert!(!copy.path.exists(), "{}", copy.path.display());
        assert!(bin.0.join(copy.path.file_name().expect("a name")).exists());
    }
    assert!(dir.join("a.png").exists());
    assert!(dir.join("b.png").exists());
    assert!(rig.app.menu.undo.is_none());
    assert!(!rig.app.shell.gallery_mut().undo_available());
}

#[test]
fn undo_after_a_duplicate_keeps_a_copy_that_was_edited_and_counts_it() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let bin = new_dir();
    let copies = duplicate(&mut rig, &dir, &["a.png", "b.png"]);
    let edited = copies[0].path.clone();
    std::fs::write(&edited, b"edited after the duplicate").expect("edit the copy");

    rig.app
        .undo_with(&rig.ctx.clone(), fake_trash(bin.0.clone(), ""));

    wait_for_text(
        &mut rig,
        "Undid the duplicate: moved 1 copy to the Trash, kept 1 that changed or are gone",
    );
    assert!(edited.exists(), "the edited copy stays");
    assert!(!copies[1].path.exists());
    assert!(rig.app.menu.undo.is_none(), "the step is spent");
}

#[test]
fn a_trash_failure_in_a_duplicate_undo_keeps_the_slot() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let bin = new_dir();
    let copies = duplicate(&mut rig, &dir, &["a.png", "b.png"]);
    let refused = copies[0]
        .path
        .file_name()
        .expect("a name")
        .to_string_lossy();

    rig.app
        .undo_with(&rig.ctx.clone(), fake_trash(bin.0.clone(), &refused));

    wait_for_text(&mut rig, "1 failed");
    assert!(copies[0].path.exists(), "the refused copy stays");
    assert!(!copies[1].path.exists());
    assert_eq!(
        rig.app.menu.undo,
        Some(UndoStep::Duplicate { copies }),
        "the slot is kept"
    );
    assert!(rig.app.shell.gallery_mut().undo_available());
}

#[test]
fn a_rename_then_a_paste_leaves_nothing_to_undo() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    rename_a_to_z(&mut rig, &dir);
    let source = new_dir();
    let image = source.0.join("pic.png");
    write_png(&image);
    rig.app
        .paste_with(&rig.ctx.clone(), move || Ok(vec![image]));
    frames_until(&mut rig, "the paste to end", |rig| {
        rig.app.menu.undo.is_none()
    });
    assert!(dir.join("pic.png").exists(), "the file was pasted");
    assert!(!rig.app.shell.gallery_mut().undo_available());

    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Undo);

    assert!(dir.join("z.png").exists(), "the rename stays");
    assert!(!dir.join("a.png").exists());
}

#[test]
fn a_paste_that_cannot_read_the_clipboard_still_clears_the_slot() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    rename_a_to_z(&mut rig, &dir);
    assert!(rig.app.shell.gallery_mut().undo_available());

    rig.app.paste_with(&rig.ctx.clone(), || {
        Err(crate::clipboard::ClipboardError::Timeout)
    });

    frames_until(&mut rig, "the paste to end", |rig| {
        rig.app.menu.undo.is_none()
    });
    assert!(!rig.app.shell.gallery_mut().undo_available());
    wait_for_text(&mut rig, "Cannot paste files");
}

#[test]
fn a_delete_clears_the_slot() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let bin = new_dir();
    rename_a_to_z(&mut rig, &dir);

    rig.app.delete_with(
        &rig.ctx.clone(),
        vec![dir.join("b.png")],
        false,
        fake_trash(bin.0.clone(), ""),
    );

    frames_until(&mut rig, "the delete to end", |rig| {
        rig.app.menu.undo.is_none()
    });
    assert!(!rig.app.shell.gallery_mut().undo_available());
}

#[test]
fn a_new_duplicate_replaces_a_rename_step_and_a_failed_one_keeps_it() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    rename_a_to_z(&mut rig, &dir);
    let step = rig.app.menu.undo.clone();
    assert!(matches!(step, Some(UndoStep::Rename { .. })));

    let missing = vec![dir.join("missing.png")];
    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Duplicate(missing));
    wait_for_text(&mut rig, "Cannot duplicate");
    assert_eq!(rig.app.menu.undo, step, "no copy was made");

    duplicate(&mut rig, &dir, &["b.png"]);
}

#[test]
fn a_rename_undo_is_refused_when_the_old_name_was_taken_outside_the_app() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    rename_a_to_z(&mut rig, &dir);
    write_png(&dir.join("a.png"));

    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Undo);

    wait_for_text(&mut rig, "Cannot undo the rename: a.png is taken");
    assert!(dir.join("z.png").exists(), "the file stays renamed");
    assert!(rig.app.menu.undo.is_none(), "a refused step is spent");
    assert!(!rig.app.shell.gallery_mut().undo_available());
}

#[test]
fn a_rename_undo_is_refused_when_the_renamed_file_is_gone() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    rename_a_to_z(&mut rig, &dir);
    std::fs::remove_file(dir.join("z.png")).expect("remove the file");

    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Undo);

    wait_for_text(&mut rig, "Cannot undo the rename: z.png is gone");
    assert!(!dir.join("a.png").exists());
    assert!(rig.app.menu.undo.is_none());
}

#[test]
fn undo_waits_with_a_notice_while_a_write_runs_and_keeps_the_step() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    rename_a_to_z(&mut rig, &dir);
    let (release, wait) = crossbeam_channel::bounded::<()>(1);
    rig.app.start_write(&rig.ctx.clone(), move || {
        let _ = wait.recv_timeout(Duration::from_secs(3));
        Outcome::default()
    });

    rig.app
        .apply_menu_event(&rig.ctx.clone(), GridMenuEvent::Undo);
    let _ = release.send(());

    wait_for_text(&mut rig, "Another file task is running");
    assert!(dir.join("z.png").exists());
    assert!(matches!(rig.app.menu.undo, Some(UndoStep::Rename { .. })));
}
