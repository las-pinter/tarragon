//! The gallery top strip: header, info row, filter pill, and sort pills.

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, Response, RichText,
    Sense, Stroke, StrokeKind, Ui, Vec2, vec2,
};
use tarragon_core::sorting::SortMode;

use super::Gallery;
use crate::filter_bar;
use crate::theme::points;
use crate::tokens;

/// Gap between the sort label and a pill, in points.
const SORT_GAP: f32 = 6.0;
/// Horizontal pill padding, in points.
const PILL_PAD_X: f32 = 10.0;
/// Vertical pill padding, in points.
const PILL_PAD_Y: f32 = 3.0;
/// Horizontal filter pill padding, in points. The stylesheet sets 8 px.
const FILTER_PAD_X: f32 = 8.0;
/// Vertical filter pill padding, in points. The stylesheet sets 2 px.
const FILTER_PAD_Y: f32 = 2.0;

impl Gallery {
    /// Draw the header, the info row, and the filter bar.
    ///
    /// The header draws first and refreshes the cached view. The info row and
    /// the grid then paint one set in one frame.
    pub(super) fn strip(&mut self, ui: &mut Ui) {
        Frame::new()
            .fill(tokens::BG_PRIMARY)
            .inner_margin(Margin::symmetric(
                points(tokens::SPACING_S),
                points(tokens::SPACING_S),
            ))
            .show(ui, |ui| {
                self.header(ui);
                ui.add_space(tokens::SPACING_S);
                let picked = self.sort_segment(ui);
                if let Some(mode) = picked {
                    self.pick_sort_mode(mode);
                }
                let scope = self.filter_scope();
                let colors = self.colors_enabled;
                let (names, paths) = (&self.tags, &self.folders);
                self.filter_events |=
                    filter_bar::show(ui, &mut self.bar, names, paths, scope, colors);
                // The flag latches, so read it once and clear it. One click
                // then opens the window once, and the user can close it.
                if std::mem::take(&mut self.filter_events.manage_tags) {
                    self.tag_manager.open();
                    // The app refreshes the rows when a menu opens.
                    self.filter_events.opened = true;
                }
            });
    }

    /// Draw the filter pill and the sort pills at the right of one line.
    /// Returns the picked sort mode. The dock keeps the gallery wide enough
    /// for them.
    fn sort_segment(&self, ui: &mut Ui) -> Option<SortMode> {
        let mut picked = None;
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                picked = self.sort_controls(ui);
            });
        });
        picked
    }

    /// The filter pill text, or none when no filter is active.
    fn active_filter_text(&self) -> Option<String> {
        filter_pill_text(self.bar.state.active_count(self.filter_scope()))
    }

    /// Draw the filter pill, the sort pills, and the label in a right-to-left
    /// layout. Returns the picked sort mode.
    fn sort_controls(&self, ui: &mut Ui) -> Option<SortMode> {
        let mut picked = None;
        // Right-to-left layout puts the label last on the screen.
        ui.spacing_mut().item_spacing.x = SORT_GAP;
        // The pill draws before the bar, so a bar click shows next frame.
        if let Some(text) = self.active_filter_text() {
            let style = (tokens::AMBER_DARK, tokens::AMBER_ACCENT, Stroke::NONE);
            let pad = vec2(FILTER_PAD_X, FILTER_PAD_Y);
            pill_frame(ui, &text, style, pad);
        }
        for mode in SortMode::ALL.into_iter().rev() {
            if sort_pill(ui, mode, self.sort_mode == mode).clicked() {
                picked = Some(mode);
            }
        }
        ui.label(
            RichText::new("Sort")
                .size(tokens::SIZE_SMALL)
                .color(tokens::TEXT_SECONDARY),
        );
        picked
    }
}

/// Draw one sort pill and return its click response.
///
/// The active pill takes the amber text on the tertiary fill with an amber
/// border. The inactive pill takes the secondary text on the secondary fill.
fn sort_pill(ui: &mut Ui, mode: SortMode, active: bool) -> Response {
    let (fill, color, border) = if active {
        (
            tokens::BG_TERTIARY,
            tokens::AMBER_ACCENT,
            Stroke::new(1.0, tokens::AMBER_ACCENT),
        )
    } else {
        (tokens::BG_SECONDARY, tokens::TEXT_SECONDARY, Stroke::NONE)
    };
    let pad = vec2(PILL_PAD_X, PILL_PAD_Y);
    let rect = pill_frame(ui, sort_label(mode), (fill, color, border), pad);
    ui.interact(
        rect,
        Id::new(("tarragon-sort-pill", mode.as_str())),
        Sense::click(),
    )
}

/// Paint a caption pill from its fill, text color, border, and padding.
/// Returns its rect.
fn pill_frame(
    ui: &mut Ui,
    text: &str,
    (fill, color, border): (Color32, Color32, Stroke),
    pad: Vec2,
) -> Rect {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::proportional(tokens::SIZE_CAPTION),
        color,
    );
    let (rect, _) = ui.allocate_exact_size(galley.size() + 2.0 * pad, Sense::hover());
    let painter = ui.painter();
    let radius = CornerRadius::same(tokens::RADIUS_M);
    painter.rect(rect, radius, fill, border, StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    rect
}

/// The filter pill text, or none when no filter is active.
pub(crate) fn filter_pill_text(count: usize) -> Option<String> {
    let noun = if count == 1 { "filter" } else { "filters" };
    (count > 0).then(|| format!("{count} {noun} active"))
}

/// The sort label for a mode.
pub(crate) const fn sort_label(mode: SortMode) -> &'static str {
    match mode {
        SortMode::Natural => "Name",
        SortMode::Date => "Date",
    }
}
