//! The Log panel content: a toolbar and the log lines.
//!
//! The caller owns the lines and the debug flag. The panel reports the
//! clicks and never keeps a copy of the lines.

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Label, Layout, Margin, RichText, Sense, Stroke,
    Ui, UiBuilder,
};

use crate::theme::points;
use crate::tokens;

/// Line height as a multiple of the log font size, as in `theme.css`.
const LINE_HEIGHT_FACTOR: f32 = 1.55;
/// Border width of the lines box, in points. `theme.css` sets 1 px.
const BORDER_WIDTH: f32 = 1.0;

/// The severity of one log line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    /// Detail for developers. The app maps trace here.
    Debug,
    /// Normal progress.
    Info,
    /// Something unexpected that the app handled.
    Warn,
    /// A failure.
    Error,
}

impl LogLevel {
    /// The text colour for this level.
    #[must_use]
    pub fn color(self) -> Color32 {
        match self {
            Self::Debug => tokens::TEXT_TERTIARY,
            Self::Info => tokens::TEXT_PRIMARY,
            Self::Warn => tokens::AMBER_ACCENT,
            Self::Error => tokens::CORAL_STRONG,
        }
    }
}

/// One preformatted log line.
#[derive(Clone, Copy, Debug)]
pub struct LogLine<'a> {
    /// The severity, which picks the colour.
    pub level: LogLevel,
    /// The full line text, for example `[21:04:02] INFO     scan: done`.
    pub text: &'a str,
}

/// Row access for the log lines, so the panel reads only the visible rows.
///
/// The panel reads the source during the frame. An implementation that holds a
/// lock must not log while it holds it, because the logger may push into the
/// same store and deadlock.
pub trait LogSource {
    /// The number of lines.
    fn len(&self) -> usize;
    /// The line at `index`, oldest first. `index` is below `len()`.
    fn line(&self, index: usize) -> LogLine<'_>;
    /// Whether there are no lines.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A [`LogSource`] over a slice of lines.
#[derive(Clone, Copy, Debug)]
pub struct LogSlice<'a>(pub &'a [LogLine<'a>]);

impl LogSource for LogSlice<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn line(&self, index: usize) -> LogLine<'_> {
        self.0[index]
    }
}

/// The per-frame input of the Log panel.
#[derive(Clone, Copy)]
pub struct LogView<'a> {
    /// The lines to show, oldest first.
    pub source: &'a dyn LogSource,
    /// The current state of the Debug checkbox. The panel does not filter
    /// lines: the flag only sets the log level in the app, and lines that are
    /// already logged stay visible.
    pub debug: bool,
}

/// What the user did in the Log panel during one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogPanelEvents {
    /// The Clear button was clicked.
    pub clear: bool,
    /// The new Debug value, when the user toggled the checkbox.
    pub debug: Option<bool>,
}

/// Draw the toolbar and the lines, and return the user's actions.
pub(super) fn show(ui: &mut Ui, view: &LogView<'_>) -> LogPanelEvents {
    let mut events = LogPanelEvents::default();

    ui.horizontal(|ui| {
        if ui.small_button("Clear").clicked() {
            events.clear = true;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut debug = view.debug;
            if ui.checkbox(&mut debug, "Debug").changed() {
                events.debug = Some(debug);
            }
        });
    });

    Frame::new()
        .fill(tokens::BG_LOG_PANEL)
        .stroke(Stroke::new(BORDER_WIDTH, tokens::SURFACE_HIGHLIGHT))
        .corner_radius(CornerRadius::same(tokens::RADIUS_M))
        .inner_margin(Margin::same(points(tokens::SPACING_XS)))
        .show(ui, |ui| lines(ui, view.source));

    events
}

/// Draw the visible rows only, and keep the view at the newest line.
fn lines(ui: &mut Ui, source: &dyn LogSource) {
    let row_height = tokens::SIZE_LOG * LINE_HEIGHT_FACTOR;
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show_rows(ui, row_height, source.len(), |ui, range| {
            for index in range {
                let line = source.line(index);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row_height),
                    Sense::hover(),
                );
                let text = RichText::new(line.text)
                    .font(FontId::monospace(tokens::SIZE_LOG))
                    .color(line.level.color());
                // Long lines end with an ellipsis instead of wrapping.
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.add(Label::new(text).truncate().selectable(true));
                    },
                );
            }
        });
}

#[cfg(test)]
mod tests;
