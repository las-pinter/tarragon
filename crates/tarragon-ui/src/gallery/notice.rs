//! The notice line at the foot of the gallery.

use std::time::{Duration, Instant};

use egui::{Frame, Label, Margin, Panel, RichText, Ui};

use super::{Gallery, WAKE_SLACK};
use crate::theme::points;
use crate::tokens;

/// How long the notice line shows, counted from the first frame that paints it.
pub(super) const NOTICE_TIME: Duration = Duration::from_secs(6);

/// One message and the time it started.
#[derive(Clone, Debug)]
struct Message {
    /// The text that shows.
    text: String,
    /// The time of the first frame that painted the message, or none before it.
    since: Option<Instant>,
}

/// The message that shows now, if one does.
#[derive(Clone, Debug, Default)]
pub(super) struct Notice {
    /// The message, or none while no message shows.
    current: Option<Message>,
}

impl Notice {
    /// Replace the message. The timer starts at the next frame.
    fn set(&mut self, text: String) {
        self.current = Some(Message { text, since: None });
    }

    /// Start the timer at `now` if it has not started, and drop an expired
    /// message. Returns the text and the time left, or `None` when no message
    /// shows.
    fn update(&mut self, now: Instant) -> Option<(&str, Duration)> {
        let message = self.current.as_mut()?;
        let started = *message.since.get_or_insert(now);
        let elapsed = now.saturating_duration_since(started);
        let Some(left) = NOTICE_TIME.checked_sub(elapsed).filter(|d| !d.is_zero()) else {
            self.current = None;
            return None;
        };
        self.current
            .as_ref()
            .map(|message| (message.text.as_str(), left))
    }
}

impl Gallery {
    /// Show one line of text at the foot of the gallery for six seconds.
    ///
    /// A new call replaces the text and restarts the time.
    pub fn notify(&mut self, text: String) {
        self.notice.set(text);
    }

    /// Draw the notice line at the foot of `ui` while it shows, and ask for a
    /// repaint at its expiry. Draw it before the grid, so the grid takes the
    /// room that is left.
    pub(super) fn show_notice(&mut self, ui: &mut Ui, now: Instant) {
        let Some((text, left)) = self.notice.update(now) else {
            return;
        };
        let margin = Margin::symmetric(points(tokens::SPACING_S), points(tokens::SPACING_XS));
        Panel::bottom("tarragon-gallery-notice")
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(tokens::BG_SECONDARY).inner_margin(margin))
            .show(ui, |ui| {
                let text = RichText::new(text)
                    .size(tokens::SIZE_CAPTION)
                    .color(tokens::TEXT_TERTIARY);
                ui.add(Label::new(text).truncate());
            });
        ui.ctx().request_repaint_after(left + WAKE_SLACK);
    }
}
