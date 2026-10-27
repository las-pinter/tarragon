//! One favorite row and the empty-list text.
//!
//! The sizes follow the favorites rules in design/mockups/theme.css.

use egui::{CornerRadius, Frame, Margin, RichText, Sense, Ui, pos2, vec2};

use super::paint::{ICON_WIDTH, paint_folder_icon};
use super::row::{GAP, ROW_PAD, icon_y, paint_name, row_colors, row_id};
use crate::tokens;

/// Row height, in points: 5 px padding twice plus a 12 px line at 1.45.
const FAVORITE_HEIGHT: f32 = 27.0;
/// Space at the left and right of the row fill, in points.
const MARGIN: f32 = 6.0;

/// What the user did to a favorite row this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FavoriteAction {
    /// Nothing happened.
    None,
    /// The row was clicked, so the folder opens in the gallery.
    Open,
}

/// Draw one favorite row and report the action.
///
/// A missing folder draws dim but stays clickable, so the user can still
/// select and remove it. The drag grip and focus outline are not drawn yet.
///
/// The `salt` must be unique per row within the list. Favorites use the same
/// `row_id` scheme as the tree row, so two lists in one ui need their own
/// `push_id`.
pub fn show_favorite_row(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    missing: bool,
    salt: usize,
) -> FavoriteAction {
    let (outer, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), FAVORITE_HEIGHT), Sense::hover());
    let rect = outer.shrink2(vec2(MARGIN, 0.0));
    let body = ui.interact(rect, row_id(ui, salt), Sense::click());

    let (fill, text, icon) = row_colors(selected, body.hovered());
    // Muted text is the dimmest readable token, so a missing folder reads as gone.
    let (text, icon) = if missing {
        (tokens::TEXT_MUTED, tokens::TEXT_MUTED)
    } else {
        (text, icon)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(tokens::RADIUS_M), fill);
    let icon_x = rect.min.x + ROW_PAD;
    paint_folder_icon(painter, pos2(icon_x, icon_y(rect)), icon);
    paint_name(painter, label, icon_x + ICON_WIDTH + GAP, rect, text);

    if body.clicked() {
        FavoriteAction::Open
    } else {
        FavoriteAction::None
    }
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
