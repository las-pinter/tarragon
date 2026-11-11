//! Tests for the menu choices that write files. Every file lives in a temp
//! folder, and no test runs a real clipboard program.

use std::time::{Duration, Instant};

use image::{Rgba, RgbaImage};
use tarragon_ui::gallery::GridMenuEvent;
use tarragon_ui::grid::GridItem;

use super::*;
use crate::app::preview_tests::Rig;
use crate::app::tests::new_dir;
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
/// The folder comes back with it.
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

#[test]
fn a_rename_moves_the_file_and_selects_the_new_name_after_the_reload() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let event = GridMenuEvent::RenameTo {
        path: dir.join("a.png"),
        new_stem: "z".to_owned(),
    };
    rig.app.apply_menu_event(&rig.ctx, event);
    assert!(!dir.join("a.png").exists());
    assert!(dir.join("z.png").exists());

    let new = dir.join("z.png");
    frames_until(&mut rig, "the new name to be selected", |rig| {
        selected(rig) == [new.clone()]
    });
    assert!(rig.app.shell.gallery_mut().has_item(&new));
    assert!(!rig.app.shell.gallery_mut().has_item(&dir.join("a.png")));
    let write = rig.app.menu.last_write.clone().expect("the write is kept");
    assert_eq!(write.renamed, Some((dir.join("a.png"), new)));
    assert_eq!(write.copied, Vec::new());
}

#[test]
fn a_rename_onto_an_existing_name_shows_the_error_and_changes_nothing() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    let event = GridMenuEvent::RenameTo {
        path: dir.join("a.png"),
        new_stem: "b".to_owned(),
    };
    rig.app.apply_menu_event(&rig.ctx, event);
    wait_for_text(&mut rig, "Cannot rename");
    assert!(dir.join("a.png").exists());
    assert!(dir.join("b.png").exists());
    assert!(rig.app.menu.last_write.is_none());
    assert!(rig.app.menu.pending.is_none());
}

#[test]
fn a_duplicate_of_three_files_selects_the_three_copies_after_the_reload() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png", "c.png"]);
    let paths = ["a.png", "b.png", "c.png"]
        .map(|name| dir.join(name))
        .to_vec();
    rig.app
        .apply_menu_event(&rig.ctx, GridMenuEvent::Duplicate(paths));

    let mut copies: Vec<PathBuf> = ["a copy.png", "b copy.png", "c copy.png"]
        .map(|name| dir.join(name))
        .to_vec();
    copies.sort();
    frames_until(&mut rig, "the copies to be selected", |rig| {
        selected(rig) == copies
    });
    for copy in &copies {
        assert!(copy.exists(), "{}", copy.display());
    }
    let write = rig.app.menu.last_write.clone().expect("the write is kept");
    let mut made: Vec<PathBuf> = write.copied.iter().map(|c| c.path.clone()).collect();
    made.sort();
    assert_eq!(made, copies);
    assert!(write.copied.iter().all(|c| c.len > 0));
}

#[test]
fn a_paste_copies_the_image_and_counts_the_skipped_text_file() {
    let source = new_dir();
    let target = new_dir();
    let image = source.0.join("pic.png");
    let text = source.0.join("note.txt");
    write_png(&image);
    std::fs::write(&text, "words").expect("write the text file");

    let outcome = paste_outcome(&target.0, || Ok(vec![image.clone(), text.clone()]));

    assert_eq!(outcome.notice.as_deref(), Some("Pasted 1 file, skipped 1"));
    assert!(target.0.join("pic.png").exists());
    assert!(!target.0.join("note.txt").exists());
    let paths: Vec<PathBuf> = outcome
        .write
        .copied
        .iter()
        .map(|c| c.path.clone())
        .collect();
    assert_eq!(paths, [target.0.join("pic.png")]);
}

#[test]
fn a_paste_skips_a_folder_and_a_missing_file() {
    let source = new_dir();
    let target = new_dir();
    let outcome = paste_outcome(&target.0, || {
        Ok(vec![source.0.clone(), source.0.join("gone.png")])
    });
    assert_eq!(outcome.notice.as_deref(), Some("Pasted 0 files, skipped 2"));
    assert_eq!(outcome.write.copied, Vec::new());
}

