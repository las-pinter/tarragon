//! Tests for the confirmed deletes. The trash is a fake that moves files into a
//! temp folder, so no test touches the real trash. The real call has its own
//! test in `fileops`.

use std::time::{Duration, Instant};

use eframe::egui::{Event, Key, Modifiers};
use tarragon_core::entity::TagSource;
use tarragon_ui::gallery::{GridMenuEvent, Scope};

use super::*;
use crate::app::preview_tests::Rig;
use crate::app::tests::new_dir;
use crate::sidebar;

/// Room for a thread to finish; each wait ends at this limit.
const LIMIT: Duration = Duration::from_secs(10);

fn write_png(path: &Path) {
    image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
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
    rig.app.shell.gallery_mut().selected_paths()
}

/// A trash that moves each file into `bin`, except the file named `refuse`,
/// which it refuses.
fn fake_trash(bin: PathBuf, refuse: &'static str) -> impl FnOnce(&[PathBuf]) -> Failures + Send {
    move |paths| {
        let mut failures = Vec::new();
        for path in paths {
            if path.file_name().is_some_and(|name| name == refuse) {
                failures.push((path.clone(), FileOpError::NotAFile(path.clone())));
            } else {
                std::fs::rename(path, bin.join(path.file_name().expect("a name")))
                    .expect("move into the fake trash");
            }
        }
        failures
    }
}

fn escape() -> Vec<Event> {
    vec![Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]
}

#[test]
fn a_trash_removes_the_tile_and_selects_the_nearest_tile() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png", "c.png"]);
    let bin = new_dir();
    let b = dir.join("b.png");
    rig.app
        .shell
        .gallery_mut()
        .select_paths(std::slice::from_ref(&b));
    let ctx = rig.ctx.clone();

    rig.app
        .delete_with(&ctx, vec![b.clone()], false, fake_trash(bin.0.clone(), ""));

    let c = dir.join("c.png");
    frames_until(&mut rig, "the nearest tile to be selected", |rig| {
        selected(rig) == [c.clone()]
    });
    assert!(!b.exists());
    assert!(bin.0.join("b.png").exists());
    assert!(!rig.app.shell.gallery_mut().has_item(&b));
    assert!(rig.app.shell.gallery_mut().has_item(&dir.join("a.png")));
}

#[test]
fn a_trash_keeps_the_tags_of_the_file_and_drops_its_thumbnail_row() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let bin = new_dir();
    let b = dir.join("b.png");
    let key = b.to_string_lossy().into_owned();
    frames_until(&mut rig, "the thumbnail row", |rig| {
        rig.app.db.thumbnail(&key).expect("read").is_some()
    });
    let tag = rig.app.db.ensure_tag("keep", TagSource::User).expect("tag");
    rig.app
        .db
        .add_tags_to_files_batch(std::slice::from_ref(&key), &[tag])
        .expect("tag the file");
    let ctx = rig.ctx.clone();

    rig.app
        .delete_with(&ctx, vec![b.clone()], false, fake_trash(bin.0.clone(), ""));

    frames_until(&mut rig, "the thumbnail row to go", |rig| {
        rig.app.db.thumbnail(&key).expect("read").is_none()
    });
    let tags = rig
        .app
        .db
        .tags_for_files(std::slice::from_ref(&key))
        .expect("tags");
    assert!(tags[&key].iter().any(|tag| tag.name == "keep"));
}

#[test]
fn a_refused_trash_file_opens_the_instead_prompt_and_cancel_keeps_it() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let bin = new_dir();
    let (a, b) = (dir.join("a.png"), dir.join("b.png"));
    let ctx = rig.ctx.clone();

    rig.app.delete_with(
        &ctx,
        vec![a.clone(), b.clone()],
        false,
        fake_trash(bin.0.clone(), "b.png"),
    );

    frames_until(&mut rig, "the instead prompt", |rig| {
        rig.app.shell.gallery_mut().delete_open()
    });
    assert!(!a.exists());
    assert!(b.exists());
    // One item, not two: the prompt holds the refused file only.
    // The first frame of a window only measures it.
    rig.frame(Vec::new());
    let (texts, _) = rig.frame(Vec::new());
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with("1 item could not be moved to the Trash")),
        "{texts:?}"
    );

    rig.frame(escape());
    assert!(!rig.app.shell.gallery_mut().delete_open());
    assert!(b.exists());
    assert!(rig.app.shell.gallery_mut().has_item(&b));
}

#[test]
fn a_confirmed_instead_deletes_the_refused_files_only() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let b = dir.join("b.png");
    let ctx = rig.ctx.clone();
    // What the app gets after a confirm of the "instead" prompt.
    rig.app.apply_menu_event(
        &ctx,
        GridMenuEvent::ConfirmedDelete {
            paths: vec![b.clone()],
            permanent: true,
        },
    );
    frames_until(&mut rig, "the tile to go", |rig| {
        !rig.app.shell.gallery_mut().has_item(&b)
    });
    assert!(!b.exists());
    assert!(dir.join("a.png").exists());
}

