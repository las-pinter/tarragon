//! The inline delete question of one row of the "Manage tags" window.

use egui::{Key, Label, Layout, Modifiers, RichText, UiBuilder, vec2};

use super::rename::count_text;
use super::{TagManagerEvent, TagRow};
use crate::tokens;

/// The question for `row`. A tag with no images has no "from" part.
fn question(row: &TagRow) -> String {
    if row.count == 0 {
        format!("Delete '{}'?", row.name)
    } else {
        format!("Delete '{}' from {}?", row.name, count_text(row.count))
    }
}

/// Draw the question and its buttons in place of the row. Delete queues the
/// event, Cancel and Escape queue nothing; each ends the confirm.
pub(super) fn confirm_row(
    ui: &mut egui::Ui,
    row: &TagRow,
    confirm: &mut Option<i64>,
    events: &mut Vec<TagManagerEvent>,
) {
    // Consumed here, so the window does not see the same Escape and close.
    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        *confirm = None;
        return;
    }
    let (row_rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), tokens::MENU_ROW_HEIGHT),
        egui::Sense::hover(),
    );
    let right_to_left = Layout::right_to_left(egui::Align::Center);
    let (cancel, delete, rest) = ui
        .scope_builder(
            UiBuilder::new().max_rect(row_rect).layout(right_to_left),
            |ui| {
                let cancel = ui.small_button("Cancel").clicked();
                let delete = ui.small_button("Delete").clicked();
                (cancel, delete, ui.available_rect_before_wrap())
            },
        )
        .inner;
    // The question takes the room left of the buttons and ends in an ellipsis.
    let text_rect = rest.with_max_x(rest.max.x - tokens::SPACING_S);
    let left_to_right = Layout::left_to_right(egui::Align::Center);
    ui.scope_builder(
        UiBuilder::new().max_rect(text_rect).layout(left_to_right),
        |ui| {
            ui.add(Label::new(RichText::new(question(row))).truncate());
        },
    );
    if delete {
        events.push(TagManagerEvent::Delete { id: row.id });
    }
    if delete || cancel {
        *confirm = None;
    }
}
