//! One folder tree row: indent, chevron caret, folder outline, and a truncated name.
//!
//! The sizes follow the Folders list of design/redesign/main-window.html.

use egui::text::{LayoutJob, TextWrapping};
use egui::{Color32, CornerRadius, FontFamily, FontId, Id, Painter, Rect, Sense, Ui, vec2};

use super::paint::{CARET_SIZE, ICON_SIZE, icon_origin, paint_caret, paint_folder_icon};
use super::tree::TreeRow;
use crate::{fonts, tokens};

/// Row height, in points.
pub(super) const ROW_HEIGHT: f32 = 34.0;
/// Row padding at the left and right edge, in points.
pub(super) const ROW_PAD: f32 = 8.0;
/// Extra indent for each depth level, in points.
const INDENT_STEP: f32 = 16.0;
/// Gap between the caret, the icon, and the name, in points.
pub(super) const GAP: f32 = 8.0;
/// How far the caret click area reaches past its square, in points.
const CARET_REACH: f32 = 4.0;

/// What the user did to a row this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowAction {
    /// Nothing happened.
    None,
    /// The row was clicked, so the folder opens in the gallery.
    Navigate,
    /// The caret was clicked, so the folder expands or collapses.
    Toggle,
}

/// The id of a row, scoped to the ui; two lists in one ui need their own `push_id`.
///
/// The caller's `salt` keeps it unique when a path shows twice.
pub(super) fn row_id(ui: &Ui, salt: usize) -> Id {
    ui.id().with(("tarragon-sidebar-row", salt))
}

/// Draw one row at the full width and report the action.
///
/// A `count` above zero paints at the right edge in the caption size, and the
/// name truncates before it; zero or none paints nothing there.
///
/// The caret click area sits above the row, so a caret click never also
/// counts as a row click. A leaf keeps the caret square empty, so the icons
/// of one depth line up. The caller sets the row gap and owns the padding
/// around the whole tree. The focus outline and screen-reader information are
/// not drawn yet.
pub fn show_tree_row(
    ui: &mut Ui,
    row: &TreeRow,
    selected: bool,
    count: Option<u32>,
    salt: usize,
) -> RowAction {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let id = row_id(ui, salt);
    let body = ui.interact(rect, id, Sense::click());
    #[expect(
        clippy::cast_precision_loss,
        reason = "a tree is far shallower than 2^24 levels"
    )]
    let x = rect.min.x + ROW_PAD + INDENT_STEP * row.depth as f32;
    let caret_origin = icon_origin(rect, x, CARET_SIZE);
    let caret = row.has_caret.then(|| {
        let square = Rect::from_min_size(caret_origin, vec2(CARET_SIZE, CARET_SIZE));
        let area =
            Rect::from_x_y_ranges(square.x_range(), rect.y_range()).expand2(vec2(CARET_REACH, 0.0));
        ui.interact(area, id.with("caret"), Sense::click())
    });
    let hovered = body.hovered() || caret.as_ref().is_some_and(egui::Response::hovered);

    let (fill, text) = row_colors(selected, hovered);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(tokens::RADIUS_M), fill);
    if row.has_caret {
        paint_caret(painter, caret_origin, row.expanded, text);
    }
    let icon_x = x + CARET_SIZE + GAP;
    paint_folder_icon(painter, icon_origin(rect, icon_x, ICON_SIZE), text);
    let name_x = icon_x + ICON_SIZE + GAP;
    let right = rect.max.x - ROW_PAD;
    let name_right = match count.filter(|count| *count > 0) {
        Some(count) => paint_count(painter, count, right, rect) - GAP,
        None => right,
    };
    paint_name(painter, &row.name, name_x, name_right, rect, text, selected);

    if caret.is_some_and(|caret| caret.clicked()) {
        RowAction::Toggle
    } else if body.clicked() {
        RowAction::Navigate
    } else {
        RowAction::None
    }
}

/// The fill and text colors of a row: the coral selection, the hover step,
/// or no fill. The marks take the text color, as the page draws them.
pub(super) fn row_colors(selected: bool, hovered: bool) -> (Color32, Color32) {
    if selected {
        (tokens::CORAL_DARK, tokens::CORAL_STRONG)
    } else if hovered {
        (tokens::SURFACE_HOVER, tokens::TEXT_PRIMARY)
    } else {
        (Color32::TRANSPARENT, tokens::TEXT_PRIMARY)
    }
}

/// Paint `count` right-aligned at `right`, centred in `rect`, and return its
/// left edge. The page gives it the tertiary text on every fill, which passes
/// on the coral selection too.
fn paint_count(painter: &Painter, count: u32, right: f32, rect: Rect) -> f32 {
    let galley = painter.layout_no_wrap(
        count.to_string(),
        FontId::proportional(tokens::SIZE_CAPTION),
        tokens::TEXT_TERTIARY,
    );
    let left = right - galley.size().x;
    let y = rect.center().y - galley.size().y / 2.0;
    painter.galley(egui::pos2(left, y), galley, tokens::TEXT_TERTIARY);
    left
}

/// Draw a one-line name from `x`, centred in `rect`, cut with an ellipsis
/// before `right`. A selected name takes the medium face.
pub(super) fn paint_name(
    painter: &Painter,
    name: &str,
    x: f32,
    right: f32,
    rect: Rect,
    color: Color32,
    selected: bool,
) {
    let family = if selected {
        FontFamily::Name(fonts::MEDIUM_FAMILY.into())
    } else {
        FontFamily::Proportional
    };
    let mut job = LayoutJob::simple_singleline(
        name.to_owned(),
        FontId::new(tokens::SIZE_BODY, family),
        color,
    );
    job.wrap = TextWrapping {
        max_width: (right - x).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
}

#[cfg(test)]
mod tests;
