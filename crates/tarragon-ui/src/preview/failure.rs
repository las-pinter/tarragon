//! The card that replaces the picture when a file fails to render.

use egui::{CornerRadius, FontFamily, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};

use super::PreviewEvent;
use crate::grid::{ERROR_MARK_SIZE, paint_error};
use crate::theme::points;
use crate::widgets::{ghost_button, small_button};
use crate::{fonts, tokens};

/// The hint under the message. Retry is the only recovery the app offers.
pub(super) const HINT: &str = "Retry renders the file again. Check the log for the full trace.";

/// Why one file has no preview.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewFailure {
    /// The file name, without the folder.
    pub name: String,
    /// What went wrong, in one sentence.
    pub message: String,
}

/// Draw the card. A click on a button queues an event.
pub(super) fn show_card(ui: &mut Ui, failure: &PreviewFailure, events: &mut Vec<PreviewEvent>) {
    Frame::new()
        .fill(tokens::BG_SECONDARY)
        .stroke(Stroke::new(tokens::STROKE_WIDTH, tokens::CORAL_DARK))
        .corner_radius(CornerRadius::same(tokens::RADIUS_L))
        .inner_margin(Margin::same(points(tokens::SPACING_L)))
        .show(ui, |ui| {
            // A frame shrinks to its content; the card spans the pane.
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = tokens::SPACING_S;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tokens::SPACING_M;
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ERROR_MARK_SIZE, ERROR_MARK_SIZE), Sense::hover());
                paint_error(ui.painter(), rect, CornerRadius::ZERO);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        RichText::new("Preview failed")
                            .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
                            .color(tokens::CORAL_STRONG),
                    );
                    ui.label(detail(&failure.name));
                });
            });
            ui.label(detail(&failure.message));
            ui.label(
                RichText::new(HINT)
                    .size(tokens::SIZE_CAPTION)
                    .color(tokens::TEXT_MUTED),
            );
            ui.horizontal(|ui| {
                if small_button(ui, "Retry").clicked() {
                    events.push(PreviewEvent::RetryRender);
                }
                if ghost_button(ui, "Open in editor").clicked() {
                    events.push(PreviewEvent::OpenInEditor);
                }
            });
        });
}

/// Caption-size text in the colour of the Metadata values.
fn detail(text: &str) -> RichText {
    RichText::new(text)
        .size(tokens::SIZE_CAPTION)
        .color(tokens::TEXT_TERTIARY)
}
