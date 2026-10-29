//! The "+ add" control: a ghost button, a menu of known tags, and the inline
//! input for a new tag name.

use egui::{
    Area, CornerRadius, CursorIcon, FontId, Frame, Id, Key, Margin, Order, Response, ScrollArea,
    Sense, Stroke, StrokeKind, TextEdit, Ui, text::LayoutJob, text_edit::TextEditState, vec2,
};

use super::PreviewEvent;
use crate::theme::points;
use crate::tokens;

/// The label of the menu row that opens the name input.
pub const CREATE_NEW: &str = "Create new...";

/// How many tag rows the menu shows before it scrolls.
const MENU_ROWS: f32 = 8.0;

/// Which part of the control is open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AddState {
    /// Only the "+ add" button shows.
    #[default]
    Closed,
    /// The menu of known tags is open under the button.
    Menu,
    /// The button is swapped for the name input.
    Input {
        /// The text typed so far.
        text: String,
        /// Whether the input has asked for focus yet; losing focus before
        /// that is not a cancel.
        focused: bool,
    },
}

/// Draw the ghost button. A click opens the menu, or closes it when open.
pub fn show_button(ui: &mut Ui, state: &mut AddState) -> Response {
    let font = FontId::proportional(tokens::SIZE_CAPTION);
    let galley = ui
        .painter()
        .layout_no_wrap("+ add".to_owned(), font, tokens::TEXT_MUTED);
    let pad = vec2(tokens::SPACING_S, tokens::SPACING_XS / 2.0);
    let (rect, response) = ui.allocate_exact_size(galley.size() + pad * 2.0, Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let radius = CornerRadius::same(tokens::RADIUS_XL);
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, radius, tokens::SURFACE_HIGHLIGHT);
    }
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(tokens::STROKE_WIDTH, tokens::BORDER_INTERACTIVE),
        StrokeKind::Inside,
    );
    let color = if hovered {
        tokens::TEXT_SECONDARY
    } else {
        tokens::TEXT_MUTED
    };
    painter.galley(rect.min + pad, galley, color);
    if response.clicked() {
        *state = if *state == AddState::Menu {
            AddState::Closed
        } else {
            AddState::Menu
        };
    }
    response
}

/// Draw the menu under `button`. A row applies its tag; "Create new..." opens
/// the input. Escape or a press outside closes it with no event.
pub fn show_menu(
    ui: &Ui,
    id: Id,
    button: &Response,
    choices: &[&str],
    state: &mut AddState,
    events: &mut Vec<PreviewEvent>,
) {
    let pos = button.rect.left_bottom() + vec2(0.0, tokens::SPACING_XS);
    let room = ui.max_rect().right() - pos.x;
    let area = Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(pos)
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(tokens::BG_TERTIARY)
                .stroke(Stroke::new(tokens::STROKE_WIDTH, tokens::SURFACE_HIGHLIGHT))
                .corner_radius(CornerRadius::same(tokens::RADIUS_M))
                .inner_margin(Margin::same(points(tokens::SPACING_XS)))
                .show(ui, |ui| {
                    let mut next = None;
                    // The rows share the width of the longest one. Filling the
                    // free width would stretch the menu to the screen edge.
                    // A long name must not push the menu past the pane, so rows
                    // cap at the room left in the pane and truncate with an ellipsis.
                    let width = (choices
                        .iter()
                        .chain([&CREATE_NEW])
                        .map(|text| text_width(ui, text))
                        .fold(0.0, f32::max)
                        + 2.0 * tokens::SPACING_S)
                        .min(
                            room - tokens::STROKE_WIDTH * 2.0
                                - 2.0 * tokens::SPACING_XS
                                - ui.spacing().scroll.allocated_width(),
                        )
                        // A very narrow pane must not give a negative width.
                        .max(2.0 * tokens::SPACING_S);
                    // Only the tags scroll, so "Create new..." stays reachable
                    // however many tags there are.
                    let row =
                        tokens::SIZE_BODY + 2.0 * tokens::SPACING_XS + ui.spacing().item_spacing.y;
                    ScrollArea::vertical()
                        .max_height(MENU_ROWS * row)
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            for name in choices {
                                if menu_row(ui, name, width).clicked() {
                                    events.push(PreviewEvent::TagApplied((*name).to_owned()));
                                    next = Some(AddState::Closed);
                                }
                            }
                        });
                    if !choices.is_empty() {
                        ui.separator();
                    }
                    if menu_row(ui, CREATE_NEW, width).clicked() {
                        next = Some(AddState::Input {
                            text: String::new(),
                            focused: false,
                        });
                    }
                    next
                })
                .inner
        });
    if let Some(next) = area.inner {
        *state = next;
        return;
    }
    let escape = ui.input(|i| i.key_pressed(Key::Escape));
    let outside = ui.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !area.response.rect.contains(p) && !button.rect.contains(p))
    });
    if escape || outside {
        *state = AddState::Closed;
    }
}

