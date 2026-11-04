//! Hooks that a key shortcut calls: select all and focus the search field.

use egui::text::{CCursor, CCursorRange};
use egui::{TextEdit, Ui, text_edit::TextEditOutput};

use super::Gallery;

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
