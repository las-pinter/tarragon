//! The color swatches of the filter bar.

use egui::Ui;
use tarragon_core::filter_bar::{ColorChoice, FilterBar};

use crate::swatch::{self, Swatch};

/// The gap between two swatches, in points: room for both rings.
pub(super) const SWATCH_GAP: f32 = 8.0;

/// The filter choice a swatch stands for.
#[must_use]
pub fn choice_of(swatch: Swatch) -> ColorChoice {
    match swatch {
        Swatch::Bucket(bucket) => ColorChoice::Bucket(bucket),
        Swatch::Transparent => ColorChoice::Transparent,
    }
}

/// Whether the bar holds `choice`.
fn held(bar: &FilterBar, choice: ColorChoice) -> bool {
    match choice {
        ColorChoice::Bucket(bucket) => bar.state.colors.contains(&bucket),
        ColorChoice::Transparent => bar.state.transparent,
    }
}

/// The swatches the bar holds, in swatch order.
#[must_use]
pub fn held_swatches(bar: &FilterBar) -> Vec<Swatch> {
    Swatch::ALL
        .into_iter()
        .filter(|s| held(bar, choice_of(*s)))
        .collect()
}

/// Draw the 12 round swatches into the caller's wrapping row and return the
/// clicked choice. Held swatches get the light ring.
///
/// Swatches sit `SWATCH_GAP` apart; the gap after the last one is `gap_after`.
pub fn show_colors(ui: &mut Ui, bar: &FilterBar, gap_after: f32) -> Option<ColorChoice> {
    let mut clicked = None;
    let last = Swatch::ALL.len() - 1;
    for (i, item) in Swatch::ALL.into_iter().enumerate() {
        // egui spaces an item from the next one by the spacing set when the
        // item is placed.
        ui.spacing_mut().item_spacing.x = if i == last { gap_after } else { SWATCH_GAP };
        let choice = choice_of(item);
        if swatch::show_filter(ui, item, held(bar, choice)).clicked() {
            clicked = Some(choice);
        }
    }
    clicked
}

#[cfg(test)]
mod tests;
