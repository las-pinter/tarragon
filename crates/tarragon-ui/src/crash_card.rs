//! The card that tells the user the last run ended with a crash.
//!
//! It is a modal window inside the main window, because no panel is open at
//! the start that shows it. A dim layer covers the dock and takes its input.

use egui::{
    Align2, CornerRadius, FontFamily, FontId, Frame, Key, Label, Margin, Modifiers, RichText,
    Window, vec2,
};

use crate::theme::points;
use crate::widgets::{ghost_button, small_button};
use crate::{backdrop, fonts, tokens};

/// Window width, in points.
const WIDTH: f32 = 440.0;

/// One crash report the user has not seen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashReport {
    /// The report file name, without the folder.
    pub file_name: String,
    /// The full path of the report file.
    pub path: String,
    /// The first lines of the file: version, system, message, and place.
    pub summary: String,
}

/// A choice the user made on the card. The caller acts on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashCardEvent {
    /// Close the card; the report counts as seen.
    Dismiss,
    /// Open the report file. The card stays open.
    OpenReport,
}

/// The state of the crash card.
#[derive(Debug, Default)]
pub struct CrashCard {
    report: Option<CrashReport>,
    events: Vec<CrashCardEvent>,
}

impl CrashCard {
    /// Show the card for `report` from the next frame.
    pub fn show_report(&mut self, report: CrashReport) {
        self.report = Some(report);
    }

    /// Whether the card is shown.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.report.is_some()
    }

    /// Take the choices the user made since the last call.
    pub fn take_events(&mut self) -> Vec<CrashCardEvent> {
        std::mem::take(&mut self.events)
    }

    /// Draw the card when a report is set.
    pub fn show(&mut self, ctx: &egui::Context) {
        let Some(report) = &self.report else {
            return;
        };
        backdrop::show(ctx, "crash_card_backdrop");
        let before = self.events.len();
        let events = &mut self.events;
        let title = RichText::new("Tarragon closed unexpectedly")
            .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
            .color(tokens::CORAL_STRONG);
        Window::new(title)
            // Above the dim layer, whatever the click order.
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                ui.spacing_mut().item_spacing.y = tokens::SPACING_S;
                ui.label("The last run ended with an error. The report was saved to:");
                let path = RichText::new(&report.path)
                    .size(tokens::SIZE_CAPTION)
                    .color(tokens::TEXT_TERTIARY);
                ui.add(Label::new(path).selectable(true).wrap());
                Frame::new()
                    .fill(tokens::BG_SECONDARY)
                    .corner_radius(CornerRadius::same(tokens::RADIUS_M))
                    .inner_margin(Margin::same(points(tokens::SPACING_S)))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let text = RichText::new(&report.summary)
                            .font(FontId::monospace(tokens::SIZE_LOG))
                            .color(tokens::TEXT_TERTIARY);
                        ui.add(Label::new(text).selectable(true).wrap());
                    });
                ui.horizontal(|ui| {
                    if small_button(ui, "Open report").clicked() {
                        events.push(CrashCardEvent::OpenReport);
                    }
                    if ghost_button(ui, "Dismiss").clicked() {
                        events.push(CrashCardEvent::Dismiss);
                    }
                });
            });
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        if escape {
            self.events.push(CrashCardEvent::Dismiss);
        }
        // Only this frame's events count: an old Dismiss may still be queued.
        if self.events[before..].contains(&CrashCardEvent::Dismiss) {
            self.report = None;
        }
    }
}

#[cfg(test)]
mod tests;
