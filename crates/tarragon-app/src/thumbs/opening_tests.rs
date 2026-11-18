use super::tests::Rig;
use super::*;
use tarragon_ui::gallery::EmptyKind;

#[test]
fn a_new_thumbs_is_not_opening() {
    let rig = Rig::new("opening-new", None);
    assert!(!rig.thumbs.is_opening());
}

#[test]
fn a_waiting_folder_is_opening_and_the_gallery_shows_no_empty_card() {
    let mut rig = Rig::new("opening-wait", None);
    let folder = rig.folder("one", &["a.png"]);
    rig.thumbs.navigate(folder);
    assert!(rig.thumbs.is_opening());
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    assert!(matches!(rig.thumbs.slot, Slot::Opening { .. }));
    assert!(rig.thumbs.is_opening());
    assert_eq!(rig.gallery.empty_kind(), None);
}

#[test]
fn the_flag_drops_when_the_open_ends() {
    let mut rig = Rig::new("opening-end", Some("one"));
    rig.run_until_idle();
    assert!(!rig.thumbs.is_opening());
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    assert_eq!(rig.gallery.visible_len(), 1);
    assert!(!rig.thumbs.is_opening());
}

#[test]
fn an_empty_folder_shows_its_card_after_the_open() {
    let mut rig = Rig::new("opening-empty", None);
    let folder = rig.folder("none", &[]);
    rig.thumbs.navigate(folder);
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    assert_eq!(rig.gallery.empty_kind(), None);
    rig.run_until_idle();
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    assert_eq!(rig.gallery.empty_kind(), Some(EmptyKind::EmptyFolder));
}

#[test]
fn a_wanted_library_is_opening() {
    let mut rig = Rig::new("opening-library", None);
    rig.thumbs.show_library();
    assert!(rig.thumbs.is_opening());
}

#[test]
fn a_purge_is_not_an_open() {
    let mut rig = Rig::new("opening-purge", None);
    rig.thumbs.skip_library();
    rig.thumbs.purge(rig.dir.0.join("data"));
    rig.thumbs.before_draw(&rig.ctx, &mut rig.gallery);
    assert!(matches!(rig.thumbs.slot, Slot::Working { .. }));
    assert!(!rig.thumbs.is_opening());
    assert_eq!(rig.gallery.empty_kind(), Some(EmptyKind::NoFolder));
}