#[test]
fn the_paste_notices_name_the_cause_of_a_failure() {
    let target = new_dir();
    let missing = paste_outcome(&target.0, || Err(ClipboardError::ToolMissing("xclip")));
    assert_eq!(
        missing.notice.as_deref(),
        Some("Install xclip to paste files")
    );
    let empty = paste_outcome(&target.0, || Ok(Vec::new()));
    assert_eq!(
        empty.notice.as_deref(),
        Some("The clipboard holds no files")
    );
    let timeout = paste_outcome(&target.0, || Err(ClipboardError::Timeout));
    assert!(timeout.notice.is_some_and(|text| text.contains("in time")));
    assert_eq!(
        paste_text(2, 0, Some(("boom".to_owned(), 1))),
        "Pasted 2 files, 1 failed: boom"
    );
}

#[test]
fn a_failed_duplicate_gives_a_notice_and_no_copy() {
    let dir = new_dir();
    let outcome = duplicate_outcome(&[dir.0.join("gone.png")], None);
    assert_eq!(outcome.write.copied, Vec::new());
    let notice = outcome.notice.expect("a notice");
    assert!(notice.starts_with("Cannot duplicate: "), "{notice}");
}

#[test]
fn a_paste_only_starts_for_the_shown_folder() {
    let mut rig = Rig::new(&[], &[], 9);
    let dir = rig.dir.0.clone();
    // The library view, with a folder set: the paste does nothing.
    rig.app.shell.gallery_mut().set_folder(Some(dir.clone()));
    rig.app.paste_with(&rig.ctx.clone(), || Ok(Vec::new()));
    assert!(rig.app.menu.writing.is_none());
    // A folder scope with no folder: the paste does nothing.
    rig.app.shell.gallery_mut().set_folder(None);
    rig.app.shell.gallery_mut().set_scope(Scope::Folder);
    rig.app.paste_with(&rig.ctx.clone(), || Ok(Vec::new()));
    assert!(rig.app.menu.writing.is_none());
    // A shown folder: the paste starts.
    rig.app.shell.gallery_mut().set_folder(Some(dir));
    rig.app.paste_with(&rig.ctx.clone(), || {
        Err(ClipboardError::ToolMissing("xclip"))
    });
    assert!(rig.app.menu.writing.is_some());
    wait_for_text(&mut rig, "Install xclip to paste files");
    assert!(rig.app.menu.writing.is_none());
}

#[test]
fn a_write_does_not_block_the_caller_and_a_second_request_gets_a_notice() {
    let mut rig = Rig::new(&[], &[], 9);
    let (release, wait) = crossbeam_channel::bounded::<()>(1);
    let start = Instant::now();
    rig.app.start_write(&rig.ctx.clone(), move || {
        let _ = wait.recv_timeout(Duration::from_secs(3));
        Outcome::default()
    });
    let returned_in = start.elapsed();
    // No path, so a duplicate that ran by mistake would copy nothing.
    rig.app
        .apply_menu_event(&rig.ctx, GridMenuEvent::Duplicate(Vec::new()));
    let _ = release.send(());
    assert!(
        returned_in < Duration::from_secs(1),
        "start_write took {returned_in:?}"
    );
    wait_for_text(&mut rig, "Another file task is running");
}

#[test]
fn a_rename_waits_with_a_notice_while_a_write_runs() {
    let (mut rig, dir) = folder_rig(&["a.png"]);
    let (release, wait) = crossbeam_channel::bounded::<()>(1);
    rig.app.start_write(&rig.ctx.clone(), move || {
        let _ = wait.recv_timeout(Duration::from_secs(3));
        Outcome::default()
    });
    let event = GridMenuEvent::RenameTo {
        path: dir.join("a.png"),
        new_stem: "z".to_owned(),
    };
    rig.app.apply_menu_event(&rig.ctx, event);
    let _ = release.send(());
    assert!(dir.join("a.png").exists());
    wait_for_text(&mut rig, "Another file task is running");
}

