//! The "Manage tags" window: a list of the user tags with their image counts.
//!
//! The window floats inside the main window. It is not an OS window. The caller
//! owns the data and sets the rows.

use egui::{
    Align, Align2, FontId, Key, Label, Layout, RichText, ScrollArea, Sense, UiBuilder, vec2,
};

use crate::tokens;

/// Window width, in points.
const WIDTH: f32 = 360.0;
/// The most rows the list shows before it scrolls.
const MAX_VISIBLE_ROWS: f32 = 8.0;

/// One user tag in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagRow {
    /// The tag id.
    pub id: i64,
    /// The tag name.
    pub name: String,
    /// The number of images with the tag.
    pub count: u32,
}

/// The state of the "Manage tags" window.
#[derive(Debug, Default)]
pub struct TagManager {
    open: bool,
    rows: Vec<TagRow>,
}

impl TagManager {
    /// Show the window from the next frame.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Hide the window.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether the window is shown.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Replace the rows. The window stays as it is, open or closed.
    pub fn set_rows(&mut self, rows: Vec<TagRow>) {
        self.rows = rows;
    }

    /// Draw the window when it is open.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Manage tags")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                list(ui, &self.rows);
            });
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        self.open = open && !escape;
    }
}

/// The text after the name: "N images", or "1 image".
fn count_text(count: u32) -> String {
    if count == 1 {
        "1 image".to_owned()
    } else {
        format!("{count} images")
    }
}

/// Draw the rows in a scroll area, or the empty text.
fn list(ui: &mut egui::Ui, rows: &[TagRow]) {
    if rows.is_empty() {
        ui.label(RichText::new("No user tags yet").color(tokens::TEXT_MUTED));
        return;
    }
    ScrollArea::vertical()
        .max_height(tokens::MENU_ROW_HEIGHT * MAX_VISIBLE_ROWS)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for row in rows {
                let (rect, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), tokens::MENU_ROW_HEIGHT),
                    Sense::hover(),
                );
                let count = ui.painter().layout_no_wrap(
                    count_text(row.count),
                    FontId::proportional(tokens::SIZE_BODY),
                    tokens::TEXT_MUTED,
                );
                let count_pos = rect.right_center() - vec2(count.size().x, count.size().y / 2.0);
                let name_rect = rect.with_max_x(count_pos.x - tokens::SPACING_S);
                ui.painter().galley(count_pos, count, tokens::TEXT_MUTED);
                // The name takes the room left of the count and ends in an ellipsis.
                let layout = Layout::left_to_right(Align::Center);
                ui.scope_builder(UiBuilder::new().max_rect(name_rect).layout(layout), |ui| {
                    ui.add(Label::new(RichText::new(&row.name)).truncate());
                });
            }
        });
}

#[cfg(test)]
mod tests;
