//! Tests for the tile states: selection, multi-select check, and hover.

use super::tests::{draw_with, input, item, painted_texts, rects};
use super::*;
use egui::{Context, Event};

#[test]
fn a_selected_tile_outlines_the_thumbnail_three_points_out() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let mut selection = Selection::default();
    selection.select_only(0);
    let output = draw_with(&ctx, input(Vec::new()), &[item("sketch.psd")], &selection);
    let thumb = ctx
        .read_response(Id::new(("tarragon-grid-tile", 0)))
        .map(|r| thumbnail_rect(r.rect))
        .expect("the tile registered");
    let outline = rects(&output)
        .into_iter()
        .find(|r| r.stroke.color == tokens::CORAL_BRIGHT);
    output.drop_without_applying_deltas();
    let outline = outline.expect("the selected tile painted no coral outline");
    // The stroke centre sits 3 points out plus half the 2 point width.
    assert_eq!(outline.rect, thumb.expand(4.0));
    assert_eq!(outline.stroke.width, 2.0);
    assert_eq!(outline.stroke_kind, StrokeKind::Middle);
}

#[test]
fn a_multi_selection_paints_the_check_badge() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let items = [item("a.png"), item("b.png")];
    let mut selection = Selection::default();
    selection.select_only(0);
    selection.toggle(1);

    let output = draw_with(&ctx, input(Vec::new()), &items, &selection);
    let checks = output
        .shapes
        .iter()
        .filter(|clipped| match &clipped.shape {
            epaint::Shape::Circle(circle) => {
                circle.fill == tokens::CORAL_BRIGHT
                    && (circle.radius - CHECK_SIZE / 2.0).abs() < 0.5
            }
            _ => false,
        })
        .count();
    let marks = output
        .shapes
        .iter()
        .filter(|clipped| {
            matches!(&clipped.shape, epaint::Shape::Path(path)
                if path.points.len() == 3 && path.stroke.color == epaint::ColorMode::Solid(tokens::BG_PRIMARY))
        })
        .count();
    output.drop_without_applying_deltas();
    assert_eq!(checks, 2, "each selected tile paints one check badge");
    assert_eq!(marks, 2, "each badge holds a dark check");
    assert_eq!(CHECK_SIZE, 22.0);
}

#[test]
fn one_selected_tile_paints_no_check_badge() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let items = [item("a.png")];
    let mut selection = Selection::default();
    selection.select_only(0);

    let output = draw_with(&ctx, input(Vec::new()), &items, &selection);
    let checks = output
        .shapes
        .iter()
        .filter(|clipped| {
            matches!(&clipped.shape, epaint::Shape::Circle(circle) if circle.fill == tokens::CORAL_BRIGHT)
        })
        .count();
    output.drop_without_applying_deltas();
    assert_eq!(checks, 0, "one selected tile needs no check badge");
}

/// Draw `items`, then one frame with the pointer over the first tile, and
/// return that frame.
fn hovered_frame(ctx: &Context, items: &[GridItem]) -> egui::FullOutput {
    let selection = Selection::default();
    draw_with(ctx, input(Vec::new()), items, &selection).drop_without_applying_deltas();
    let pos = ctx
        .read_response(Id::new(("tarragon-grid-tile", 0)))
        .expect("the tile registered")
        .rect
        .center();
    draw_with(
        ctx,
        input(vec![Event::PointerMoved(pos)]),
        items,
        &selection,
    )
}

#[test]
fn hover_outlines_the_tile_at_once_with_no_shadow() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let output = hovered_frame(&ctx, &[item("a.png")]);
    let found = rects(&output);
    output.drop_without_applying_deltas();
    let outline = found
        .iter()
        .find(|r| r.stroke.color == tokens::SURFACE_HIGHLIGHT);
    assert!(
        outline.is_some(),
        "the first hovered frame paints the outline"
    );
    assert!(found.iter().all(|r| r.blur_width == 0.0), "no shadow");
}

