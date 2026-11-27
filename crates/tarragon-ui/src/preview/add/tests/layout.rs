//! Exact layout of the "+ Tag" button and its menu.

use super::{CREATE_NEW, find, frame, open_menu, preview_with, setup};
use crate::preview::add::{SEPARATOR, menu_height, menu_placement};
use crate::preview::test_support::pill;
use crate::preview::{ColorState, ColorStates, Preview};
use crate::tokens;
use egui::epaint::Shape;
use egui::{Align2, Color32, Context, Pos2, Rect, Vec2, vec2};

/// Every painted rect with its fill and stroke colour, and every text size.
#[derive(Default)]
struct Shapes {
    rects: Vec<(Rect, Color32, Color32)>,
    text_sizes: Vec<(String, Vec2)>,
}

fn walk(shape: &Shape, found: &mut Shapes) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
        Shape::Rect(r) => found.rects.push((r.rect, r.fill, r.stroke.color)),
        Shape::Text(t) => found
            .text_sizes
            .push((t.galley.job.text.clone(), t.galley.size())),
        _ => {}
    }
}

/// The room above the panel in the flip test, in points: more than a menu
/// of a few rows.
const TOP: f32 = 200.0;

/// Run one frame and give what it painted.
fn paint(ctx: &Context, preview: &mut Preview) -> Shapes {
    paint_at(ctx, preview, 600.0)
}

/// Run one frame in a window `height` tall and give what it painted.
fn paint_at(ctx: &Context, preview: &mut Preview, height: f32) -> Shapes {
    paint_below(ctx, preview, height, 0.0)
}

/// [`paint_at`] with the panel `top` points down the window.
fn paint_below(ctx: &Context, preview: &mut Preview, height: f32, top: f32) -> Shapes {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, height))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| {
        ui.add_space(top);
        preview.show(ui);
    });
    let mut found = Shapes::default();
    output
        .shapes
        .iter()
        .for_each(|c| walk(&c.shape, &mut found));
    output.drop_without_applying_deltas();
    found
}

/// A Preview with the red square lit, one full tag pill and two known tags.
fn lit() -> Preview {
    let mut preview = preview_with(vec![pill("hands", ColorState::All)], &["a", "b"]);
    preview.set_colors_enabled(true);
    let mut colors = ColorStates::default();
    colors[0] = ColorState::All;
    preview.set_colors(colors);
    preview
}

fn rect_with(found: &Shapes, pick: impl Fn(Color32, Color32) -> bool) -> Rect {
    found
        .rects
        .iter()
        .find(|(_, fill, stroke)| pick(*fill, *stroke))
        .map(|(r, ..)| *r)
        .expect("the rect is painted")
}

/// The "+ Tag" button: the hover fill, as tall as its text plus the padding.
fn button_rect(found: &Shapes) -> Rect {
    let text = found
        .text_sizes
        .iter()
        .find(|(t, _)| t == "+ Tag")
        .expect("the button text is painted")
        .1;
    found
        .rects
        .iter()
        .find(|(r, fill, _)| {
            // Hovered or focused, the button lifts one step.
            [tokens::SURFACE_HOVER, tokens::SURFACE_HIGHLIGHT].contains(fill)
                && r.size() == text + vec2(18.0, 6.0)
        })
        .map(|(r, ..)| *r)
        .expect("the button is painted")
}

/// The fill of the "hands" pill.
fn hands_fill() -> Color32 {
    crate::preview::tags::tag_colors("hands").0
}

#[test]
fn the_bars_the_pills_and_the_button_share_one_row() {
    let mut preview = lit();
    let ctx = setup(&mut preview);
    let found = paint(&ctx, &mut preview);
    let red = rect_with(&found, |fill, _| fill == tokens::BUCKET_RED);
    let pill = rect_with(&found, |fill, _| fill == hands_fill());
    let button = button_rect(&found);
    // The bars take six points more than the chip gap of six.
    assert_eq!(pill.left() - red.right(), 12.0);
    assert_eq!(button.left() - pill.right(), 6.0);
    let centre = |r: Rect| r.center().y;
    assert!((centre(red) - centre(pill)).abs() < 0.01);
    assert!((centre(button) - centre(pill)).abs() < 0.01);
}

#[test]
fn the_button_is_a_pill_on_the_hover_fill() {
    let mut preview = lit();
    let ctx = setup(&mut preview);
    let found = paint(&ctx, &mut preview);
    let button = button_rect(&found);
    let pill = rect_with(&found, |fill, _| fill == hands_fill());
    assert_eq!(button.height(), pill.height());
}

