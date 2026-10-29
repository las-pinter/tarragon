//! The Tags heading and the row of user tag pills.

use egui::{Color32, CornerRadius, CursorIcon, FontId, Rect, RichText, Sense, Ui, pos2, vec2};

use super::add::{self, AddState};
use super::{ColorState, PreviewEvent};
use crate::tokens;

/// One user tag and how many of the selected images carry it.
///
/// The row lists the union of the tags, so `None` is not expected; it paints
/// at its own dim opacity if a caller sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagPill {
    /// The tag name as the user wrote it.
    pub name: String,
    /// `All` when every selected image has it, `Some` when only a few do.
    pub state: ColorState,
}

/// The heading stays when the colour squares are hidden, because the pills
/// share it.
pub fn show_heading(ui: &mut Ui) {
    ui.add_space(tokens::SPACING_S);
    ui.label(
        RichText::new("Tags")
            .size(tokens::SIZE_CAPTION)
            .color(tokens::TEXT_MUTED),
    );
}

/// Draw the pills, wrapping to new lines, then the "+ add" button on its own
/// row below them.
/// The x on a pill removes the tag; a click on the rest of a partial pill
/// applies the tag to every selected image.
pub fn show_pills(
    ui: &mut Ui,
    pills: &[TagPill],
    known: &[String],
    add: &mut AddState,
    events: &mut Vec<PreviewEvent>,
) {
    // The pill row keeps a minimum height, so the button stays put as pills
    // come and go.
    ui.scope(|ui| {
        ui.set_min_height(tokens::PREVIEW_TAG_ROW_HEIGHT);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(tokens::SPACING_XS, tokens::SPACING_XS);
            for pill in pills {
                show_pill(ui, pill, events);
            }
        });
    });
    // The input replaces the button, as in the mockup.
    let button = (!matches!(add, AddState::Input { .. })).then(|| add::show_button(ui, add));
    if let (Some(button), AddState::Menu) = (&button, &*add) {
        // A fully applied tag has nothing to add; a partial one still does.
        let full = |name: &str| {
            pills
                .iter()
                .any(|p| p.name == name && p.state == ColorState::All)
        };
        let choices: Vec<&str> = known
            .iter()
            .map(String::as_str)
            .filter(|name| !full(name))
            .collect();
        add::show_menu(ui, ui.id().with("add-menu"), button, &choices, add, events);
    }
    add::show_input(ui, ui.id().with("add-input"), add, events);
}

fn show_pill(ui: &mut Ui, pill: &TagPill, events: &mut Vec<PreviewEvent>) {
    let opacity = pill.state.opacity();
    let font = FontId::proportional(tokens::SIZE_SMALL);
    let fade = |color: Color32| color.gamma_multiply(opacity);
    let name =
        ui.painter()
            .layout_no_wrap(pill.name.clone(), font.clone(), fade(tokens::CORAL_STRONG));
    let cross_width = ui
        .painter()
        .layout_no_wrap("x".to_owned(), font.clone(), Color32::WHITE)
        .size()
        .x;
    // The cross has its room reserved, so the pill does not jump on hover.
    let pad = vec2(tokens::SPACING_S, tokens::SPACING_XS / 2.0);
    let width = pad.x * 2.0 + name.size().x + tokens::SPACING_XS + cross_width;
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, name.size().y + pad.y * 2.0), Sense::click());
    let cross = Rect::from_min_max(pos2(rect.max.x - pad.x - cross_width, rect.min.y), rect.max);
    let hover = response.hover_pos();
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(tokens::RADIUS_XL),
        fade(tokens::CORAL_DARK),
    );
    painter.galley(rect.min + pad, name, tokens::CORAL_STRONG);
    if let Some(at) = hover {
        let pointer_on_cross = cross.contains(at);
        // Only a click that does something gets the hand.
        if pointer_on_cross || pill.state != ColorState::All {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let color = if pointer_on_cross {
            tokens::CORAL_STRONG
        } else {
            tokens::CORAL_MUTED
        };
        let galley = ui
            .painter()
            .layout_no_wrap("x".to_owned(), font, fade(color));
        ui.painter()
            .galley(pos2(cross.min.x, rect.min.y + pad.y), galley, color);
    }
    if response.clicked() {
        let on_cross = response
            .interact_pointer_pos()
            .is_some_and(|p| cross.contains(p));
        if on_cross {
            events.push(PreviewEvent::TagRemoved(pill.name.clone()));
        } else if pill.state != ColorState::All {
            events.push(PreviewEvent::TagApplied(pill.name.clone()));
        }
    }
}

#[cfg(test)]
mod tests;
