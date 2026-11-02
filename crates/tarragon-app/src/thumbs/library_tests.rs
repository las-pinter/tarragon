use std::time::Instant;

use super::tests::Rig;
use super::*;

/// A rig that scanned folder `b`, then folder `a`, so `a` is shown and the
/// stored rows hold both.
fn rig_with_two_folders(name: &str) -> (Rig, PathBuf, PathBuf) {
    let mut rig = Rig::new(name, None);
    let a = rig.folder("a", &["a1.png", "a2.png"]);
    let b = rig.folder("b", &["b1.png"]);
    for folder in [&b, &a] {
        rig.thumbs.navigate(folder.clone());
        rig.settle();
    }
    assert_eq!(rig.thumbs.take_folder_change(), Some(a.clone()));
    (rig, a, b)
}

/// Run frames until nothing opens or waits and every tile has settled.
fn settle_library(rig: &mut Rig) {
    let start = Instant::now();
    while !rig.thumbs.is_idle() || !rig.thumbs.feed.all_settled() {
        rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
        assert!(start.elapsed() < Duration::from_secs(30), "the load hangs");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn show_library_fills_the_gallery_with_both_folders() {
    let (mut rig, a, b) = rig_with_two_folders("both");
    rig.thumbs.show_library();
    settle_library(&mut rig);
    assert!(rig.thumbs.library_loaded());
    for file in [a.join("a1.png"), a.join("a2.png"), b.join("b1.png")] {
        assert!(rig.gallery.has_item(&file), "{} is missing", file.display());
    }
}

#[test]
fn a_second_show_library_starts_no_open() {
    let (mut rig, _a, _b) = rig_with_two_folders("twice");
    rig.thumbs.show_library();
    settle_library(&mut rig);
    rig.thumbs.show_library();
    assert!(rig.thumbs.is_idle());
    assert!(matches!(rig.thumbs.slot, Slot::Idle(_)));
}

#[test]
fn show_library_while_it_loads_asks_for_nothing_more() {
    let (mut rig, _a, _b) = rig_with_two_folders("loading");
    rig.thumbs.show_library();
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    rig.thumbs.show_library();
    assert_eq!(rig.thumbs.library, Library::Absent);
}

#[test]
fn a_folder_open_after_the_library_clears_loaded() {
    let (mut rig, a, b) = rig_with_two_folders("reload");
    rig.thumbs.show_library();
    settle_library(&mut rig);
    rig.thumbs.navigate(b.clone());
    rig.run_until_idle();
    assert!(!rig.thumbs.library_loaded());
    assert_eq!(rig.gallery.folder(), Some(b.as_path()));
    assert!(!rig.gallery.has_item(&a.join("a1.png")));
}

#[test]
fn the_library_keeps_the_folder_and_the_highlight() {
    let (mut rig, a, _b) = rig_with_two_folders("folder");
    rig.thumbs.show_library();
    settle_library(&mut rig);
    assert_eq!(rig.gallery.folder(), Some(a.as_path()));
    assert_eq!(rig.thumbs.take_folder_change(), None);
    assert_eq!(rig.thumbs.current.as_deref(), Some(a.as_path()));
}

#[test]
fn a_texture_for_a_file_outside_the_first_folder_reaches_its_tile() {
    let (mut rig, _a, _b) = rig_with_two_folders("texture");
    rig.thumbs.show_library();
    settle_library(&mut rig);
    // Three tiles: the feed was reset for the library, and every one settled.
    assert_eq!(rig.thumbs.feed.counts(), (3, 0));
    assert!(rig.thumbs.feed.all_settled());
}

#[test]
fn a_folder_request_wins_over_a_library_request() {
    let (mut rig, _a, b) = rig_with_two_folders("wins");
    rig.thumbs.show_library();
    rig.thumbs.navigate(b.clone());
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    assert!(matches!(
        &rig.thumbs.slot,
        Slot::Opening { target: Target::Folder(folder), .. } if folder == &b
    ));
    rig.run_until_idle();
    assert!(!rig.thumbs.library_loaded());
    assert_eq!(rig.gallery.folder(), Some(b.as_path()));
}

#[test]
fn a_folder_request_during_a_library_open_opens_after_it() {
    let (mut rig, _a, b) = rig_with_two_folders("after");
    rig.thumbs.show_library();
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    rig.thumbs.navigate(b.clone());
    rig.run_until_idle();
    assert_eq!(rig.gallery.folder(), Some(b.as_path()));
    assert!(!rig.thumbs.library_loaded());
}

/// Hide the stored rows, so reading the library fails.
fn set_rows_aside(rig: &Rig, hidden: bool) {
    let (from, to) = if hidden {
        ("thumbnails", "thumbnails_aside")
    } else {
        ("thumbnails_aside", "thumbnails")
    };
    rusqlite::Connection::open(rig.dir.0.join("t.db"))
        .expect("open the db file")
        .execute_batch(&format!("ALTER TABLE {from} RENAME TO {to};"))
        .expect("rename the table");
}

#[test]
fn a_failed_library_open_keeps_the_gallery_and_asks_no_more() {
    let (mut rig, a, b) = rig_with_two_folders("failed");
    set_rows_aside(&rig, true);
    rig.thumbs.show_library();
    rig.run_until_idle();
    assert!(!rig.thumbs.library_loaded());
    assert_eq!(rig.thumbs.library, Library::Failed);
    assert_eq!(rig.gallery.folder(), Some(a.as_path()));
    assert!(rig.gallery.has_item(&a.join("a1.png")));
    assert!(!rig.gallery.has_item(&b.join("b1.png")));
    rig.thumbs.show_library();
    assert!(rig.thumbs.is_idle());
    assert!(matches!(rig.thumbs.slot, Slot::Idle(_)));
}

#[test]
fn a_folder_open_clears_the_failed_latch() {
    let (mut rig, _a, b) = rig_with_two_folders("latch");
    set_rows_aside(&rig, true);
    rig.thumbs.show_library();
    rig.run_until_idle();
    assert_eq!(rig.thumbs.library, Library::Failed);
    set_rows_aside(&rig, false);
    rig.thumbs.navigate(b);
    assert_eq!(rig.thumbs.library, Library::Absent);
    rig.run_until_idle();
    rig.thumbs.show_library();
    settle_library(&mut rig);
    assert!(rig.thumbs.library_loaded());
}

#[test]
fn a_successful_folder_open_clears_the_failed_latch() {
    let (mut rig, _a, _b) = rig_with_two_folders("latch-ok");
    rig.thumbs.library = Library::Failed;
    let c = rig.folder("c", &["c1.png"]);
    rig.thumbs.pending = Some(c);
    rig.run_until_idle();
    assert_eq!(rig.thumbs.library, Library::Absent);
}
