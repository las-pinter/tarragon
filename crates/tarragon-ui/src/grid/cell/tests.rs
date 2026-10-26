//! Tests for the tile painter and its hit test.

use super::*;
use crate::grid::{CellClick, GridResponse, VirtualGrid, metrics_for};
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput};

/// A headless screen with the given events.
fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
        events,
        ..Default::default()
    }
}

/// One item with no texture.
fn item(name: &str) -> GridItem {
    GridItem::new(Path::new("/images").join(name), 1, None)
}

/// Every text shape in the output, as `(text, font size)`.
fn painted_texts(output: &egui::FullOutput) -> Vec<(String, f32)> {
    fn collect(shape: &epaint::Shape, text: &mut Vec<(String, f32)>) {
        match shape {
            epaint::Shape::Text(shape) => {
                let size = shape
                    .galley
                    .job
                    .sections
                    .first()
                    .map_or(0.0, |section| section.format.font_id.size);
                text.push((shape.galley.job.text.clone(), size));
            }
            epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, text);
                }
            }
            _ => {}
        }
    }
    let mut text = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut text);
    }
    text
}

/// Draw one frame over `items` with `selection` and return the output.
fn draw_with(
    ctx: &Context,
    raw: RawInput,
    items: &[GridItem],
    selection: &Selection,
) -> egui::FullOutput {
    let grid = VirtualGrid {
        metrics: metrics_for(700.0),
        item_count: items.len(),
    };
    let order: Vec<usize> = (0..items.len()).collect();
    ctx.run_ui(raw, |ui| {
        grid.show_items_at(ui, 0.0, items, &order, selection);
    })
}

/// Draw one frame of a two-tile grid and return the outcome.
fn draw_frame(ctx: &Context, raw: RawInput, items: &[GridItem]) -> GridResponse {
    let grid = VirtualGrid {
        metrics: metrics_for(700.0),
        item_count: items.len(),
    };
    let order: Vec<usize> = (0..items.len()).collect();
    let selection = Selection::default();
    let mut outcome = GridResponse::default();
    ctx.run_ui(raw, |ui| {
        outcome = grid.show_items_at(ui, 0.0, items, &order, &selection);
    })
    .drop_without_applying_deltas();
    outcome
}

#[test]
fn a_tile_paints_a_placeholder_and_a_badge() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let items = [item("sketch.psd")];
    let selection = Selection::default();
    let output = draw_with(&ctx, input(Vec::new()), &items, &selection);
    let texts = painted_texts(&output);
    output.drop_without_applying_deltas();

    assert!(
        texts
            .iter()
            .any(|(text, size)| text == "PSD" && (size - BADGE_SIZE).abs() < f32::EPSILON),
        "no badge text at the badge size: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|(text, size)| text == "PSD"
                && (size - tokens::SIZE_CAPTION).abs() < f32::EPSILON),
        "no placeholder text at the caption size: {texts:?}"
    );
}

#[test]
fn the_thumbnail_is_one_token_tall() {
    let cell = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 208.0));
    let thumb = thumbnail_rect(cell);
    assert!(
        (thumb.height() - tokens::THUMBNAIL_SIZE).abs() < f32::EPSILON,
        "the thumbnail is {} points tall",
        thumb.height()
    );
    assert!((tokens::THUMBNAIL_SIZE - 160.0).abs() < f32::EPSILON);
}

#[test]
fn a_texture_paints_as_a_rounded_rect() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let texture = ctx.load_texture(
        "thumb",
        egui::ColorImage::filled([4, 4], Color32::RED),
        egui::TextureOptions::NEAREST,
    );
    let items = [GridItem::new(
        Path::new("/images/shot.png").to_path_buf(),
        1,
        Some(texture),
    )];
    let grid = VirtualGrid {
        metrics: metrics_for(700.0),
        item_count: 1,
    };
    let order = [0];
    let selection = Selection::default();
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        grid.show_items_at(ui, 0.0, &items, &order, &selection);
    });
    let textured = output
        .shapes
        .iter()
        .any(|clipped| matches!(&clipped.shape, epaint::Shape::Rect(rect) if rect.brush.is_some()));
    output.drop_without_applying_deltas();
    assert!(textured, "the tile painted no texture");
}

#[test]
fn fit_rect_keeps_the_aspect_ratio_and_centers() {
    let bounds = Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 160.0));
    let wide = fit_rect(bounds, vec2(256.0, 128.0));
    assert!((wide.width() / wide.height() - 2.0).abs() < 1e-4);
    assert!(bounds.contains_rect(wide));
    assert_eq!(wide.center(), bounds.center());
    assert!((wide.height() - 150.0).abs() < 1e-3, "the width limits it");

    let tall = fit_rect(bounds, vec2(100.0, 200.0));
    assert!((tall.width() / tall.height() - 0.5).abs() < 1e-4);
    assert!((tall.height() - 160.0).abs() < 1e-3, "the height limits it");
    assert_eq!(tall.center(), bounds.center());
}

