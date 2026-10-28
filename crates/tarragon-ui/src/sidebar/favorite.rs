//! One favorite row and the empty-list text.
//!
//! The sizes follow the favorites rules in design/mockups/theme.css.

use egui::text::LayoutJob;
use egui::{
    CornerRadius, FontId, Frame, Margin, Rect, RichText, Sense, Shape, Stroke, TextFormat, Ui,
    pos2, vec2,
};

use super::paint::{ICON_WIDTH, paint_folder_icon};
use super::row::{GAP, ROW_PAD, icon_y, paint_name, row_colors, row_id};
use crate::tokens;

/// Row height, in points: 5 px padding twice plus a 12 px line at 1.45.
const FAVORITE_HEIGHT: f32 = 27.0;
/// Space at the left and right of the row fill, in points.
const MARGIN: f32 = 6.0;

/// Grip letter spacing, in points: `.grip` in design/mockups/theme.css.
const GRIP_SPACING: f32 = -1.0;

/// What the user did to a favorite row this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FavoriteAction {
    /// Nothing happened.
    None,
    /// The row was clicked, so the folder opens in the gallery.
    Open,
    /// The user began to drag the grip.
    DragStarted,
    /// The user let go of the grip.
    Dropped,
}

/// Draw one favorite row and report the action.
///
/// A missing folder draws dim but stays clickable, so the user can still
/// select and remove it. The grip is the only drag handle, so a click on the
/// row never starts a drag. The row draws no focus outline.
///
/// With `dragging` set, the row draws the lifted fill and dim. The caller draws
/// the border with `paint_lifted_border` after the list.
///
/// The row rect comes back so the caller can find the drop slot.
///
/// `DragStarted` and `Dropped` win over `Open`. A `Dropped` in the same frame
/// as `DragStarted` is lost; the panel's lost-release guard ends that drag.
///
/// The `salt` must be unique per row within the list. Favorites use the same
/// `row_id` scheme as the tree row, so two lists in one ui need their own
/// `push_id`.
pub fn show_favorite_row(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    missing: bool,
    dragging: bool,
    salt: usize,
) -> (FavoriteAction, Rect) {
    let (outer, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), FAVORITE_HEIGHT), Sense::hover());
    let rect = outer.shrink2(vec2(MARGIN, 0.0));
    let id = row_id(ui, salt);
    let body = ui.interact(rect, id, Sense::click());
    let grip = ui.painter().layout_job(LayoutJob::single_section(
        "::".to_owned(),
        TextFormat {
            font_id: FontId::proportional(tokens::SIZE_CAPTION),
            color: tokens::TEXT_MUTED,
            extra_letter_spacing: GRIP_SPACING,
            ..Default::default()
        },
    ));
    let grip_x = rect.min.x + ROW_PAD;
    let grip_rect =
        Rect::from_min_size(pos2(grip_x, rect.min.y), vec2(grip.size().x, rect.height()));
    let handle = ui.interact(grip_rect, id.with("grip"), Sense::drag());

    let (fill, text, icon) = row_colors(selected, body.hovered());
    let fill = if dragging {
        tokens::SURFACE_HIGHLIGHT
    } else {
        fill
    };
    // Muted text is the dimmest readable token, so a missing folder reads as gone.
    let (text, icon) = if missing {
        (tokens::TEXT_MUTED, tokens::TEXT_MUTED)
    } else {
        (text, icon)
    };
    let mut painter = ui.painter().clone();
    if dragging {
        // The mockup dims the dragged row to 0.75; the caller dashes the border.
        painter.set_opacity(0.75);
    }
    painter.rect_filled(rect, CornerRadius::same(tokens::RADIUS_M), fill);
    painter.galley(
        pos2(grip_x, rect.center().y - grip.size().y / 2.0),
        grip.clone(),
        tokens::TEXT_MUTED,
    );
    let icon_x = grip_x + grip.size().x + GAP;
    paint_folder_icon(&painter, pos2(icon_x, icon_y(rect)), icon);
    paint_name(&painter, label, icon_x + ICON_WIDTH + GAP, rect, text);

    let action = if handle.drag_started() {
        FavoriteAction::DragStarted
    } else if handle.drag_stopped() {
        FavoriteAction::Dropped
    } else if body.clicked() {
        FavoriteAction::Open
    } else {
        FavoriteAction::None
    };
    (action, rect)
}

/// Draw the dashed amber border of the lifted row.
///
/// The panel calls it after the whole list. The stroke sits on the row edge,
/// so a later neighbour's fill would cover half of it if it were drawn in the row.
pub fn paint_lifted_border(ui: &Ui, rect: Rect) {
    let mut painter = ui.painter().clone();
    painter.set_opacity(0.75);
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    painter.extend(Shape::dashed_line(
        &corners,
        Stroke::new(1.0, tokens::AMBER_ACCENT),
        3.0,
        3.0,
    ));
}

/// Draw the muted text that shows when the user has no favorites.
pub fn show_empty_favorites(ui: &mut Ui) {
    Frame::new()
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.label(
                RichText::new("No favorites yet.")
                    .size(tokens::SIZE_SMALL)
                    .color(tokens::TEXT_MUTED),
            );
        });
}

#[cfg(test)]
mod tests;