#[test]
fn a_rename_that_ends_while_the_same_folder_loads_shows_the_new_name_after_that_load() {
    let (mut rig, dir) = folder_rig(&["a.png", "b.png"]);
    rig.app.thumbs.reload_folder(dir.clone());
    assert!(rig.app.thumbs.is_opening());
    let event = GridMenuEvent::RenameTo {
        path: dir.join("a.png"),
        new_stem: "z".to_owned(),
    };
    rig.app.apply_menu_event(&rig.ctx, event);
    assert!(rig.app.menu.deferred.is_some());

    let new = dir.join("z.png");
    frames_until(&mut rig, "the new name to be selected", |rig| {
        selected(rig) == [new.clone()]
    });
    assert!(!rig.app.shell.gallery_mut().has_item(&dir.join("a.png")));
}

#[test]
fn a_confirmed_delete_waits_with_a_notice_while_a_write_runs() {
    let (mut rig, dir) = folder_rig(&["a.png"]);
    let (release, wait) = crossbeam_channel::bounded::<()>(1);
    rig.app.start_write(&rig.ctx.clone(), move || {
        let _ = wait.recv_timeout(Duration::from_secs(3));
        Outcome::default()
    });
    let event = GridMenuEvent::ConfirmedDelete {
        paths: vec![dir.join("a.png")],
        permanent: true,
    };
    rig.app.apply_menu_event(&rig.ctx, event);
    let _ = release.send(());
    assert!(dir.join("a.png").exists());
    wait_for_text(&mut rig, "Another file task is running");
    assert!(dir.join("a.png").exists());
}

/// An app that shows `dir` and waits for a load, with the pending selection of
/// `new.png`.
fn pending_rig() -> (Rig, PathBuf) {
    let mut rig = Rig::new(&[], &[], 9);
    let dir = sidebar::normalize(&rig.dir.0);
    let gallery = rig.app.shell.gallery_mut();
    gallery.set_scope(Scope::Folder);
    gallery.set_folder(Some(dir.clone()));
    gallery.set_items(vec![GridItem::new(dir.join("old.png"), 1, None)]);
    let new = dir.join("new.png");
    rig.app.thumbs.reload_folder(dir.clone());
    rig.app.menu.pending = Some(PendingSelect {
        folder: Some(dir.clone()),
        paths: vec![new],
        near: false,
    });
    (rig, dir)
}

#[test]
fn the_selection_is_held_until_the_new_list_lands() {
    let (mut rig, dir) = pending_rig();
    rig.app.apply_pending_select(&rig.ctx.clone());
    assert!(rig.app.menu.pending.is_some());
    assert_eq!(selected(&mut rig), Vec::<PathBuf>::new());

    let new = dir.join("new.png");
    let items = vec![
        GridItem::new(dir.join("old.png"), 1, None),
        GridItem::new(new.clone(), 1, None),
    ];
    rig.app.shell.gallery_mut().set_items(items);
    rig.app.apply_pending_select(&rig.ctx.clone());
    assert!(rig.app.menu.pending.is_none());
    assert_eq!(selected(&mut rig), [new]);
}

#[test]
fn the_held_selection_is_dropped_when_the_user_opens_another_folder() {
    let (mut rig, dir) = pending_rig();
    // The write was for another folder than the one that is shown now, and the
    // shown list holds the file.
    let items = vec![GridItem::new(dir.join("new.png"), 1, None)];
    rig.app.shell.gallery_mut().set_items(items);
    if let Some(pending) = rig.app.menu.pending.as_mut() {
        pending.folder = Some(dir.join("other"));
    }
    rig.app.apply_pending_select(&rig.ctx.clone());
    assert!(rig.app.menu.pending.is_none());
    assert_eq!(selected(&mut rig), Vec::<PathBuf>::new());
}

