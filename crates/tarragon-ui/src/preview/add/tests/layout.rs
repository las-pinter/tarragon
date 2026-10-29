//! Exact layout of the "+ add" button and its menu.

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

/// Run one frame and give what it painted.
fn paint(ctx: &Context, preview: &mut Preview) -> Shapes {
    paint_at(ctx, preview, 600.0)
}

/// Run one frame in a window `height` tall and give what it painted.
fn paint_at(ctx: &Context, preview: &mut Preview, height: f32) -> Shapes {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, height))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| preview.show(ui));
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

/// The room above a pill that the row gives it: the pill is centred in a row
/// at least as tall as the theme's interactive height.
fn centring(ctx: &Context, pill: Rect) -> f32 {
    (ctx.global_style().spacing.interact_size.y - pill.height()).max(0.0) / 2.0
}

#[test]
fn the_pills_start_one_gap_below_the_colour_row() {
    let mut preview = lit();
    let ctx = setup(&mut preview);
    let found = paint(&ctx, &mut preview);
    let red = rect_with(&found, |fill, _| fill == tokens::BUCKET_RED);
    let pill = rect_with(&found, |fill, _| fill == tokens::CORAL_DARK);
    assert_eq!(
        pill.top() - red.bottom(),
        tokens::SPACING_S + centring(&ctx, pill)
    );
}

#[test]
fn the_button_is_an_outlined_row_of_its_own_below_the_pill_row() {
    let mut preview = lit();
    let ctx = setup(&mut preview);
    let found = paint(&ctx, &mut preview);
    let red = rect_with(&found, |fill, _| fill == tokens::BUCKET_RED);
    let pill = rect_with(&found, |fill, _| fill == tokens::CORAL_DARK);
    let button = rect_with(&found, |_, stroke| stroke == tokens::BORDER_INTERACTIVE);
    let text = found
        .text_sizes
        .iter()
        .find(|(t, _)| t == "+ add")
        .unwrap()
        .1;
    // 2 and 8 points of padding inside a 1 point border.
    assert_eq!(button.size(), text + vec2(18.0, 6.0));
    assert_eq!(button.left(), red.left());
    let row_top = pill.top() - centring(&ctx, pill);
    assert_eq!(
        button.top() - row_top,
        tokens::PREVIEW_TAG_ROW_HEIGHT + tokens::SPACING_S
    );
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
    let button = rect_with(&paint(&ctx, &mut preview), |_, s| {
        s == tokens::BORDER_INTERACTIVE
    });
    open_menu(&ctx, &mut preview);
    let texts = frame(&ctx, &mut preview, vec![]);
    let first = find(&texts, "a").unwrap();
    assert!(first.y > button.bottom(), "{first:?} {button:?}");
}

#[test]
fn the_menu_height_counts_rows_rule_and_frame() {
    // One row, and 5 points of padding and border either side.
    assert_eq!(menu_height(0), 27.0 + 10.0);
    assert_eq!(menu_height(2), 27.0 * 3.0 + 9.0 + 10.0);
    // The tag rows stop growing at eight and scroll.
    assert_eq!(menu_height(80), 27.0 * 9.0 + 9.0 + 10.0);
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

/// The menu frame in a painted frame: the one rect with both a fill and a
/// border that is not the button's. The area fades in, so the colours are not
/// matched exactly.
fn menu_frame(found: &Shapes) -> Rect {
    let button = rect_with(found, |_, stroke| stroke == tokens::BORDER_INTERACTIVE);
    let frames: Vec<Rect> = found
        .rects
        .iter()
        .filter(|(r, fill, stroke)| *r != button && fill.a() > 0 && stroke.a() > 0)
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
    let button = rect_with(&paint(&ctx, &mut preview), |_, s| {
        s == tokens::BORDER_INTERACTIVE
    });
    open_menu(&ctx, &mut preview);
    // The window ends just under the button, so the menu cannot fit below.
    let height = button.bottom() + tokens::SPACING_S;
    for _ in 0..3 {
        paint_at(&ctx, &mut preview, height);
    }
    let frame = menu_frame(&paint_at(&ctx, &mut preview, height));
    assert_eq!(frame.bottom(), button.top() - tokens::SPACING_XS);
    assert!(frame.top() >= tokens::MENU_EDGE_MARGIN, "{frame:?}");
    assert!(frame.bottom() <= height - tokens::MENU_EDGE_MARGIN);
}
