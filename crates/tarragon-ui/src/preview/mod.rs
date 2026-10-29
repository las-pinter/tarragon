//! The Preview panel body: the selected image or images, and their details.

use egui::{Align, Layout, RichText, ScrollArea, Ui};

use crate::swatch::Swatch;
use crate::tokens;

mod colors;
mod image;
mod meta;
mod tags;
pub use colors::{ColorState, ColorStates};
pub use image::PreviewImage;
pub use meta::Meta;
pub use tags::TagPill;

/// The hint shown while nothing is selected.
pub const EMPTY_HINT: &str = "Select an image to preview it";

/// What the panel asks the app to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewEvent {
    /// The user clicked this colour square.
    ColorClicked(Swatch),
    /// The user clicked the x on this tag: remove it from every selected image.
    TagRemoved(String),
    /// The user clicked a partial tag: add it to every selected image.
    TagApplied(String),
}

/// The state of the Preview panel.
#[derive(Debug)]
pub struct Preview {
    /// The details of the one previewed file, when there is one.
    meta: Option<Meta>,
    /// The picture of that file, when it is ready.
    image: Option<PreviewImage>,
    /// How many selected images carry each colour.
    colors: ColorStates,
    /// The user tags of the selected images, in display order.
    tags: Vec<TagPill>,
    /// Whether the colour squares show; settings can turn colour tags off.
    colors_enabled: bool,
    /// Events not yet taken by the app.
    events: Vec<PreviewEvent>,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            meta: None,
            image: None,
            colors: ColorStates::default(),
            tags: Vec::new(),
            colors_enabled: true,
            events: Vec::new(),
        }
    }
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

    /// Set how many selected images carry each colour, in `Swatch::ALL` order.
    pub fn set_colors(&mut self, colors: ColorStates) {
        self.colors = colors;
    }

    /// Set the tag pills to show, with how many selected images carry each.
    pub fn set_tags(&mut self, tags: Vec<TagPill>) {
        self.tags = tags;
    }

    /// Show or hide the colour squares, to follow the colour tags setting.
    pub fn set_colors_enabled(&mut self, enabled: bool) {
        self.colors_enabled = enabled;
    }

    /// Return the queued events and empty the queue.
    pub fn take_events(&mut self) -> Vec<PreviewEvent> {
        std::mem::take(&mut self.events)
    }

    /// Draw the panel body. It scrolls, so a short pane loses nothing.
    pub fn show(&mut self, ui: &mut Ui) {
        let (colors, tags, enabled) = (&self.colors, &self.tags, self.colors_enabled);
        let events = &mut self.events;
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
                        tags::show_heading(ui);
                        if enabled {
                            colors::show_row(ui, colors, events);
                        }
                        tags::show_pills(ui, tags, events);
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