#[test]
fn a_non_square_texture_paints_with_its_own_aspect_ratio() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let texture = ctx.load_texture(
        "wide",
        egui::ColorImage::filled([8, 2], Color32::RED),
        egui::TextureOptions::NEAREST,
    );
    let items = [GridItem::new(
        Path::new("/images/wide.png").to_path_buf(),
        1,
        Some(texture),
    )];
    let grid = VirtualGrid {
        metrics: metrics_for(700.0),
        item_count: 1,
    };
    let order = [0];
    let selection = Selection::default();
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        grid.show_items_at(ui, 0.0, &items, &order, &selection);
    });
    let painted = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            epaint::Shape::Rect(rect) if rect.brush.is_some() => Some(rect.rect),
            _ => None,
        });
    output.drop_without_applying_deltas();
    let painted = painted.expect("the tile painted a texture");
    assert!(
        (painted.width() / painted.height() - 4.0).abs() < 1e-3,
        "the picture is {} by {}",
        painted.width(),
        painted.height()
    );
}

#[test]
fn a_click_reports_the_index_and_the_modifiers() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let items = [item("a.png"), item("b.png")];
    draw_frame(&ctx, input(Vec::new()), &items);

    let id = Id::new(("tarragon-grid-tile", 1));
    let pos = ctx
        .read_response(id)
        .expect("the second tile registered")
        .rect
        .center();
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };

    draw_frame(&ctx, input(vec![Event::PointerMoved(pos)]), &items);
    draw_frame(&ctx, input(vec![press(true)]), &items);
    let outcome = draw_frame(&ctx, input(vec![press(false)]), &items);
    assert_eq!(
        outcome.click,
        Some(CellClick {
            index: 1,
            position: 1,
            modifiers: Modifiers::NONE,
        })
    );
}

#[test]
fn the_badge_colours_come_from_the_extension() {
    // The pairs follow `design/mockups/busy-states.html` lines 55-62.
    let pairs = [
        ("a.jpg", tokens::BADGE_BG_SAGE, tokens::BADGE_FG_SAGE),
        ("a.JPEG", tokens::BADGE_BG_SAGE, tokens::BADGE_FG_SAGE),
        ("a.png", tokens::BADGE_BG_NAVY, tokens::BADGE_FG_NAVY),
        ("a.psd", tokens::BADGE_BG_VINOUS, tokens::BADGE_FG_VINOUS),
        ("a.psb", tokens::BADGE_BG_VINOUS, tokens::BADGE_FG_VINOUS),
        ("a.webp", tokens::BADGE_BG_TEAL, tokens::BADGE_FG_TEAL),
        ("a.gif", tokens::BADGE_BG_PLUM, tokens::BADGE_FG_PLUM),
        ("a.tif", tokens::BADGE_BG_UMBER, tokens::BADGE_FG_UMBER),
        ("a.tiff", tokens::BADGE_BG_UMBER, tokens::BADGE_FG_UMBER),
        ("a.bmp", tokens::BADGE_BG_OLIVE, tokens::BADGE_FG_OLIVE),
        ("a.kra", tokens::BADGE_BG_PLUM, tokens::BADGE_FG_PLUM),
        ("a.clip", tokens::BADGE_BG_AZURE, tokens::BADGE_FG_AZURE),
        ("noext", tokens::BADGE_BG_NEUTRAL, tokens::BADGE_FG_NEUTRAL),
    ];
    for (name, fill, text) in pairs {
        assert_eq!(
            badge_colors(Path::new(name)),
            (fill, text),
            "the badge pair for {name}"
        );
    }
}

#[test]
fn the_badge_text_comes_from_the_path() {
    assert_eq!(extension_label(Path::new("/images/scan_07.tiff")), "TIFF");
    assert_eq!(extension_label(Path::new("/images/matte.BMP")), "BMP");
    assert_eq!(extension_label(Path::new("/images/concept.kra")), "KRA");
    assert_eq!(extension_label(Path::new("/images/noext")), "FILE");
}

#[test]
fn a_selected_tile_outlines_the_thumbnail() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let items = [item("sketch.psd")];
    let grid = VirtualGrid {
        metrics: metrics_for(700.0),
        item_count: 1,
    };
    let mut selection = Selection::default();
    selection.select_only(0);
    let order = [0];
    let output = ctx.run_ui(input(Vec::new()), |ui| {
        grid.show_items_at(ui, 0.0, &items, &order, &selection);
    });
    let outlined = output.shapes.iter().any(|clipped| match &clipped.shape {
        epaint::Shape::Rect(rect) => rect.stroke.color == tokens::CORAL_MUTED,
        _ => false,
    });
    output.drop_without_applying_deltas();
    assert!(outlined, "the selected tile painted no coral outline");
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
                circle.fill == tokens::CORAL_MUTED && (circle.radius - CHECK_SIZE / 2.0).abs() < 0.5
            }
            _ => false,
        })
        .count();
    output.drop_without_applying_deltas();
    assert_eq!(checks, 2, "each selected tile paints one check badge");
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
            matches!(&clipped.shape, epaint::Shape::Circle(circle) if circle.fill == tokens::CORAL_MUTED)
        })
        .count();
    output.drop_without_applying_deltas();
    assert_eq!(checks, 0, "one selected tile needs no check badge");
}

