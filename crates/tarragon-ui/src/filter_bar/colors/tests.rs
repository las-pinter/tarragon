//! Tests for the colour swatch row.

use super::show_colors;
use crate::swatch::tests::stroked_widths;
use crate::test_support::input;
use crate::tokens;
use egui::{Context, Event, Modifiers, PointerButton, Rect, vec2};
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_bar::{ColorChoice, FilterBar};

struct Frame {
    shapes: Vec<egui::epaint::ClippedShape>,
    clicked: Option<ColorChoice>,
    used: Rect,
}

fn frame(ctx: &Context, bar: &FilterBar, enabled: bool, max: f32, raw: egui::RawInput) -> Frame {
    let mut out = None;
    let output = ctx.run_ui(raw, |ui| {
        ui.scope(|ui| {
            ui.set_max_width(max);
            let clicked = show_colors(ui, bar, enabled);
            out = Some((clicked, ui.min_rect()));
        });
    });
    let shapes = output.shapes.clone();
    output.drop_without_applying_deltas();
    let (clicked, used) = out.expect("the closure ran");
    Frame {
        shapes,
        clicked,
        used,
    }
}

fn plain(bar: &FilterBar, enabled: bool, max: f32) -> Frame {
    frame(&Context::default(), bar, enabled, max, input(600.0, vec![]))
}

#[test]
fn twelve_swatches_draw_and_the_last_is_transparent() {
    let f = plain(&FilterBar::default(), true, 600.0);
    assert_eq!(stroked_widths(&f.shapes).len(), 12);
    let mut bar = FilterBar::default();
    bar.toggle_color(ColorChoice::Transparent);
    let f = plain(&bar, true, 600.0);
    let widths = stroked_widths(&f.shapes);
    assert_eq!(widths.len(), 12);
    assert_eq!(widths[11], (2.0, tokens::AMBER_ACCENT));
    let rest = (1.0, tokens::BORDER_INACTIVE);
    assert!(widths[..11].iter().all(|w| *w == rest));
}

#[test]
fn a_click_returns_the_matching_choice() {
    let bar = FilterBar::default();
    let want: [(u8, ColorChoice); 3] = [
        (0, ColorChoice::Bucket(ColorBucket::Red)),
        (6, ColorChoice::Bucket(ColorBucket::Blue)),
        (11, ColorChoice::Transparent),
    ];
    for (i, choice) in want {
        let ctx = Context::default();
        let first = frame(&ctx, &bar, true, 600.0, input(600.0, vec![]));
        let step = 36.0 + tokens::SPACING_XS;
        let pos = first.used.min + vec2(f32::from(i) * step + 18.0, 18.0);
        let click = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let events = vec![Event::PointerMoved(pos), click(true), click(false)];
        let second = frame(&ctx, &bar, true, 600.0, input(600.0, events));
        assert_eq!(second.clicked, Some(choice), "swatch {i}");
    }
}

#[test]
fn held_choices_get_the_amber_outline() {
    let mut bar = FilterBar::default();
    bar.toggle_color(ColorChoice::Bucket(ColorBucket::Red));
    bar.toggle_color(ColorChoice::Bucket(ColorBucket::Neutral));
    let widths = stroked_widths(&plain(&bar, true, 600.0).shapes);
    for (i, w) in widths.iter().enumerate() {
        let want = if i == 0 || i == 10 {
            (2.0, tokens::AMBER_ACCENT)
        } else {
            (1.0, tokens::BORDER_INACTIVE)
        };
        assert_eq!(*w, want, "swatch {i}");
    }
}

#[test]
fn disabled_draws_nothing_and_returns_none() {
    let f = plain(&FilterBar::default(), false, 600.0);
    assert_eq!(stroked_widths(&f.shapes).len(), 0);
    assert_eq!(f.clicked, None);
    assert_eq!(f.used.size(), vec2(0.0, 0.0));
}

#[test]
fn the_row_wraps_at_a_narrow_width_without_overflow() {
    let f = plain(&FilterBar::default(), true, 100.0);
    // Two swatches fit per row, so 12 swatches take 6 rows.
    assert!(f.used.width() <= 100.0, "{:?}", f.used);
    let rows = 6.0;
    let want = rows * 36.0 + (rows - 1.0) * tokens::SPACING_XS;
    assert_eq!(f.used.height(), want);
}

#[test]
fn the_row_leaves_the_callers_item_spacing_alone() {
    let ctx = Context::default();
    let mut seen = None;
    let output = ctx.run_ui(input(600.0, vec![]), |ui| {
        let before = ui.spacing().item_spacing;
        show_colors(ui, &FilterBar::default(), true);
        seen = Some((before, ui.spacing().item_spacing));
    });
    output.drop_without_applying_deltas();
    let (before, after) = seen.expect("the closure ran");
    assert_eq!(before, after);
}