/// The width of `text` in the menu font.
fn text_width(ui: &Ui, text: &str) -> f32 {
    let font = FontId::proportional(tokens::SIZE_BODY);
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, tokens::TEXT_PRIMARY)
        .size()
        .x
}

/// One menu row, `width` wide so the whole row takes the click.
fn menu_row(ui: &mut Ui, text: &str, width: f32) -> Response {
    let font = FontId::proportional(tokens::SIZE_BODY);
    let pad = vec2(tokens::SPACING_S, tokens::SPACING_XS);
    let mut job = LayoutJob::simple(
        text.to_owned(),
        font,
        tokens::TEXT_PRIMARY,
        (width - 2.0 * pad.x).max(0.0),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, galley.size().y + pad.y * 2.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    let color = if response.hovered() {
        painter.rect_filled(
            rect,
            CornerRadius::same(tokens::RADIUS_S),
            tokens::SURFACE_HIGHLIGHT,
        );
        tokens::AMBER_ACCENT
    } else {
        tokens::TEXT_PRIMARY
    };
    painter.galley(rect.min + pad, galley, color);
    response
}

/// Draw the name input and its hint. Enter applies a non-empty trimmed name.
/// Enter on a blank name, Escape, and losing focus close it with no event.
pub fn show_input(ui: &mut Ui, id: Id, state: &mut AddState, events: &mut Vec<PreviewEvent>) {
    let AddState::Input { text, focused } = state else {
        return;
    };
    if !*focused {
        // Reset paths only clear `text`; egui keeps the cursor and undo history
        // under this id, so a fresh input drops them or Ctrl+Z restores old text.
        ui.data_mut(|d| d.remove::<TextEditState>(id));
    }
    let response = ui.add(
        TextEdit::singleline(text)
            .id(id)
            .hint_text("Tag name...")
            .desired_width(f32::INFINITY)
            .frame(
                Frame::new()
                    .fill(tokens::BG_TERTIARY)
                    .corner_radius(CornerRadius::same(tokens::RADIUS_S))
                    .inner_margin(Margin::symmetric(
                        points(tokens::SPACING_S),
                        points(tokens::SPACING_XS),
                    )),
            ),
    );
    ui.painter().rect_stroke(
        response.rect,
        CornerRadius::same(tokens::RADIUS_S),
        Stroke::new(tokens::STROKE_WIDTH, tokens::CORAL_STRONG),
        StrokeKind::Inside,
    );
    ui.label(
        egui::RichText::new("Press Enter to add. Escape or focus loss cancels.")
            .size(tokens::SIZE_SMALL)
            .color(tokens::TEXT_MUTED),
    );
    if !*focused {
        response.request_focus();
        *focused = true;
        return;
    }
    if response.lost_focus() {
        let enter = ui.input(|i| i.key_pressed(Key::Enter));
        let name = text.trim();
        if enter && !name.is_empty() {
            events.push(PreviewEvent::TagApplied(name.to_owned()));
        }
        *state = AddState::Closed;
    }
}

#[cfg(test)]
mod tests;
