//! Reusable egui widgets: grid, sidebar, chips, pills, and dialogs.

use egui::{Button, Response, RichText, Ui, vec2};

use crate::tokens;

/// A small button: `SIZE_SMALL` text with 8 by 3 point padding, as `.btn.small`.
pub(crate) fn small_button(ui: &mut Ui, text: &str) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(8.0, 3.0);
        ui.add(Button::new(RichText::new(text).size(tokens::SIZE_SMALL)))
    })
    .inner
}
