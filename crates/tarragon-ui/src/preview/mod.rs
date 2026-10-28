//! The Preview panel body: the selected image or images, and their details.

use egui::{Align, Layout, RichText, Ui};

use crate::tokens;

/// The hint shown while nothing is selected.
pub const EMPTY_HINT: &str = "Select an image to preview it";

/// The state of the Preview panel.
#[derive(Debug, Default)]
pub struct Preview {}

impl Preview {
    /// Draw the panel body.
    pub fn show(&mut self, ui: &mut Ui) {
        // The hint sits in the middle, muted, like the other panels' empty text.
        ui.with_layout(
            Layout::top_down(Align::Center).with_main_align(Align::Center),
            |ui| {
                ui.label(
                    RichText::new(EMPTY_HINT)
                        .size(tokens::SIZE_SMALL)
                        .color(tokens::TEXT_MUTED),
                );
            },
        );
    }
}
