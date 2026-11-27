//! Tests for the tile painter and its hit test.

use super::*;
use crate::grid::{CellClick, GridResponse, VirtualGrid, metrics_for};
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput};

/// A headless screen with the given events.
pub(super) fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
        events,
        ..Default::default()
    }
}

/// One item with no texture.
pub(super) fn item(name: &str) -> GridItem {
    GridItem::new(Path::new("/images").join(name), 1, None)
}

/// Every text shape in the output, as `(text, font size)`.
pub(super) fn painted_texts(output: &egui::FullOutput) -> Vec<(String, f32)> {
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
pub(super) fn draw_with(
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
fn the_thumbnail_is_the_square_at_the_top_of_the_cell() {
    let cell = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 200.0 + LABEL_BLOCK));
    let thumb = thumbnail_rect(cell);
    assert_eq!(
        thumb,
        Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 200.0))
    );
}

#[test]
fn a_letterboxed_picture_rounds_only_the_corners_it_shares() {
    let square = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
    let radius = CornerRadius::same(8);
    let wide = fit_rect(square, vec2(200.0, 100.0));
    assert_eq!(touching_corners(wide, square, radius), CornerRadius::ZERO);
    assert_eq!(touching_corners(square, square, radius), radius);
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
    // The page gives PNG, JPG, PSD, and WEBP; the rest keep the mockup text
    // colours of `design/mockups/busy-states.html`.
    let colours = [
        ("a.jpg", tokens::BADGE_FG_SAGE),
        ("a.JPEG", tokens::BADGE_FG_SAGE),
        ("a.png", tokens::BADGE_FG_NAVY),
        ("a.psd", tokens::BADGE_FG_VINOUS),
        ("a.psb", tokens::BADGE_FG_VINOUS),
        ("a.webp", tokens::BADGE_FG_TEAL),
        ("a.gif", tokens::BADGE_FG_PLUM),
        ("a.tif", tokens::BADGE_FG_UMBER),
        ("a.tiff", tokens::BADGE_FG_UMBER),
        ("a.bmp", tokens::BADGE_FG_OLIVE),
        ("a.kra", tokens::BADGE_FG_PLUM),
        ("a.clip", tokens::BADGE_FG_AZURE),
        ("noext", tokens::BADGE_FG_NEUTRAL),
    ];
    for (name, colour) in colours {
        assert_eq!(badge_color(Path::new(name)), colour, "the badge of {name}");
    }
    assert_eq!(tokens::BADGE_FG_NAVY, Color32::from_rgb(0x7b, 0xa8, 0xc8));
    assert_eq!(tokens::BADGE_FG_VINOUS, Color32::from_rgb(0xf0, 0x99, 0x7b));
}

#[test]
fn the_badge_sits_on_the_base_fill_eight_points_in() {
    let ctx = Context::default();
    crate::apply_theme(&ctx);
    let output = draw_with(
        &ctx,
        input(Vec::new()),
        &[item("a.png")],
        &Selection::default(),
    );
    let thumb = ctx
        .read_response(Id::new(("tarragon-grid-tile", 0)))
        .map(|r| thumbnail_rect(r.rect))
        .expect("the tile registered");
    let badge = rects(&output)
        .into_iter()
        .find(|r| r.fill == tokens::BG_PRIMARY && r.rect.min == thumb.min + Vec2::splat(8.0));
    output.drop_without_applying_deltas();
    assert!(badge.is_some(), "no base-fill badge at the corner inset");
}

#[test]
fn the_badge_text_comes_from_the_path() {
    assert_eq!(extension_label(Path::new("/images/scan_07.tiff")), "TIFF");
    assert_eq!(extension_label(Path::new("/images/matte.BMP")), "BMP");
    assert_eq!(extension_label(Path::new("/images/concept.kra")), "KRA");
    assert_eq!(extension_label(Path::new("/images/noext")), "FILE");
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

/// Every rect shape in the output.
pub(super) fn rects(output: &egui::FullOutput) -> Vec<epaint::RectShape> {
    fn collect(shape: &epaint::Shape, found: &mut Vec<epaint::RectShape>) {
        match shape {
            epaint::Shape::Rect(rect) => found.push(rect.clone()),
            epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut found);
    }
    found
}
