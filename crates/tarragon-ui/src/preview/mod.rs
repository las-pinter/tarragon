//! The Preview panel body: the selected image or images, and their details.

use egui::{Align, Layout, RichText, Ui};

use crate::tokens;

mod meta;
pub use meta::Meta;

/// The hint shown while nothing is selected.
pub const EMPTY_HINT: &str = "Select an image to preview it";

/// The state of the Preview panel.
#[derive(Debug, Default)]
pub struct Preview {
    /// The details of the one previewed file, when there is one.
    meta: Option<Meta>,
}

impl Preview {
    /// Set the file details to show, or `None` for the empty state.
    pub fn set_meta(&mut self, meta: Option<Meta>) {
        self.meta = meta;
    }

    /// Draw the panel body.
    pub fn show(&mut self, ui: &mut Ui) {
        if let Some(meta) = &self.meta {
            meta::show_grid(ui, meta);
            return;
        }
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
