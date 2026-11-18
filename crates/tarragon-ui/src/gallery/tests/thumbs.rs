//! Tests for the thumbnail setters and the visible-set report.

use super::*;
use crate::grid::TileError;
use egui::{Color32, ColorImage, TextureOptions};

/// A small texture from `ctx`.
fn texture(ctx: &egui::Context) -> egui::TextureHandle {
    ctx.load_texture(
        "t",
        ColorImage::filled([2, 2], Color32::RED),
        TextureOptions::NEAREST,
    )
}

/// A gallery over the four [`items`], with the first two cells selected.
fn selected_gallery() -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_scope(Scope::All);
    gallery.set_items(items());
    gallery
        .selection
        .apply_click(0, &gallery.visible, Modifiers::NONE);
    gallery
        .selection
        .apply_click(1, &gallery.visible, Modifiers::COMMAND);
    gallery
}

#[test]
fn setters_keep_the_selection() {
    let ctx = egui::Context::default();
    let mut gallery = selected_gallery();
    let before = gallery.selection.len();
    assert_eq!(before, 2);
    let key = PathBuf::from("/images/a.png");

    assert!(gallery.set_texture(&key, texture(&ctx)));
    assert_eq!(gallery.selection.len(), before);
    assert!(gallery.mark_failed(&key, TileError::Decode));
    assert_eq!(gallery.selection.len(), before);
    assert!(gallery.clear_texture(&key));
    assert_eq!(gallery.selection.len(), before);
}

#[test]
fn an_unknown_key_is_a_no_op() {
    let ctx = egui::Context::default();
    let mut gallery = selected_gallery();
    let key = PathBuf::from("/nowhere/x.png");
    assert!(!gallery.set_texture(&key, texture(&ctx)));
    assert!(!gallery.clear_texture(&key));
    assert!(!gallery.mark_failed(&key, TileError::Read));
    assert_eq!(gallery.selection.len(), 2);
    assert!(
        gallery
            .items
            .iter()
            .all(|i| i.texture.is_none() && i.error.is_none())
    );
}

/// Whether the named item has a texture, and its error.
fn state(gallery: &Gallery, name: &str) -> (bool, Option<TileError>) {
    let item = &gallery.items[gallery.index[&PathBuf::from("/images").join(name)]];
    (item.texture.is_some(), item.error)
}

#[test]
fn the_setters_change_only_the_named_item() {
    let ctx = egui::Context::default();
    let mut gallery = selected_gallery();
    let key = PathBuf::from("/images/a 2.png");
    gallery.set_texture(&key, texture(&ctx));
    gallery.mark_failed(&PathBuf::from("/images/b.png"), TileError::Other);
    assert_eq!(state(&gallery, "a 2.png"), (true, None));
    assert_eq!(state(&gallery, "b.png"), (false, Some(TileError::Other)));
    assert_eq!(state(&gallery, "a.png"), (false, None));
    gallery.set_texture(&PathBuf::from("/images/b.png"), texture(&ctx));
    assert_eq!(
        state(&gallery, "b.png"),
        (true, None),
        "a texture clears the failure"
    );
}

#[test]
fn set_items_rebuilds_the_index() {
    let mut gallery = selected_gallery();
    gallery.set_items(items_in_two_folders());
    assert!(!gallery.mark_failed(&PathBuf::from("/images/a.png"), TileError::Read));
    assert!(gallery.mark_failed(&PathBuf::from("/library/renders/c.png"), TileError::Read));
}

/// A gallery of many files in one folder.
fn long_gallery() -> Gallery {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    let list = (0..400)
        .map(|n| GridItem::new(PathBuf::from(format!("/images/f{n:04}.png")), n, None))
        .collect();
    gallery.set_items(list);
    gallery
}

#[test]
fn visible_keys_change_with_the_scroll_and_stay_equal_otherwise() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = long_gallery();
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    let first: Vec<PathBuf> = gallery.visible_keys().map(Path::to_path_buf).collect();
    let revision = gallery.visible_revision();
    assert!(first.len() > 3, "the grid painted too few tiles");
    assert!(first.len() < 400, "the grid painted every item");

    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert_eq!(
        gallery.visible_revision(),
        revision,
        "a quiet frame bumped it"
    );

    let pos = Pos2::new(600.0, 400.0);
    let wheel = Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -1500.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    };
    frame(&ctx, &mut gallery, vec![Event::PointerMoved(pos), wheel]).drop_without_applying_deltas();
    for _ in 0..5 {
        frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    }
    let scrolled: Vec<PathBuf> = gallery.visible_keys().map(Path::to_path_buf).collect();
    assert_ne!(scrolled, first, "the scroll did not move the set");
    assert!(gallery.visible_revision() > revision);
}

#[test]
fn the_first_item_wins_for_a_duplicate_path() {
    let mut gallery = Gallery::new(SortMode::Natural);
    let twin = || GridItem::new(PathBuf::from("/images/same.png"), 1, None);
    gallery.set_items(vec![twin(), twin()]);
    assert!(gallery.mark_failed(Path::new("/images/same.png"), TileError::Other));
    assert_eq!(gallery.items[0].error, Some(TileError::Other));
    assert_eq!(gallery.items[1].error, None);
}
