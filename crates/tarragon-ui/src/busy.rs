//! The busy strip: a label and a track with a block sliding across it.

use egui::{Rect, RichText, Sense, Ui, pos2, vec2};

use crate::tokens;

/// Height of the busy bar, in points.
const BUSY_BAR_HEIGHT: f32 = 6.0;
/// The block of the busy bar is this share of the track wide.
const BUSY_BLOCK_SHARE: f32 = 0.45;
/// The block runs this many track widths, so it starts and ends off the track.
const BUSY_RUN_SHARE: f32 = 1.5;
/// Seconds the block takes to cross the track.
const BUSY_PERIOD: f64 = 1.2;

/// How far the busy block has run, from 0 to 1, after `time` seconds.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the fraction is in 0 to 1, so f32 holds it"
)]
fn busy_phase(time: f64) -> f32 {
    (time / BUSY_PERIOD).fract() as f32
}

/// The `label` and a track with a block sliding across it. The strip asks for
/// a repaint, so the block keeps moving while it shows.
pub(crate) fn strip(ui: &mut Ui, label: &str) {
    ui.horizontal(|ui| {
        let label = RichText::new(label).size(tokens::SIZE_CAPTION);
        ui.label(label.color(tokens::TEXT_MUTED));
        let size = vec2(ui.available_width(), BUSY_BAR_HEIGHT);
        let (track, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter().with_clip_rect(track);
        painter.rect_filled(track, tokens::RADIUS_XS, tokens::BG_TERTIARY);
        // The block runs from off the left edge to off the right edge.
        let phase = busy_phase(ui.input(|i| i.time));
        let width = track.width() * BUSY_BLOCK_SHARE;
        let left = track.left() + track.width() * BUSY_RUN_SHARE * phase - width;
        let block = Rect::from_min_size(pos2(left, track.top()), vec2(width, track.height()));
        painter.rect_filled(block, tokens::RADIUS_XS, tokens::AMBER_ACCENT);
        ui.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::{busy_phase, strip};
    use crate::{test_support, tokens};
    use egui::epaint::Shape;
    use egui::{Context, Rect};

    /// The text and the amber block rects of one frame at `time` seconds.
    fn frame(ctx: &Context, time: f64) -> (Vec<String>, Vec<Rect>) {
        let mut input = test_support::input(400.0, vec![]);
        input.time = Some(time);
        let output = ctx.run_ui(input, |ui| strip(ui, "Working..."));
        let mut texts = Vec::new();
        let mut blocks = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut texts, &mut blocks);
        }
        output.drop_without_applying_deltas();
        (texts, blocks)
    }

    fn collect(shape: &Shape, texts: &mut Vec<String>, blocks: &mut Vec<Rect>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, texts, blocks)),
            Shape::Text(t) => texts.push(t.galley.job.text.clone()),
            Shape::Rect(r) if r.fill == tokens::AMBER_ACCENT => blocks.push(r.rect),
            _ => {}
        }
    }

    #[test]
    fn phase_wraps_every_period() {
        assert!(busy_phase(0.0).abs() < 1e-6);
        assert!((busy_phase(0.6) - 0.5).abs() < 1e-6);
        assert!(busy_phase(1.2).abs() < 1e-3);
    }

    #[test]
    fn strip_shows_label_and_moves_block() {
        let ctx = test_support::context();
        let (texts, first) = frame(&ctx, 10.0);
        assert!(texts.iter().any(|t| t == "Working..."), "{texts:?}");
        assert_eq!(first.len(), 1, "{first:?}");
        let (_, second) = frame(&ctx, 10.3);
        assert_eq!(second.len(), 1, "{second:?}");
        assert_ne!(first[0].min.x, second[0].min.x);
    }
}