#[test]
fn a_write_that_ends_during_a_folder_switch_does_not_reload_the_old_folder() {
    let (mut rig, dir) = folder_rig(&["a.png"]);
    let other_dir = new_dir();
    let other = sidebar::normalize(&other_dir.0);
    write_png(&other.join("o.png"));
    // The user asks for another folder; it waits to open, and the gallery still
    // shows the old folder.
    rig.app.thumbs.navigate(other.clone());
    assert_eq!(rig.app.thumbs.loading_folder(), Some(other.as_path()));
    assert_eq!(rig.app.shell.gallery_mut().folder(), Some(dir.as_path()));

    let write = FileWrite {
        copied: Vec::new(),
        renamed: Some((dir.join("a.png"), dir.join("z.png"))),
    };
    let ctx = rig.ctx.clone();
    rig.app.finish_write(&ctx, write, None, Some(&dir));

    // The switch wins: the pending folder is still the other one, and no
    // selection is held for the old folder.
    assert_eq!(rig.app.thumbs.loading_folder(), Some(other.as_path()));
    assert!(rig.app.menu.pending.is_none());
    let last = other.join("o.png");
    frames_until(&mut rig, "the other folder to open", |rig| {
        rig.app.shell.gallery_mut().has_item(&last)
    });
    assert_eq!(rig.app.shell.gallery_mut().folder(), Some(other.as_path()));
}

#[test]
fn a_write_for_another_folder_than_the_shown_one_does_not_reload() {
    let (mut rig, dir) = folder_rig(&["a.png"]);
    let ctx = rig.ctx.clone();
    let write = FileWrite {
        copied: Vec::new(),
        renamed: Some((dir.join("a.png"), dir.join("z.png"))),
    };
    let elsewhere = dir.join("elsewhere");
    rig.app.finish_write(&ctx, write, None, Some(&elsewhere));
    assert!(rig.app.menu.pending.is_none());
    assert!(!rig.app.thumbs.is_opening());
}

#[test]
fn in_the_library_view_a_paste_does_nothing_and_a_duplicate_copies_beside_each_file() {
    let (mut rig, dir) = folder_rig(&["a.png"]);
    rig.app.shell.gallery_mut().set_scope(Scope::All);
    let ctx = rig.ctx.clone();
    // `reload_library` acts only on a loaded library, so load it first.
    rig.app.thumbs.show_library();
    frames_until(&mut rig, "the library to load", |rig| {
        rig.app.thumbs.library_loaded() && !rig.app.thumbs.is_opening()
    });

    // Paste has no one target folder here, so no write starts.
    rig.app
        .paste_with(&ctx, || panic!("the clipboard must not be read"));
    assert!(rig.app.menu.writing.is_none());

    // Duplicate copies each file into its own folder.
    let source = dir.join("a.png");
    rig.app
        .apply_menu_event(&ctx, GridMenuEvent::Duplicate(vec![source]));
    assert!(rig.app.menu.writing.is_some());
    let copy = dir.join("a copy.png");
    // The write ends in a frame, which asks the library to load again and holds
    // the copies for selection.
    let mut reloaded = false;
    frames_until(&mut rig, "the duplicate to end", |rig| {
        if rig.app.menu.last_write.is_some() {
            reloaded = rig.app.thumbs.is_opening() && rig.app.menu.pending.is_some();
            return true;
        }
        false
    });
    assert!(reloaded, "the library did not load again");
    assert!(copy.exists());
    let write = rig.app.menu.last_write.clone().expect("the write is kept");
    assert_eq!(write.copied.len(), 1);
    assert_eq!(write.copied[0].path, copy);
}

#[test]
fn the_held_selection_is_dropped_when_the_load_ends_without_the_files() {
    let mut rig = Rig::new(&[], &[], 9);
    let dir = sidebar::normalize(&rig.dir.0);
    let gallery = rig.app.shell.gallery_mut();
    gallery.set_scope(Scope::Folder);
    gallery.set_folder(Some(dir.clone()));
    // No load runs, so no list can still bring the file.
    rig.app.menu.pending = Some(PendingSelect {
        folder: Some(dir.clone()),
        paths: vec![dir.join("new.png")],
        near: false,
    });
    rig.app.apply_pending_select(&rig.ctx.clone());
    assert!(rig.app.menu.pending.is_none());
}
