//! The colour swatch row of the filter bar.

use egui::Ui;
use tarragon_core::filter_bar::{ColorChoice, FilterBar};

use crate::swatch::{self, Swatch};
use crate::tokens;

/// The filter choice a swatch stands for.
fn choice_of(swatch: Swatch) -> ColorChoice {
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

/// Draw the 12 filter swatches in a wrapping row and return the clicked choice.
///
/// Swatches the bar holds get the active outline. Nothing is drawn, and the
/// result is `None`, when `enabled` is false.
pub fn show_colors(ui: &mut Ui, bar: &FilterBar, enabled: bool) -> Option<ColorChoice> {
    if !enabled {
        return None;
    }
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::splat(tokens::SPACING_XS);
        for item in Swatch::ALL {
            let choice = choice_of(item);
            if swatch::show_filter(ui, item, held(bar, choice)).clicked() {
                clicked = Some(choice);
            }
        }
    });
    clicked
}

#[cfg(test)]
mod tests;
