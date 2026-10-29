//! The row of colour squares: which colours the selected images carry.

use egui::{RichText, Ui};

use super::PreviewEvent;
use crate::swatch::{self, Swatch};
use crate::tokens;

/// How many of the selected images carry one colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorState {
    /// No selected image has the colour.
    #[default]
    None,
    /// Some of them have it.
    Some,
    /// All of them have it. A single image only ever gives `All` or `None`.
    All,
}

impl ColorState {
    /// The opacity of the square for this state.
    #[must_use]
    pub const fn opacity(self) -> f32 {
        match self {
            Self::All => 1.0,
            Self::Some => 0.5,
            Self::None => 0.3,
        }
    }
}

/// One state for each square, in `Swatch::ALL` order.
pub type ColorStates = [ColorState; 12];

/// Draw the Tags heading and the squares. A click queues an event.
pub fn show_row(ui: &mut Ui, states: &ColorStates, events: &mut Vec<PreviewEvent>) {
    ui.add_space(tokens::SPACING_S);
    ui.label(
        RichText::new("Tags")
            .size(tokens::SIZE_CAPTION)
            .color(tokens::TEXT_MUTED),
    );
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tokens::SPACING_XS;
        for (swatch, state) in Swatch::ALL.into_iter().zip(states) {
            if swatch::show(ui, swatch, swatch::SIZE, state.opacity()).clicked() {
                events.push(PreviewEvent::ColorClicked(swatch));
            }
        }
    });
}

#[cfg(test)]
mod tests;
