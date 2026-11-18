//! Hooks that a key shortcut calls: select all and focus the search field.
//! The search quiet-window timer lives here too.

use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange};
use egui::{TextEdit, Ui, text_edit::TextEditOutput};

use super::{Gallery, SEARCH_DEBOUNCE};

/// A timer that applies a change after a quiet window.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Debouncer {
    /// The instant when the quiet window ends, or none.
    deadline: Option<Instant>,
}

impl Debouncer {
    /// Start the quiet window at `now`.
    pub(super) fn arm(&mut self, now: Instant) {
        self.deadline = Some(now + SEARCH_DEBOUNCE);
    }

    /// The time left before the quiet window ends, or none when no window runs.
    pub(super) fn remaining(&self, now: Instant) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    /// Return whether the quiet window has passed, and clear the timer.
    pub(super) fn is_ready(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.deadline else {
            return false;
        };
        if now >= deadline {
            self.deadline = None;
            return true;
        }
        false
    }
}

impl Gallery {
    /// Select every visible item and let the revision follow.
    pub fn select_all_visible(&mut self) {
        self.selection.select_all(&self.visible);
        self.sync_selection_revision();
    }

    /// Ask the search field to take focus and select its text on the next draw.
    pub fn focus_search(&mut self) {
        self.search_focus_requested = true;
    }

    /// Act on a focus request once: focus the field and select its text.
    ///
    /// The request clears here, so later frames leave the focus alone.
    pub(super) fn apply_search_focus(&mut self, ui: &Ui, output: TextEditOutput) {
        if !std::mem::take(&mut self.search_focus_requested) {
            return;
        }
        output.response.request_focus();
        let end = CCursor::new(self.search.chars().count());
        let mut state = output.state;
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(0), end)));
        TextEdit::store_state(ui.ctx(), output.response.id, state);
    }
}
