//! The row under the mosaic: the selected count over the total size, then
//! "Edit tags" and the open-all button.
//!
//! The sizes follow the Preview of design/redesign/main-multi-select.html.

use egui::{Align, Color32, FontFamily, FontId, Label, Layout, RichText, Ui, vec2};

use super::PreviewEvent;
use super::info::ROW_HEIGHT;
use super::meta::size_text;
use crate::{fonts, tokens, widgets};

/// The gap between the text and the buttons, in points.
const GAP: f32 = 12.0;
/// The gap between the two buttons, in points.
const BUTTON_GAP: f32 = 8.0;
/// The text of the button that shows or hides the tag row.
pub const EDIT_TAGS_TEXT: &str = "Edit tags";
/// The open-all text when the selection does not open in one named app.
pub const OPEN_ALL_TEXT: &str = "Open all";
/// The most of the row the open-all text may take before it drops the app
/// name, so the count keeps its room.
const OPEN_SHARE: f32 = 0.4;

/// What the app knows about the whole selection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MultiSummary {
    /// The size of every selected file together, in bytes.
    pub bytes: u64,
    /// The app every selected file opens in, when it is one named app.
    pub app: Option<String>,
}

impl MultiSummary {
    /// The open-all button text: "Open all in Krita", or "Open all".
    #[must_use]
    pub fn open_label(&self) -> String {
        self.app.as_ref().map_or_else(
            || OPEN_ALL_TEXT.to_owned(),
            |app| format!("{OPEN_ALL_TEXT} in {app}"),
        )
    }
}

/// The count heading: "2 selected".
#[must_use]
pub fn count_text(total: usize) -> String {
    format!("{total} selected")
}

/// The line under the count: the total size, then the hint for the images the
/// mosaic does not draw.
#[must_use]
pub fn detail_text(summary: &MultiSummary, hint: Option<&str>) -> String {
    let size = format!("{} in all", size_text(summary.bytes));
    match hint {
        Some(hint) => format!("{size} · {hint}"),
        None => size,
    }
}

/// The open-all button for a row `width` wide: the named label while the
/// button, padding included, takes at most `OPEN_SHARE` of the row, else the
/// plain "Open all".
fn fitting_label(ui: &Ui, summary: &MultiSummary, width: f32) -> String {
    let label = summary.open_label();
    let font = FontId::new(
        tokens::SIZE_BODY,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into()),
    );
    let text = ui
        .painter()
        .layout_no_wrap(label.clone(), font, Color32::WHITE)
        .size()
        .x;
    // The flat button pads its text by the large spacing on each side.
    if text + 2.0 * tokens::SPACING_L <= width * OPEN_SHARE {
        label
    } else {
        OPEN_ALL_TEXT.to_owned()
    }
}

/// Draw the row. `tags_open` flips on an "Edit tags" click.
pub fn show(
    ui: &mut Ui,
    total: usize,
    summary: &MultiSummary,
    hint: Option<&str>,
    tags_open: &mut bool,
    events: &mut Vec<PreviewEvent>,
) {
    let size = vec2(ui.available_width(), ROW_HEIGHT);
    let label = fitting_label(ui, summary, size.x);
    ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        if widgets::primary_button_with_height(ui, &label, ROW_HEIGHT).clicked() {
            events.push(PreviewEvent::OpenAll);
        }
        if widgets::secondary_toggle_button(ui, EDIT_TAGS_TEXT, *tags_open, ROW_HEIGHT).clicked() {
            *tags_open = !*tags_open;
        }
        ui.add_space(GAP - BUTTON_GAP);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let count = RichText::new(count_text(total))
                .size(tokens::SIZE_TITLE)
                .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
                .color(tokens::TEXT_PRIMARY);
            ui.add(Label::new(count).truncate());
            let line = RichText::new(detail_text(summary, hint))
                .size(tokens::SIZE_BODY)
                .color(tokens::TEXT_SECONDARY);
            ui.add(Label::new(line).truncate());
        });
    });
}

#[cfg(test)]
mod tests;
