//! A check menu for the filter bar: rows with a leading tick when checked.

use egui::{Align2, Area, FontId, Id, Key, Order, Rect, ScrollArea, Sense, Ui, vec2};

use self::layout::{LEAD_GAP, MENU_ROWS, TICK_WIDTH, menu_height, paint_tick, tick_box};
use crate::popup::{menu_frame, menu_placement, row_frame, text_width};
use crate::tokens;

mod layout;

/// What the menu did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuResult {
    /// The row at this index was clicked. The menu stays open.
    Toggled(usize),
    /// An outside click or Escape closed the menu.
    Closed,
    /// Nothing happened.
    Open,
}

/// Draw the menu under `button_rect`, or above it at the bottom edge. Each
/// row is a label and whether it is checked. The caller flips the state.
#[must_use]
pub fn show_check_menu(ui: &Ui, id: Id, button_rect: Rect, rows: &[(String, bool)]) -> MenuResult {
    let bounds = ui.ctx().content_rect().shrink(tokens::MENU_EDGE_MARGIN);
    let (pos, pivot) = menu_placement(button_rect, menu_height(rows.len()), bounds);
    let area = Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(pos)
        .pivot(pivot)
        .constrain_to(bounds)
        .show(ui.ctx(), |ui| {
            menu_frame()
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let label = |t: &str| text_width(ui, t);
                    let widest = rows
                        .iter()
                        .map(|(t, _)| label(t))
                        .fold(label("No items"), f32::max);
                    let width = widest + 2.0 * tokens::MENU_ROW_PAD_X + TICK_WIDTH + LEAD_GAP;
                    ScrollArea::vertical()
                        .max_height(MENU_ROWS * tokens::MENU_ROW_HEIGHT)
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            if rows.is_empty() {
                                let size = vec2(width, tokens::MENU_ROW_HEIGHT);
                                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                                let at = tick_box(rect).right_center() + vec2(LEAD_GAP, 0.0);
                                ui.painter().text(
                                    at,
                                    Align2::LEFT_CENTER,
                                    "No items",
                                    FontId::proportional(tokens::SIZE_BODY),
                                    tokens::TEXT_MUTED,
                                );
                            }
                            rows.iter()
                                .enumerate()
                                .fold(None, |hit, (i, (text, checked))| {
                                    let (rect, response, color) = row_frame(ui, width);
                                    let tick = tick_box(rect);
                                    if *checked {
                                        paint_tick(ui, tick);
                                    }
                                    let at = tick.right_center() + vec2(LEAD_GAP, 0.0);
                                    ui.painter().text(
                                        at,
                                        Align2::LEFT_CENTER,
                                        text,
                                        FontId::proportional(tokens::SIZE_BODY),
                                        color,
                                    );
                                    hit.or(response.clicked().then_some(i))
                                })
                        })
                        .inner
                })
                .inner
        });
    if let Some(i) = area.inner {
        return MenuResult::Toggled(i);
    }
    let escape = ui.input(|i| i.key_pressed(Key::Escape));
    let outside = ui.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !area.response.rect.contains(p) && !button_rect.contains(p))
    });
    if escape || outside {
        MenuResult::Closed
    } else {
        MenuResult::Open
    }
}

#[cfg(test)]
mod tests;
