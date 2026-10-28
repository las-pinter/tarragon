//! The Preview panel body: the selected image or images, and their details.

use egui::{Align, Layout, RichText, ScrollArea, Ui};

use crate::tokens;

mod image;
mod meta;
pub use image::PreviewImage;
pub use meta::Meta;

/// The hint shown while nothing is selected.
pub const EMPTY_HINT: &str = "Select an image to preview it";

/// The state of the Preview panel.
#[derive(Debug, Default)]
pub struct Preview {
    /// The details of the one previewed file, when there is one.
    meta: Option<Meta>,
    /// The picture of that file, when it is ready.
    image: Option<PreviewImage>,
}

impl Preview {
    /// Set the file details to show, or `None` for the empty state.
    pub fn set_meta(&mut self, meta: Option<Meta>) {
        self.meta = meta;
    }

    /// Set the picture to show, or `None` for no picture.
    pub fn set_image(&mut self, image: Option<PreviewImage>) {
        self.image = image;
    }

    /// Draw the panel body. It scrolls, so a short pane loses nothing.
    pub fn show(&mut self, ui: &mut Ui) {
        match (&self.image, &self.meta) {
            (None, None) => show_empty(ui),
            (image, meta) => {
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(image) = image {
                            image::show_box(ui, image);
                        }
                        if let Some(meta) = meta {
                            meta::show_grid(ui, meta);
                        }
                    });
            }
        }
    }
}

/// The hint sits in the middle, muted, like the other panels' empty text.
fn show_empty(ui: &mut Ui) {
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
