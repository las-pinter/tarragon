//! One folder tree row: indent, caret slot, folder icon, and a truncated name.
//!
//! The sizes follow the tree rules in design/mockups/theme.css.

use egui::text::{LayoutJob, TextWrapping};
use egui::{CornerRadius, FontId, Id, Rect, Sense, Ui, pos2, vec2};

use super::paint::{ICON_HEIGHT, ICON_WIDTH, paint_caret, paint_folder_icon};
use super::tree::TreeRow;
use crate::tokens;

/// Row height, in points: 4 px padding twice plus a 12 px line at 1.45.
const ROW_HEIGHT: f32 = 25.0;
/// Row padding at the left and right edge, in points.
const ROW_PAD: f32 = 6.0;
/// Extra indent for each depth level, in points.
const INDENT_STEP: f32 = 14.0;
/// Gap between the caret slot, the icon, and the name, in points.
const GAP: f32 = 6.0;
/// Caret slot width, in points. It stays when a leaf draws no caret.
const CARET_SLOT: f32 = 4.0;
/// How far the caret click area reaches past its slot, in points.
const CARET_REACH: f32 = 3.0;

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
fn row_id(ui: &Ui, salt: usize) -> Id {
    ui.id().with(("tarragon-sidebar-row", salt))
}

/// Draw one row at the full panel width and report the action.
///
/// The caret click area sits above the row, so a caret click never also
/// counts as a row click. The caller sets `item_spacing.y` to 0 for a gapless
/// list and owns the padding around the whole tree. The focus outline and
/// screen-reader information are not drawn yet.
pub fn show_tree_row(ui: &mut Ui, row: &TreeRow, selected: bool, salt: usize) -> RowAction {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let id = row_id(ui, salt);
    let body = ui.interact(rect, id, Sense::click());
    #[expect(
        clippy::cast_precision_loss,
        reason = "a tree is far shallower than 2^24 levels"
    )]
    let x = rect.min.x + ROW_PAD + INDENT_STEP * row.depth as f32;
    let slot = Rect::from_min_size(pos2(x, rect.min.y), vec2(CARET_SLOT, ROW_HEIGHT));
    let caret = row.has_caret.then(|| {
        let area = slot.expand2(vec2(CARET_REACH, 0.0));
        ui.interact(area, id.with("caret"), Sense::click())
    });
    let hovered = body.hovered() || caret.as_ref().is_some_and(egui::Response::hovered);

    let (fill, text) = if selected {
        (tokens::BG_TERTIARY, tokens::TEXT_PRIMARY)
    } else if hovered {
        (tokens::SURFACE_HOVER, tokens::TEXT_PRIMARY)
    } else {
        (egui::Color32::TRANSPARENT, tokens::TEXT_SECONDARY)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(tokens::RADIUS_M), fill);
    if row.has_caret {
        paint_caret(painter, slot, row.expanded, text);
    }
    let icon_x = slot.max.x + GAP;
    let icon_color = if selected {
        tokens::CORAL_STRONG
    } else {
        tokens::TEXT_SECONDARY
    };
    let icon_y = (rect.center().y - ICON_HEIGHT / 2.0).round();
    paint_folder_icon(painter, pos2(icon_x, icon_y), icon_color);

    let name_x = icon_x + ICON_WIDTH + GAP;
    let mut job = LayoutJob::simple_singleline(
        row.name.clone(),
        FontId::proportional(tokens::SIZE_BODY),
        text,
    );
    job.wrap = TextWrapping {
        max_width: (rect.max.x - ROW_PAD - name_x).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = painter.layout_job(job);
    painter.galley(
        pos2(name_x, rect.center().y - galley.size().y / 2.0),
        galley,
        text,
    );

    if caret.is_some_and(|caret| caret.clicked()) {
        RowAction::Toggle
    } else if body.clicked() {
        RowAction::Navigate
    } else {
        RowAction::None
    }
}

#[cfg(test)]
mod tests;
