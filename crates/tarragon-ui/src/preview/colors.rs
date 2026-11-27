//! The colour bars: which colours the selected images carry.

use egui::{Sense, Ui, Vec2, vec2};

use crate::swatch::{self, Swatch};

/// How many of the selected images carry one color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorState {
    /// No selected image has the color.
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

/// One state for each colour, in `Swatch::ALL` order.
pub type ColorStates = [ColorState; 12];

/// The size of one colour bar, in points.
pub const BAR: Vec2 = vec2(18.0, 8.0);
/// The gap between two bars, in points.
const BAR_GAP: f32 = 3.0;

/// Draw a bar for each colour the selection carries, in swatch order: full
/// when every selected image has it, dimmed when only some do. The bars are
/// one size, because no colour share is stored. Returns whether any bar was
/// drawn.
pub fn show_bars(ui: &mut Ui, states: &ColorStates) -> bool {
    let carried: Vec<(Swatch, ColorState)> = Swatch::ALL
        .into_iter()
        .zip(states.iter().copied())
        .filter(|(_, state)| *state != ColorState::None)
        .collect();
    if carried.is_empty() {
        return false;
    }
    // The bars sit in the caller's row, so they share its centre line; only
    // the gap between them changes, and the caller's gap comes back after.
    // The gap after an item is the one set when it was placed, so the last
    // bar takes the caller's gap again.
    let gap = ui.spacing().item_spacing.x;
    let last = carried.len() - 1;
    for (index, (swatch, state)) in carried.into_iter().enumerate() {
        ui.spacing_mut().item_spacing.x = if index == last { gap } else { BAR_GAP };
        let (rect, _) = ui.allocate_exact_size(BAR, Sense::hover());
        swatch::paint(ui.painter(), rect, swatch, state.opacity());
    }
    true
}

#[cfg(test)]
mod tests;
