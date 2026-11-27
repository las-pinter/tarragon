//! Tests for the colour swatches.

use super::{SWATCH_GAP, held_swatches, show_colors};
use crate::swatch::tests::circles;
use crate::swatch::{FILTER_SIZE, Swatch};
use crate::test_support::input;
use crate::tokens;
use egui::{Context, Event, Modifiers, PointerButton, Rect, vec2};
use tarragon_core::color::ColorBucket;
use tarragon_core::filter_bar::{ColorChoice, FilterBar};

/// The gap a test asks for after the last swatch.
const AFTER: f32 = 16.0;

struct Frame {
    shapes: Vec<egui::epaint::ClippedShape>,
    clicked: Option<ColorChoice>,
    used: Rect,
    /// Where the next item starts after the swatches.
    next: f32,
}

fn frame(ctx: &Context, bar: &FilterBar, raw: egui::RawInput) -> Frame {
    let mut out = None;
    let output = ctx.run_ui(raw, |ui| {
        ui.horizontal(|ui| {
            let clicked = show_colors(ui, bar, AFTER);
            let used = ui.min_rect();
            let next = ui
                .allocate_exact_size(vec2(1.0, 1.0), egui::Sense::hover())
                .0
                .min
                .x;
            out = Some((clicked, used, next));
        });
    });
    let shapes = output.shapes.clone();
    output.drop_without_applying_deltas();
    let (clicked, used, next) = out.expect("the closure ran");
    Frame {
        shapes,
        clicked,
        used,
        next,
    }
}

fn plain(bar: &FilterBar) -> Frame {
    frame(&Context::default(), bar, input(600.0, vec![]))
}

/// The ring colours of the drawn swatches, keyed by swatch index.
fn rings(f: &Frame) -> Vec<(usize, egui::Color32)> {
    let step = FILTER_SIZE + SWATCH_GAP;
    circles(&f.shapes)
        .into_iter()
        .filter(|c| c.3.width > 0.0)
        .map(|(center, _, _, stroke)| {
            let offset = center.x - f.used.min.x - FILTER_SIZE / 2.0;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a swatch index is below twelve"
            )]
            let index = (offset / step).round() as usize;
            (index, stroke.color)
        })
        .collect()
}

#[test]
fn twelve_round_swatches_draw_with_the_page_gap() {
    let f = plain(&FilterBar::default());
    let fills: Vec<_> = circles(&f.shapes)
        .into_iter()
        .filter(|c| c.3.width == 0.0)
        .collect();
    assert_eq!(fills.len(), 12, "eleven buckets and the checker base");
    assert_eq!(fills[0].2, tokens::BUCKET_RED);
    assert_eq!(fills[11].2, tokens::CHECKER_LIGHT, "the checker is last");
    let step = fills[1].0.x - fills[0].0.x;
    assert_eq!(step, FILTER_SIZE + SWATCH_GAP);
    assert_eq!(SWATCH_GAP, 8.0);
    assert!(rings(&f).is_empty(), "no swatch is held");
}

#[test]
fn the_gap_after_the_last_swatch_is_the_callers() {
    let f = plain(&FilterBar::default());
    assert_eq!(f.next - f.used.max.x, AFTER);
}

#[test]
fn held_choices_get_the_light_ring() {
    let mut bar = FilterBar::default();
    bar.toggle_color(ColorChoice::Bucket(ColorBucket::Red));
    bar.toggle_color(ColorChoice::Bucket(ColorBucket::Neutral));
    bar.toggle_color(ColorChoice::Transparent);
    let found = rings(&plain(&bar));
    let want: Vec<_> = [0, 10, 11]
        .into_iter()
        .map(|i| (i, tokens::TEXT_PRIMARY))
        .collect();
    assert_eq!(found, want);
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
        let first = frame(&ctx, &bar, input(600.0, vec![]));
        let step = FILTER_SIZE + SWATCH_GAP;
        let half = FILTER_SIZE / 2.0;
        let pos = first.used.min + vec2(f32::from(i) * step + half, half);
        let click = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let events = vec![Event::PointerMoved(pos), click(true), click(false)];
        let second = frame(&ctx, &bar, input(600.0, events));
        assert_eq!(second.clicked, Some(choice), "swatch {i}");
    }
}

#[test]
fn held_swatches_follow_the_swatch_order() {
    let mut bar = FilterBar::default();
    bar.toggle_color(ColorChoice::Transparent);
    bar.toggle_color(ColorChoice::Bucket(ColorBucket::Blue));
    bar.toggle_color(ColorChoice::Bucket(ColorBucket::Red));
    assert_eq!(
        held_swatches(&bar),
        [
            Swatch::Bucket(ColorBucket::Red),
            Swatch::Bucket(ColorBucket::Blue),
            Swatch::Transparent
        ]
    );
}
