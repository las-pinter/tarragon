//! Tests for the Preview pane with a multi-selection.

use super::preview_tests::painted_pictures;
use crate::preview::{ColorState, PreviewImage, TagPill};
use crate::shell::Shell;
use crate::shell::tests::{input, leaf_rect, painted_text};
use crate::theme::apply_theme;
use egui::{Context, TextureId};

fn images(count: u64) -> Vec<PreviewImage> {
    (0..count)
        .map(|i| PreviewImage {
            texture: TextureId::User(100 + i),
            size: (64, 48),
        })
        .collect()
}

/// Run two frames; return the pictures and the texts of the second.
fn settle(ctx: &Context, shell: &mut Shell) -> (Vec<(TextureId, egui::Rect)>, Vec<String>) {
    let mut last = (Vec::new(), Vec::new());
    for _ in 0..2 {
        let output = ctx.run_ui(input(), |ui| shell.show(ui));
        last = (painted_pictures(&output), painted_text(&output));
        output.drop_without_applying_deltas();
    }
    last
}

#[test]
fn a_multi_selection_paints_each_texture_once_inside_the_pane() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_mosaic(images(4), 4);
    let (found, text) = settle(&ctx, &mut shell);
    let pane = leaf_rect(&shell, crate::layout::Pane::Preview);
    let mut textures: Vec<_> = found.iter().map(|(t, _)| *t).collect();
    textures.sort_by_key(|t| format!("{t:?}"));
    assert_eq!(
        textures,
        images(4).iter().map(|i| i.texture).collect::<Vec<_>>()
    );
    for (_, rect) in &found {
        assert!(pane.contains_rect(*rect), "{rect:?} is outside {pane:?}");
    }
    assert!(!text.iter().any(|t| t.contains("more")), "{text:?}");
}

#[test]
fn the_mosaic_replaces_the_single_picture() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    let single = TextureId::User(7);
    shell.preview.set_image(Some(PreviewImage {
        texture: single,
        size: (64, 48),
    }));
    shell.preview.set_mosaic(images(2), 2);
    let (found, _) = settle(&ctx, &mut shell);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|(t, _)| *t != single));
}

#[test]
fn more_images_than_the_cap_show_the_cap_and_a_hint() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_mosaic(images(12), 12);
    let (found, text) = settle(&ctx, &mut shell);
    assert_eq!(found.len(), 9, "{found:?}");
    let mut spots: Vec<String> = found.iter().map(|(_, r)| format!("{:?}", r.min)).collect();
    spots.sort_unstable();
    spots.dedup();
    assert_eq!(spots.len(), 9, "cells overlap: {found:?}");
    assert!(text.iter().any(|t| t == "+3 more"), "{text:?}");

    shell.preview.set_max_multi_preview(4);
    let (found, text) = settle(&ctx, &mut shell);
    assert_eq!(found.len(), 4, "{found:?}");
    assert!(text.iter().any(|t| t == "+8 more"), "{text:?}");
}

#[test]
fn a_multi_selection_without_meta_still_shows_the_tags_area() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_mosaic(images(3), 3);
    shell.preview.set_tags(vec![TagPill {
        name: "hands".into(),
        state: ColorState::Some,
    }]);
    let (_, text) = settle(&ctx, &mut shell);
    for want in ["Tags", "hands", "+ add"] {
        assert!(
            text.iter().any(|t| t == want),
            "{want} is missing: {text:?}"
        );
    }
    assert!(!text.iter().any(|t| t == crate::preview::EMPTY_HINT));
    assert!(!text.iter().any(|t| t == "Metadata"), "{text:?}");
}

#[test]
fn a_single_image_in_the_mosaic_is_not_a_mosaic() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_mosaic(images(1), 1);
    let (found, text) = settle(&ctx, &mut shell);
    assert!(found.is_empty(), "{found:?}");
    assert!(text.iter().any(|t| t == crate::preview::EMPTY_HINT));
}

#[test]
fn the_hint_counts_the_whole_selection_when_few_pictures_are_ready() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_mosaic(images(9), 20);
    let (found, text) = settle(&ctx, &mut shell);
    assert_eq!(found.len(), 9);
    assert!(text.iter().any(|t| t == "+11 more"), "{text:?}");

    shell.preview.set_mosaic(images(3), 5);
    let (found, text) = settle(&ctx, &mut shell);
    assert_eq!(found.len(), 3);
    assert!(text.iter().any(|t| t == "+2 more"), "{text:?}");
}

#[test]
fn a_big_selection_with_one_picture_ready_shows_a_one_cell_mosaic() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview.set_mosaic(images(1), 3);
    let (found, text) = settle(&ctx, &mut shell);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(text.iter().any(|t| t == "+2 more"), "{text:?}");
    assert!(!text.iter().any(|t| t == crate::preview::EMPTY_HINT));
}
