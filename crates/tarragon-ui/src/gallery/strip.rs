//! The gallery top strip: tabs, search field, info bar, and sort pills.

use std::time::Instant;

use egui::{
    Align, CornerRadius, FontId, Frame, Id, Layout, Margin, Response, RichText, Sense, Stroke,
    StrokeKind, TextEdit, Ui, vec2,
};
use tarragon_core::sorting::SortMode;

use super::{Gallery, Scope};
use crate::theme::points;
use crate::tokens;

/// Horizontal tab padding, in points. The stylesheet sets 16 px.
const TAB_PAD_X: f32 = 16.0;
/// Vertical tab padding, in points. The stylesheet sets 5 px.
const TAB_PAD_Y: f32 = 5.0;
/// Gap between two tabs, in points. The stylesheet sets 2 px.
const TAB_GAP: f32 = 2.0;
/// Gap between the sort label and a pill, in points.
const SORT_GAP: f32 = 6.0;
/// Horizontal pill padding, in points.
const PILL_PAD_X: f32 = 10.0;
/// Vertical pill padding, in points.
const PILL_PAD_Y: f32 = 3.0;

impl Gallery {
    /// Draw the tab bar, the search field, the info bar, and the sort pills.
    ///
    /// The tabs and the search field draw first and refresh the cached view.
    /// The info bar and the grid then paint one set in one frame.
    pub(super) fn strip(&mut self, ui: &mut Ui, now: Instant) {
        Frame::new()
            .fill(tokens::BG_PRIMARY)
            .inner_margin(Margin::symmetric(
                points(tokens::SPACING_S),
                points(tokens::SPACING_S),
            ))
            .show(ui, |ui| {
                let width = ui.available_width();
                let tabs = ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = TAB_GAP;
                    let mut picked = None;
                    for scope in [Scope::Folder, Scope::All] {
                        if tab(ui, scope.label(), self.scope == scope).clicked() {
                            picked = Some(scope);
                        }
                    }
                    picked
                });
                let row = tabs.response.rect;
                ui.painter().hline(
                    row.min.x..=row.min.x + width,
                    row.max.y,
                    Stroke::new(1.0, tokens::SEPARATOR),
                );
                if let Some(scope) = tabs.inner {
                    self.set_scope(scope);
                }

                ui.add_space(tokens::SPACING_XS);
                self.search_field(ui, now);
                let picked = self.sort_segment(ui);
                if let Some(mode) = picked {
                    self.pick_sort_mode(mode);
                }
            });
    }

    /// Draw the search field and re-arm the timer on a change.
    fn search_field(&mut self, ui: &mut Ui, now: Instant) {
        let response = ui.add(
            TextEdit::singleline(&mut self.search)
                .id(Id::new("tarragon-gallery-search"))
                .hint_text("Search file names")
                .text_color(tokens::TEXT_MUTED)
                .desired_width(f32::INFINITY)
                .frame(
                    Frame::new()
                        .fill(tokens::BG_SECONDARY)
                        .corner_radius(CornerRadius::same(tokens::RADIUS_M))
                        .inner_margin(Margin::symmetric(
                            points(tokens::SPACING_S),
                            points(tokens::SPACING_XS),
                        )),
                ),
        );
        if response.has_focus() {
            ui.painter().rect_stroke(
                response.rect,
                CornerRadius::same(tokens::RADIUS_M),
                Stroke::new(1.0, tokens::CORAL_STRONG),
                StrokeKind::Inside,
            );
        }
        if response.changed() {
            // The gallery asks for the wake frame again on every pass.
            self.arm_search(now);
        }
    }

    /// Draw the info bar and the sort pills. Returns the picked sort mode.
    fn sort_segment(&self, ui: &mut Ui) -> Option<SortMode> {
        let info = self.info_text();
        let mut picked = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(info)
                    .size(tokens::SIZE_CAPTION)
                    .color(tokens::TEXT_MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Right-to-left layout puts the label last on the screen.
                ui.spacing_mut().item_spacing.x = SORT_GAP;
                for mode in SortMode::ALL.into_iter().rev() {
                    if sort_pill(ui, mode, self.sort_mode == mode).clicked() {
                        picked = Some(mode);
                    }
                }
                ui.label(
                    RichText::new("Sort")
                        .size(tokens::SIZE_SMALL)
                        .color(tokens::TEXT_MUTED),
                );
            });
        });
        picked
    }
}

/// Draw one scope tab and return its click response.
fn tab(ui: &mut Ui, label: &str, active: bool) -> Response {
    let (fill, color) = if active {
        (tokens::BG_SECONDARY, tokens::AMBER_ACCENT)
    } else {
        (tokens::BG_TERTIARY, tokens::TEXT_SECONDARY)
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(tokens::SIZE_BODY),
        color,
    );
    let size = galley.size() + vec2(2.0 * TAB_PAD_X, 2.0 * TAB_PAD_Y);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, Id::new(("tarragon-scope-tab", label)), Sense::click());
    let radius = CornerRadius {
        nw: tokens::RADIUS_S,
        ne: tokens::RADIUS_S,
        sw: 0,
        se: 0,
    };
    let painter = ui.painter();
    painter.rect(rect, radius, fill, Stroke::NONE, StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    if active {
        painter.hline(
            rect.x_range(),
            rect.max.y - 0.5,
            Stroke::new(1.0, tokens::AMBER_ACCENT),
        );
    }
    response
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
    let galley = ui.painter().layout_no_wrap(
        sort_label(mode).to_owned(),
        FontId::proportional(tokens::SIZE_CAPTION),
        color,
    );
    let size = galley.size() + vec2(2.0 * PILL_PAD_X, 2.0 * PILL_PAD_Y);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let id = Id::new(("tarragon-sort-pill", mode.as_str()));
    let response = ui.interact(rect, id, Sense::click());
    let radius = CornerRadius::same(tokens::RADIUS_M);
    let painter = ui.painter();
    painter.rect(rect, radius, fill, border, StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    response
}

/// The sort label for a mode.
pub(crate) const fn sort_label(mode: SortMode) -> &'static str {
    match mode {
        SortMode::Natural => "Name",
        SortMode::Date => "Date",
    }
}
