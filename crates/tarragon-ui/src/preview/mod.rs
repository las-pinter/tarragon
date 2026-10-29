//! The Preview panel body: the selected image or images, and their details.

use egui::{RichText, ScrollArea, Ui};

use crate::swatch::Swatch;
use crate::tokens;

mod add;
pub mod budget;
mod colors;
#[cfg(test)]
mod fill_tests;
mod image;
mod meta;
mod mosaic;
#[cfg(test)]
mod pane_tests;
mod tags;
#[cfg(test)]
mod test_support;
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
    /// The pictures of a multi-selection that are ready, in display order.
    mosaic: Vec<PreviewImage>,
    /// How many images are selected, which can exceed `mosaic`.
    mosaic_total: usize,
    /// How many mosaic pictures show at most.
    mosaic_cap: usize,
    /// How many selected images carry each colour.
    colors: ColorStates,
    /// The user tags of the selected images, in display order.
    tags: Vec<TagPill>,
    /// Every known user tag name, for the "+ add" menu. Colour tags are left out.
    known_tags: Vec<String>,
    /// Which part of the "+ add" control is open.
    add: add::AddState,
    /// Whether the colour squares show; settings can turn colour tags off.
    colors_enabled: bool,
    /// Events not yet taken by the app.
    events: Vec<PreviewEvent>,
    /// The height of everything below the picture box in the last frame. The
    /// box can only fill the rest of the pane once that height is known.
    below_box: f32,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            meta: None,
            image: None,
            mosaic: Vec::new(),
            mosaic_total: 0,
            mosaic_cap: mosaic::DEFAULT_CAP,
            colors: ColorStates::default(),
            tags: Vec::new(),
            known_tags: Vec::new(),
            add: add::AddState::default(),
            colors_enabled: true,
            events: Vec::new(),
            below_box: 0.0,
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
        if image.is_none() {
            // The next box is sized from a fresh measure.
            self.below_box = 0.0;
        }
        self.image = image;
    }

    /// Set the ready pictures of a multi-selection and how many images are
    /// selected in all. With two or more selected and a picture ready, the
    /// mosaic replaces the single picture; the selected images it does not
    /// draw become a "+k more" hint.
    pub fn set_mosaic(&mut self, images: Vec<PreviewImage>, total: usize) {
        self.mosaic = images;
        self.mosaic_total = total;
    }

    /// Set how many pictures the mosaic shows at most, to follow the
    /// `max_multi_preview` setting. Zero counts as one.
    pub fn set_max_multi_preview(&mut self, cap: usize) {
        self.mosaic_cap = cap.max(1);
    }

    /// Set how many selected images carry each colour, in `Swatch::ALL` order.
    pub fn set_colors(&mut self, colors: ColorStates) {
        self.colors = colors;
    }

    /// Set the tag pills to show, with how many selected images carry each.
    pub fn set_tags(&mut self, tags: Vec<TagPill>) {
        self.tags = tags;
    }

    /// Set the user tag names the "+ add" menu offers. The menu hides the
    /// ones every selected image already has.
    pub fn set_known_tags(&mut self, names: Vec<String>) {
        self.known_tags = names;
    }

    /// Show or hide the colour squares, to follow the colour tags setting.
    pub fn set_colors_enabled(&mut self, enabled: bool) {
        self.colors_enabled = enabled;
    }

    /// Close the "+ add" menu and input. The app calls it when the selection
    /// changes; `set_tags` must not, because tags refresh after every tag event.
    pub fn selection_changed(&mut self) {
        self.add = add::AddState::default();
    }

    /// Return the queued events and empty the queue.
    pub fn take_events(&mut self) -> Vec<PreviewEvent> {
        std::mem::take(&mut self.events)
    }

    /// Draw the panel body. It scrolls, so a short pane loses nothing.
    pub fn show(&mut self, ui: &mut Ui) {
        let (colors, tags, enabled) = (&self.colors, &self.tags, self.colors_enabled);
        let (known, add) = (&self.known_tags, &mut self.add);
        let (events, below_box) = (&mut self.events, &mut self.below_box);
        let (mosaic, total, cap) = (&self.mosaic, self.mosaic_total, self.mosaic_cap);
        // A big selection with one picture ready already shows a one-cell
        // mosaic, so the panel does not flip to the single box while it fills.
        let multi = total >= 2 && !mosaic.is_empty();
        match (&self.image, &self.meta) {
            (None, None) if !multi => {
                // A hidden input must not come back stale with the next selection.
                *add = add::AddState::default();
                show_empty(ui);
            }
            (image, meta) => {
                // The scope keeps the solid bar from leaking to the caller's ui.
                ui.scope(|ui| {
                    // Floating bars show only on hover, so cut-off content on a
                    // short pane would give no sign. A solid bar is there while
                    // the content overflows and takes no room otherwise.
                    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show_viewport(ui, |ui, viewport| {
                            let mut box_bottom = None;
                            if multi {
                                mosaic::show(ui, mosaic, total, cap);
                            } else if let Some(image) = image {
                                let height = image::box_height(viewport.height(), *below_box);
                                box_bottom = Some(image::show_box(ui, image, height).bottom());
                            }
                            if let Some(meta) = meta {
                                meta::show_grid(ui, meta);
                            }
                            tags::show_heading(ui);
                            if enabled {
                                colors::show_row(ui, colors, events);
                            }
                            tags::show_pills(ui, tags, known, add, events);
                            if let Some(bottom) = box_bottom {
                                // The content below is only known after layout, so
                                // the next frame sizes the box with it.
                                let below = ui.min_rect().bottom() - bottom;
                                if (below - *below_box).abs() > 0.5 {
                                    *below_box = below;
                                    ui.ctx().request_repaint();
                                }
                            }
                        });
                });
            }
        }
    }
}

/// The hint sits in the middle, muted, like the other panels' empty text.
fn show_empty(ui: &mut Ui) {
    ui.centered_and_justified(|ui| {
        ui.label(
            RichText::new(EMPTY_HINT)
                .size(tokens::SIZE_SMALL)
                .color(tokens::TEXT_MUTED),
        );
    });
}
