//! One favorite row and the empty-list text.
//!
//! The sizes follow the favorites rules in design/mockups/theme.css.

use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Frame, Galley, Margin, Painter, Rect, RichText,
    Sense, Shape, Stroke, TextFormat, Ui, pos2, vec2,
};

use super::paint::{ICON_WIDTH, paint_folder_icon};
use super::row::{GAP, ROW_PAD, icon_y, paint_name, row_colors, row_id};
use crate::tokens;

/// Row height, in points: 5 px padding twice plus a 12 px line at 1.45.
pub(super) const FAVORITE_HEIGHT: f32 = 27.0;
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
/// row never starts a drag. The row draws no focus outline. The cursor is a
/// grab hand over the grip and a closed hand while the grip is dragged.
///
/// The row takes its place in the layout and is drawn `shift` points below
/// it, so rows can slide during a drag. With `lifted` set the row paints
/// nothing: the caller draws it last with `paint_lifted_row`, above the other
/// rows.
///
/// The returned rect is the row's place in the layout, without the shift, so
/// the caller can find the drop slot.
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
    lifted: bool,
    salt: usize,
    shift: f32,
) -> (FavoriteAction, Rect) {
    let (outer, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), FAVORITE_HEIGHT), Sense::hover());
    let home = outer.shrink2(vec2(MARGIN, 0.0));
    let rect = home.translate(vec2(0.0, shift));
    let id = row_id(ui, salt);
    let body = ui.interact(rect, id, Sense::click());
    let grip = grip_galley(ui);
    let grip_rect = Rect::from_min_size(
        pos2(rect.min.x + ROW_PAD, rect.min.y),
        vec2(grip.size().x, rect.height()),
    );
    let handle = ui.interact(grip_rect, id.with("grip"), Sense::drag());
    if handle.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    } else if handle.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    if !lifted {
        let (fill, text, icon) = row_colors(selected, body.hovered());
        paint_row(
            ui.painter(),
            &grip,
            label,
            rect,
            fill,
            colors(missing, text, icon),
        );
    }

    let action = if handle.drag_started() {
        FavoriteAction::DragStarted
    } else if handle.drag_stopped() {
        FavoriteAction::Dropped
    } else if body.clicked() {
        FavoriteAction::Open
    } else {
        FavoriteAction::None
    };
    (action, home)
}

/// Draw the lifted row at `rect`: the highlight fill, dimmed to 0.75.
///
/// The panel calls it after the other rows, so the lifted row covers them.
pub fn paint_lifted_row(ui: &Ui, label: &str, selected: bool, missing: bool, rect: Rect) {
    let grip = grip_galley(ui);
    let (_, text, icon) = row_colors(selected, false);
    let mut painter = ui.painter().clone();
    // The mockup dims the dragged row to 0.75; `paint_lifted_border` dashes the border.
    painter.set_opacity(0.75);
    paint_row(
        &painter,
        &grip,
        label,
        rect,
        tokens::SURFACE_HIGHLIGHT,
        colors(missing, text, icon),
    );
}

/// The text and icon colours: muted for a missing folder.
///
/// Muted text is the dimmest readable token, so a missing folder reads as gone.
fn colors(missing: bool, text: Color32, icon: Color32) -> (Color32, Color32) {
    if missing {
        (tokens::TEXT_MUTED, tokens::TEXT_MUTED)
    } else {
        (text, icon)
    }
}

/// The laid-out `::` grip.
fn grip_galley(ui: &Ui) -> Arc<Galley> {
    ui.painter().layout_job(LayoutJob::single_section(
        "::".to_owned(),
        TextFormat {
            font_id: FontId::proportional(tokens::SIZE_CAPTION),
            color: tokens::TEXT_MUTED,
            extra_letter_spacing: GRIP_SPACING,
            ..Default::default()
        },
    ))
}

/// Paint the row fill, grip, folder icon, and name at `rect`.
fn paint_row(
    painter: &Painter,
    grip: &Arc<Galley>,
    label: &str,
    rect: Rect,
    fill: Color32,
    (text, icon): (Color32, Color32),
) {
    let grip_x = rect.min.x + ROW_PAD;
    painter.rect_filled(rect, CornerRadius::same(tokens::RADIUS_M), fill);
    painter.galley(
        pos2(grip_x, rect.center().y - grip.size().y / 2.0),
        grip.clone(),
        tokens::TEXT_MUTED,
    );
    let icon_x = grip_x + grip.size().x + GAP;
    paint_folder_icon(painter, pos2(icon_x, icon_y(rect)), icon);
    paint_name(painter, label, icon_x + ICON_WIDTH + GAP, rect, text);
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