#[test]
fn a_permanent_delete_waits_for_the_confirm_and_escape_changes_nothing() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let paths = vec![dir.join("a.png"), dir.join("b.png")];
    let ctx = rig.ctx.clone();

    rig.app
        .apply_menu_event(&ctx, GridMenuEvent::DeletePermanently(paths.clone()));
    assert!(rig.app.shell.gallery_mut().delete_open());
    assert!(rig.app.menu.writing.is_none());
    rig.frame(Vec::new());
    assert!(rig.app.menu.writing.is_none());

    rig.frame(escape());
    assert!(!rig.app.shell.gallery_mut().delete_open());
    assert!(rig.app.menu.writing.is_none());
    for path in &paths {
        assert!(path.exists(), "{}", path.display());
        assert!(rig.app.shell.gallery_mut().has_item(path));
    }
}

#[test]
fn a_confirmed_permanent_delete_removes_the_files_for_good() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png", "c.png"]);
    let gone = vec![dir.join("a.png"), dir.join("b.png")];
    let ctx = rig.ctx.clone();
    rig.app.apply_menu_event(
        &ctx,
        GridMenuEvent::ConfirmedDelete {
            paths: gone.clone(),
            permanent: true,
        },
    );
    let c = dir.join("c.png");
    frames_until(&mut rig, "the survivor to be selected", |rig| {
        selected(rig) == [c.clone()]
    });
    for path in &gone {
        assert!(!path.exists(), "{}", path.display());
    }
    assert!(c.exists());
}

#[test]
fn the_outcome_names_the_refused_trash_files_and_counts_both_sides() {
    let dir = new_dir();
    let bin = new_dir();
    let (a, b) = (dir.0.join("a.png"), dir.0.join("b.png"));
    write_png(&a);
    write_png(&b);
    let trash = delete_outcome(
        &[a.clone(), b.clone()],
        false,
        None,
        fake_trash(bin.0.clone(), "b.png"),
    );
    assert_eq!(trash.deleted, std::slice::from_ref(&a));
    assert_eq!(trash.refused, std::slice::from_ref(&b));
    let text = trash.notice.expect("a notice");
    assert!(
        text.starts_with("Moved 1 file to the Trash, 1 failed: "),
        "{text}"
    );

    // A failed permanent delete offers nothing further.
    let permanent = delete_outcome(std::slice::from_ref(&b), true, None, |paths| {
        vec![(paths[0].clone(), FileOpError::NotAFile(paths[0].clone()))]
    });
    assert_eq!(permanent.refused, Vec::<PathBuf>::new());
    assert_eq!(permanent.deleted, Vec::<PathBuf>::new());
    assert_eq!(delete_text(true, 3, 0, None), "Deleted 3 files");
    assert_eq!(delete_text(false, 1, 0, None), "Moved 1 file to the Trash");
}

#[test]
fn a_delete_that_ends_while_the_same_folder_loads_reloads_after_that_load() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png", "c.png"]);
    let b = dir.join("b.png");
    // An open of the same folder is waiting when the delete ends.
    rig.app.thumbs.reload_folder(dir.clone());
    assert!(rig.app.thumbs.is_opening());
    std::fs::remove_file(&b).expect("remove the file");
    let ctx = rig.ctx.clone();

    rig.app
        .finish_delete(&ctx, vec![b.clone()], Vec::new(), Some(&dir));
    assert!(rig.app.menu.deferred.is_some());

    let c = dir.join("c.png");
    frames_until(&mut rig, "the nearest tile to be selected", |rig| {
        selected(rig) == [c.clone()]
    });
    assert!(!rig.app.shell.gallery_mut().has_item(&b));
    assert!(rig.app.menu.deferred.is_none());
}

#[test]
fn a_deferred_reload_is_dropped_when_another_folder_is_shown() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let other_dir = new_dir();
    let other = sidebar::normalize(&other_dir.0);
    write_png(&other.join("o.png"));
    rig.app.thumbs.navigate(other.clone());
    let ctx = rig.ctx.clone();
    rig.app
        .finish_delete(&ctx, vec![dir.join("b.png")], Vec::new(), Some(&dir));
    assert!(rig.app.menu.deferred.is_some());

    let last = other.join("o.png");
    frames_until(&mut rig, "the other folder to open", |rig| {
        rig.app.shell.gallery_mut().has_item(&last)
    });
    rig.frame(Vec::new());
    assert!(rig.app.menu.deferred.is_none());
    assert!(rig.app.menu.pending.is_none());
    assert!(!rig.app.thumbs.is_opening());
    assert_eq!(rig.app.shell.gallery_mut().folder(), Some(other.as_path()));
}

#[test]
fn a_delete_that_ends_during_a_folder_switch_does_not_cancel_the_switch() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let other_dir = new_dir();
    let other = sidebar::normalize(&other_dir.0);
    write_png(&other.join("o.png"));
    rig.app.thumbs.navigate(other.clone());
    assert_eq!(rig.app.thumbs.loading_folder(), Some(other.as_path()));

    let ctx = rig.ctx.clone();
    rig.app
        .finish_delete(&ctx, vec![dir.join("b.png")], Vec::new(), Some(&dir));

    assert_eq!(rig.app.thumbs.loading_folder(), Some(other.as_path()));
    assert!(rig.app.menu.pending.is_none());
    let last = other.join("o.png");
    frames_until(&mut rig, "the other folder to open", |rig| {
        rig.app.shell.gallery_mut().has_item(&last)
    });
    assert_eq!(rig.app.shell.gallery_mut().folder(), Some(other.as_path()));
}