#[test]
fn hover_shows_the_pixel_size_and_the_file_size() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let known = item("a.png").with_meta(2_516_582, (2048, 1536));
    let output = hovered_frame(&ctx, &[known]);
    let texts = painted_texts(&output);
    output.drop_without_applying_deltas();
    assert!(
        texts.iter().any(|(t, _)| t == "2048 × 1536 · 2.4 MB"),
        "{texts:?}"
    );
}

#[test]
fn the_info_text_drops_an_unknown_pixel_size() {
    assert_eq!(info_text(&item("a.png").with_meta(142, (0, 48))), "142 B");
    assert_eq!(
        info_text(&item("a.png").with_meta(142, (64, 48))),
        "64 × 48 · 142 B"
    );
}

#[test]
fn a_tile_at_rest_shows_no_info_bar_and_no_outline() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let known = item("a.png").with_meta(142, (64, 48));
    let output = draw_with(&ctx, input(Vec::new()), &[known], &Selection::default());
    let texts = painted_texts(&output);
    let outlined = rects(&output).iter().any(|r| r.stroke.width > 0.0);
    output.drop_without_applying_deltas();
    assert!(!texts.iter().any(|(t, _)| t.contains("142 B")));
    assert!(!outlined);
}

#[test]
fn a_selected_name_turns_light_coral_and_medium() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let mut selection = Selection::default();
    selection.select_only(0);
    let output = draw_with(&ctx, input(Vec::new()), &[item("a.png")], &selection);
    let name = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            epaint::Shape::Text(text) if text.galley.job.text == "a.png" => {
                text.galley.job.sections.first().map(|s| s.format.clone())
            }
            _ => None,
        });
    output.drop_without_applying_deltas();
    let name = name.expect("the name is drawn");
    assert_eq!(name.color, tokens::CORAL_STRONG);
    assert_eq!(
        name.font_id.family,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into())
    );
    assert_eq!(name.font_id.size, tokens::SIZE_BODY);
}

/// Count the rect shapes in the output that stroke with the selection colour.
fn selection_strokes(output: &egui::FullOutput) -> usize {
    fn count(shape: &epaint::Shape) -> usize {
        match shape {
            epaint::Shape::Rect(rect)
                if rect.stroke.width > 0.0 && rect.stroke.color == tokens::CORAL_BRIGHT =>
            {
                1
            }
            epaint::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
            _ => 0,
        }
    }
    output
        .shapes
        .iter()
        .map(|clipped| count(&clipped.shape))
        .sum()
}

/// Draw one selected tile and count its selection strokes.
fn selected_strokes(ctx: &Context, item: GridItem) -> usize {
    let mut selection = Selection::default();
    selection.select_only(0);
    let output = draw_with(ctx, input(Vec::new()), &[item], &selection);
    let strokes = selection_strokes(&output);
    output.drop_without_applying_deltas();
    strokes
}

#[test]
fn a_selected_tile_with_a_texture_paints_one_selection_stroke() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let texture = ctx.load_texture(
        "selected",
        egui::ColorImage::filled([4, 4], Color32::RED),
        egui::TextureOptions::NEAREST,
    );
    let item = GridItem::new(
        Path::new("/images/shot.png").to_path_buf(),
        1,
        Some(texture),
    );
    assert_eq!(selected_strokes(&ctx, item), 1);
}

#[test]
fn a_selected_placeholder_tile_paints_one_selection_stroke() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    assert_eq!(selected_strokes(&ctx, item("sketch.psd")), 1);
}

#[test]
fn an_unselected_tile_paints_no_selection_stroke() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let output = draw_with(
        &ctx,
        input(Vec::new()),
        &[item("a.png")],
        &Selection::default(),
    );
    let strokes = selection_strokes(&output);
    output.drop_without_applying_deltas();
    assert_eq!(strokes, 0);
}