#[test]
fn menu_rows_sit_one_row_height_apart() {
    let mut preview = lit();
    let ctx = setup(&mut preview);
    open_menu(&ctx, &mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    let (a, b) = (find(&texts, "a").unwrap(), find(&texts, "b").unwrap());
    assert_eq!(b.y - a.y, tokens::MENU_ROW_HEIGHT);
    let create = find(&texts, CREATE_NEW).unwrap();
    assert_eq!(create.y - b.y, tokens::MENU_ROW_HEIGHT + SEPARATOR);
}

#[test]
fn the_menu_opens_below_the_button() {
    let mut preview = lit();
    let ctx = setup(&mut preview);
    let button = button_rect(&paint(&ctx, &mut preview));
    open_menu(&ctx, &mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    let first = find(&texts, "a").unwrap();
    assert!(first.y > button.bottom(), "{first:?} {button:?}");
}

#[test]
fn the_menu_height_counts_rows_rule_and_frame() {
    // The padding sits inside the frame, six points a side.
    let pad = 2.0 * tokens::MENU_PADDING;
    assert_eq!(menu_height(0), tokens::MENU_ROW_HEIGHT + pad);
    assert_eq!(
        menu_height(2),
        tokens::MENU_ROW_HEIGHT * 3.0 + SEPARATOR + pad
    );
    // The tag rows stop growing at eight and scroll.
    assert_eq!(
        menu_height(80),
        tokens::MENU_ROW_HEIGHT * 9.0 + SEPARATOR + pad
    );
}

#[test]
fn the_menu_flips_above_the_button_only_when_it_does_not_fit_below() {
    let bounds = Rect::from_min_max(Pos2::new(8.0, 8.0), Pos2::new(392.0, 592.0));
    let at = |top: f32| Rect::from_min_size(Pos2::new(20.0, top), vec2(40.0, 20.0));
    let (pos, pivot) = menu_placement(at(100.0), 100.0, bounds);
    assert_eq!((pos, pivot), (Pos2::new(20.0, 124.0), Align2::LEFT_TOP));
    // 500 + 20 + 4 + 100 = 624 passes the 592 limit; above it fits.
    let (pos, pivot) = menu_placement(at(500.0), 100.0, bounds);
    assert_eq!((pos, pivot), (Pos2::new(20.0, 496.0), Align2::LEFT_BOTTOM));
    // No room either way keeps it below, where the window clamp acts.
    let (pos, pivot) = menu_placement(at(100.0), 600.0, bounds);
    assert_eq!((pos, pivot), (Pos2::new(20.0, 124.0), Align2::LEFT_TOP));
}

/// The menu frame in a painted frame: the filled rect taller than a row that
/// is not the button's. The area fades in, so the colour is not matched.
fn menu_frame(found: &Shapes) -> Rect {
    let button = button_rect(found);
    let frames: Vec<Rect> = found
        .rects
        .iter()
        .filter(|(r, fill, _)| {
            *r != button
                && fill.a() > 0
                && r.height() > tokens::MENU_ROW_HEIGHT
                && r.width() > tokens::MENU_ROW_HEIGHT
        })
        .map(|(r, ..)| *r)
        .collect();
    assert_eq!(frames.len(), 1, "{frames:?}");
    frames[0]
}

#[test]
fn the_drawn_menu_frame_is_as_tall_as_menu_height() {
    for count in [0, 2, 9, 80] {
        let names: Vec<String> = (0..count).map(|i| format!("tag-{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut preview = preview_with(vec![], &refs);
        let ctx = setup(&mut preview);
        open_menu(&ctx, &mut preview);
        // The menu area fades in, so the first frame paints it too faintly.
        paint(&ctx, &mut preview);
        let frame = menu_frame(&paint(&ctx, &mut preview));
        assert_eq!(frame.height(), menu_height(count), "{count} tags");
    }
}

#[test]
fn a_menu_near_the_bottom_opens_above_the_button_inside_the_edge_margin() {
    let mut preview = lit();
    let ctx = setup(&mut preview);
    let button = button_rect(&paint_below(&ctx, &mut preview, 600.0, TOP));
    open_menu(&ctx, &mut preview);
    // The window ends just under the button, so the menu cannot fit below.
    let height = button.bottom() + tokens::SPACING_S;
    for _ in 0..3 {
        paint_below(&ctx, &mut preview, height, TOP);
    }
    let frame = menu_frame(&paint_below(&ctx, &mut preview, height, TOP));
    // The chip row height comes from the font, so it is a fraction off a point.
    assert!((frame.bottom() - (button.top() - tokens::SPACING_XS)).abs() < 0.1);
    assert!(frame.top() >= tokens::MENU_EDGE_MARGIN, "{frame:?}");
    assert!(frame.bottom() <= height - tokens::MENU_EDGE_MARGIN);
}
