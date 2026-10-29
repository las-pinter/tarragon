//! Tests for the pill hover look, the cursor, the cross edge, and narrow panes.

use super::{
    Painted, body_pos, click_at, cross_pos, ctx_with_fonts, ctx_with_fonts_at, first_pill, frame,
    pill, preview,
};
use crate::preview::{ColorState, Preview, PreviewEvent, TagPill};
use crate::tokens;
use egui::{Context, Event, Pos2, Rect};

/// Hover `at` for one frame, then paint a second frame under the pointer.
fn hover_frame(ctx: &Context, preview: &mut Preview, at: Pos2) -> Painted {
    frame(ctx, preview, 400.0, vec![Event::PointerMoved(at)]);
    frame(ctx, preview, 400.0, vec![])
}

/// The left edge of the cross, read from where the hovered pill paints it.
fn cross_left(ctx: &Context, preview: &mut Preview, rect: Rect) -> f32 {
    let found = hover_frame(ctx, preview, body_pos(rect));
    found
        .texts
        .iter()
        .find(|(t, ..)| t == "x")
        .expect("the x is painted on hover")
        .2
        .x
}

#[test]
fn the_x_is_bright_under_the_pointer_and_muted_beside_it() {
    let mut preview = preview(vec![pill("blue", ColorState::All)]);
    let ctx = ctx_with_fonts(&mut preview);
    let rect = first_pill(&ctx, &mut preview);
    let x_color = |found: &Painted| found.texts.iter().find(|(t, ..)| t == "x").map(|t| t.1);
    let off = hover_frame(&ctx, &mut preview, body_pos(rect));
    assert_eq!(x_color(&off), Some(tokens::CORAL_MUTED));
    let on = hover_frame(&ctx, &mut preview, cross_pos(rect));
    assert_eq!(x_color(&on), Some(tokens::CORAL_STRONG));
}

#[test]
fn the_cross_edge_splits_apply_from_remove() {
    let mut preview = preview(vec![pill("blue", ColorState::Some)]);
    let ctx = ctx_with_fonts(&mut preview);
    let rect = first_pill(&ctx, &mut preview);
    let edge = cross_left(&ctx, &mut preview, rect);
    let y = rect.center().y;
    frame(
        &ctx,
        &mut preview,
        400.0,
        click_at(Pos2::new(edge - 0.5, y)),
    );
    assert_eq!(
        preview.take_events(),
        [PreviewEvent::TagApplied("blue".into())]
    );
    frame(
        &ctx,
        &mut preview,
        400.0,
        click_at(Pos2::new(edge + 0.5, y)),
    );
    assert_eq!(
        preview.take_events(),
        [PreviewEvent::TagRemoved("blue".into())]
    );
}

#[test]
fn the_hand_cursor_shows_only_where_a_click_does_something() {
    let mut preview = preview(vec![
        pill("full", ColorState::All),
        pill("part", ColorState::Some),
    ]);
    let ctx = ctx_with_fonts(&mut preview);
    let found = frame(&ctx, &mut preview, 400.0, vec![]);
    let (full, part) = (found.pills[0].0, found.pills[1].0);
    let hand = egui::CursorIcon::PointingHand;
    assert_ne!(hover_frame(&ctx, &mut preview, body_pos(full)).cursor, hand);
    assert_eq!(
        hover_frame(&ctx, &mut preview, cross_pos(full)).cursor,
        hand
    );
    assert_eq!(hover_frame(&ctx, &mut preview, body_pos(part)).cursor, hand);
}

#[test]
fn pills_below_the_width_of_the_squares_still_lay_out_inside_the_content() {
    let tags: Vec<TagPill> = (0..12)
        .map(|i| pill(&format!("tag-number-{i}"), ColorState::Some))
        .collect();
    let mut preview = preview(tags);
    let width = 200.0;
    let ctx = ctx_with_fonts_at(&mut preview, width);
    let found = frame(&ctx, &mut preview, width, vec![]);
    assert_eq!(found.pills.len(), 12);
    // The squares do not wrap, so the scroll content is as wide as they are.
    let content = 12.0 * crate::swatch::SIZE + 11.0 * tokens::SPACING_XS;
    for (rect, _) in &found.pills {
        assert!(rect.max.x <= content + 0.01, "{rect:?} leaves the content");
    }
}
