//! One favorite row and the empty-list text.
//!
//! The sizes follow the Favorites list of design/redesign/main-window.html.

use egui::{
    Color32, CornerRadius, CursorIcon, Frame, Margin, Painter, Rect, RichText, Sense, Shape,
    Stroke, Ui, vec2,
};

use super::paint::{ICON_SIZE, icon_origin, paint_star};
use super::row::{ROW_HEIGHT, ROW_PAD, paint_name, row_colors, row_id};
use crate::tokens;

/// Row height, in points: the height of a tree row.
pub(super) const FAVORITE_HEIGHT: f32 = ROW_HEIGHT;
/// Space at the left and right of the row fill, in points: the side padding
/// of the panel.
pub(super) const MARGIN: f32 = 12.0;
/// Gap between the star and the name, in points.
const STAR_GAP: f32 = 10.0;
/// The text of the context menu entry that removes a favorite.
pub const REMOVE_TEXT: &str = "Remove";

/// What the user did to a favorite row this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FavoriteAction {
    /// Nothing happened.
    None,
    /// The row was clicked, so the folder opens in the gallery.
    Open,
    /// The user began to drag the row.
    DragStarted,
    /// The user let go of the row.
    Dropped,
    /// The user chose Remove in the row's context menu.
    Remove,
}

/// Draw one favorite row and report the action.
///
/// A missing folder draws dim but stays clickable, so the user can still open
/// and remove it. The whole row is the drag handle: a press that moves past
/// egui's drag threshold starts a drag, and one that does not is a click. The
/// cursor is a closed hand while the row is dragged. A right click opens a
/// menu with Remove. The row draws no focus outline.
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
    let body = ui.interact(rect, id, Sense::click_and_drag());
    if body.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }
    let mut removed = false;
    body.context_menu(|ui| {
        if ui.button(REMOVE_TEXT).clicked() {
            removed = true;
            ui.close();
        }
    });
    if !lifted {
        let (fill, text) = row_colors(selected, body.hovered());
        paint_row(
            ui.painter(),
            label,
            rect,
            fill,
            colors(missing, text, fill),
            selected,
        );
    }

    let action = if removed {
        FavoriteAction::Remove
    } else if body.drag_started() {
        FavoriteAction::DragStarted
    } else if body.drag_stopped() {
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
    let (_, text) = row_colors(selected, false);
    let mut painter = ui.painter().clone();
    // The dragged row dims to 0.75; `paint_lifted_border` dashes the border.
    painter.set_opacity(0.75);
    paint_row(
        &painter,
        label,
        rect,
        tokens::SURFACE_HIGHLIGHT,
        colors(missing, text, tokens::SURFACE_HIGHLIGHT),
        selected,
    );
}

/// The text and star colors: amber for a folder on disk, muted for a missing one.
///
/// A missing folder keeps the lowest readable text for its fill: the floor on
/// the panel, and the lighter tertiary text on the hover, raised, and selected
/// fills, where the floor is too dark. Its star stays dim.
fn colors(missing: bool, text: Color32, fill: Color32) -> (Color32, Color32) {
    if !missing {
        return (text, tokens::AMBER_ACCENT);
    }
    let text = if fill == Color32::TRANSPARENT {
        tokens::TEXT_SECONDARY
    } else {
        tokens::TEXT_TERTIARY
    };
    (text, tokens::TEXT_MUTED)
}

/// Paint the row fill, star, and name at `rect`.
fn paint_row(
    painter: &Painter,
    label: &str,
    rect: Rect,
    fill: Color32,
    (text, star): (Color32, Color32),
    selected: bool,
) {
    painter.rect_filled(rect, CornerRadius::same(tokens::RADIUS_M), fill);
    let star_x = rect.min.x + ROW_PAD;
    paint_star(painter, icon_origin(rect, star_x, ICON_SIZE), star);
    let name_x = star_x + ICON_SIZE + STAR_GAP;
    paint_name(
        painter,
        label,
        name_x,
        rect.max.x - ROW_PAD,
        rect,
        text,
        selected,
    );
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
        .inner_margin(Margin::symmetric(20, 6))
        .show(ui, |ui| {
            ui.label(
                RichText::new("No favorites yet.")
                    .size(tokens::SIZE_SMALL)
                    .color(tokens::TEXT_SECONDARY),
            );
        });
}

#[cfg(test)]
mod tests;
