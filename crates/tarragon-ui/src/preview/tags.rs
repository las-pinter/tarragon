//! The chip row under the picture: the colour bars, the user tag pills,
//! and "+ Tag".

use egui::{
    Color32, CornerRadius, CursorIcon, FontFamily, FontId, Rect, Sense, Ui, Vec2, pos2, vec2,
};

use super::add::{self, AddState};
use super::{ColorState, ColorStates, PreviewEvent};
use crate::{fonts, tokens};

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

/// The padding inside a pill, in points.
const PILL_PAD: Vec2 = vec2(9.0, 3.0);
/// The pill corner radius: half the height, so the ends are round.
const PILL_RADIUS: u8 = 12;
/// The gap between the chips of the row, in points.
const CHIP_GAP: f32 = 6.0;
/// The gap after the colour bars, on top of the chip gap, in points.
const BARS_GAP: f32 = 6.0;

/// The tag colour pairs of the redesign palette, as fill and text.
pub(crate) const TAG_COLORS: [(Color32, Color32); 7] = [
    (tokens::BADGE_BG_VINOUS, tokens::BADGE_FG_VINOUS),
    (tokens::BADGE_BG_SAGE, tokens::BADGE_FG_SAGE),
    (tokens::BADGE_BG_NAVY, tokens::BADGE_FG_NAVY),
    (tokens::BADGE_BG_UMBER, tokens::BADGE_FG_UMBER),
    (tokens::BADGE_BG_PLUM, tokens::BADGE_FG_PLUM),
    (tokens::BADGE_BG_TEAL, tokens::BADGE_FG_TEAL),
    (tokens::BADGE_BG_OLIVE, tokens::BADGE_FG_OLIVE),
];

/// The fill and text colour of a tag: one palette pair, picked by a hash of
/// the name that does not change between runs, so a tag keeps its colour.
#[must_use]
pub fn tag_colors(name: &str) -> (Color32, Color32) {
    // FNV-1a, 32 bit.
    let hash = name.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    let index = usize::try_from(hash).unwrap_or(0) % TAG_COLORS.len();
    TAG_COLORS[index]
}

/// Draw one wrapping row: the colour bars when `colors` is given, the tag
/// pills, then "+ Tag". The x on a pill removes the tag; a click on the rest
/// of a partial pill applies the tag to every selected image.
pub fn show_chips(
    ui: &mut Ui,
    colors: Option<&ColorStates>,
    pills: &[TagPill],
    known: &[String],
    add: &mut AddState,
    events: &mut Vec<PreviewEvent>,
) {
    let mut button = None;
    // The row is one chip tall, so the bars and the chips share its centre.
    let chip = ui.ctx().fonts_mut(|fonts| fonts.row_height(&pill_font())) + PILL_PAD.y * 2.0;
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = chip;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(CHIP_GAP, CHIP_GAP);
            if let Some(colors) = colors
                && super::colors::show_bars(ui, colors)
            {
                ui.add_space(BARS_GAP);
            }
            for pill in pills {
                show_pill(ui, pill, events);
            }
            // The input replaces the button, as in the mockup.
            if !matches!(add, AddState::Input { .. }) {
                button = Some(add::show_button(ui, add));
            }
        });
    });
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
        add::show_menu(
            ui,
            ui.id().with("add-menu"),
            button.rect,
            &choices,
            add,
            events,
        );
    }
    add::show_input(ui, ui.id().with("add-input"), add, events);
}

/// The pill text face: the body size in the medium face.
fn pill_font() -> FontId {
    FontId::new(
        tokens::SIZE_BODY,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into()),
    )
}

fn show_pill(ui: &mut Ui, pill: &TagPill, events: &mut Vec<PreviewEvent>) {
    let opacity = pill.state.opacity();
    let (fill, text) = tag_colors(&pill.name);
    let font = pill_font();
    let fade = |color: Color32| color.gamma_multiply(opacity);
    let name = ui
        .painter()
        .layout_no_wrap(pill.name.clone(), font.clone(), fade(text));
    let cross_width = ui
        .painter()
        .layout_no_wrap("x".to_owned(), font.clone(), Color32::WHITE)
        .size()
        .x;
    // The cross has its room reserved, so the pill does not jump on hover.
    let pad = PILL_PAD;
    let width = pad.x * 2.0 + name.size().x + tokens::SPACING_XS + cross_width;
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, name.size().y + pad.y * 2.0), Sense::click());
    let cross = Rect::from_min_max(pos2(rect.max.x - pad.x - cross_width, rect.min.y), rect.max);
    let hover = response.hover_pos();
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(PILL_RADIUS), fade(fill));
    painter.galley(rect.min + pad, name, text);
    if let Some(at) = hover {
        let pointer_on_cross = cross.contains(at);
        // Only a click that does something gets the hand.
        if pointer_on_cross || pill.state != ColorState::All {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        // The cross is the tag text, dimmed until the pointer is on it.
        let color = if pointer_on_cross {
            text
        } else {
            text.gamma_multiply(0.6)
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