#[test]
fn a_hovered_tile_paints_the_hover_border() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let items = [item("a.png")];
    let selection = Selection::default();
    draw_with(&ctx, input(Vec::new()), &items, &selection).drop_without_applying_deltas();
    let pos = ctx
        .read_response(Id::new(("tarragon-grid-tile", 0)))
        .expect("the tile registered")
        .rect
        .center();

    // The hover fade runs 150 ms, so it needs more than nine 60 Hz frames.
    let mut output = draw_with(
        &ctx,
        input(vec![Event::PointerMoved(pos)]),
        &items,
        &selection,
    );
    for _ in 0..12 {
        output.drop_without_applying_deltas();
        output = draw_with(
            &ctx,
            input(vec![Event::PointerMoved(pos)]),
            &items,
            &selection,
        );
    }

    let hovered = output.shapes.iter().any(|clipped| match &clipped.shape {
        epaint::Shape::Rect(rect) => rect.stroke.color == tokens::BORDER_CARD,
        _ => false,
    });
    output.drop_without_applying_deltas();
    assert!(hovered, "the hovered tile painted no card border");
}

/// Draw one tile of `item` and return the shapes.
fn tile_output(item: GridItem) -> egui::FullOutput {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    draw_with(&ctx, input(Vec::new()), &[item], &Selection::default())
}

/// Whether the output holds a circle stroked in the error colour.
fn has_error_ring(output: &egui::FullOutput) -> bool {
    output.shapes.iter().any(|clipped| {
        matches!(&clipped.shape, epaint::Shape::Circle(circle)
            if circle.stroke.color == tokens::CORAL_STRONG)
    })
}

#[test]
fn a_failed_tile_paints_the_error_mark() {
    let mut failed = item("broken.psd");
    failed.error = Some(crate::grid::TileError::Decode);
    let output = tile_output(failed);
    let texts = painted_texts(&output);
    let ring = has_error_ring(&output);
    output.drop_without_applying_deltas();
    assert!(ring, "no error ring");
    assert!(
        texts
            .iter()
            .any(|(text, size)| text == "!" && (*size - ERROR_MARK_FONT).abs() < 0.01),
        "no bang in {texts:?}"
    );
    assert!(
        !texts
            .iter()
            .any(|(_, size)| (*size - tokens::SIZE_CAPTION).abs() < 0.01),
        "the extension placeholder text still shows"
    );
}

#[test]
fn a_cleared_texture_paints_the_placeholder() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let texture = ctx.load_texture(
        "thumb",
        egui::ColorImage::filled([4, 4], Color32::RED),
        egui::TextureOptions::NEAREST,
    );
    let mut cleared = item("shot.png");
    cleared.texture = Some(texture);
    cleared.texture = None;
    let output = draw_with(&ctx, input(Vec::new()), &[cleared], &Selection::default());
    let texts = painted_texts(&output);
    let ring = has_error_ring(&output);
    output.drop_without_applying_deltas();
    assert!(!ring, "a cleared tile shows an error ring");
    assert!(
        texts
            .iter()
            .any(|(text, size)| text == "PNG" && (*size - tokens::SIZE_CAPTION).abs() < 0.01),
        "no extension placeholder in {texts:?}"
    );
}

#[test]
fn a_texture_wins_over_an_error() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let texture = ctx.load_texture(
        "thumb",
        egui::ColorImage::filled([4, 4], Color32::RED),
        egui::TextureOptions::NEAREST,
    );
    let mut both = item("both.png");
    both.texture = Some(texture);
    both.error = Some(crate::grid::TileError::Read);
    let output = draw_with(&ctx, input(Vec::new()), &[both], &Selection::default());
    let textured = output
        .shapes
        .iter()
        .any(|clipped| matches!(&clipped.shape, epaint::Shape::Rect(rect) if rect.brush.is_some()));
    let ring = has_error_ring(&output);
    let coral_name = output.shapes.iter().any(|clipped| {
        matches!(&clipped.shape, epaint::Shape::Text(text)
            if text.galley.job.text == "both.png"
                && text.galley.job.sections.iter().any(|s| s.format.color == tokens::CORAL_STRONG))
    });
    output.drop_without_applying_deltas();
    assert!(textured, "the texture did not paint");
    assert!(!ring, "an error ring shows over a texture");
    assert!(!coral_name, "the name is coral over a texture");
}
