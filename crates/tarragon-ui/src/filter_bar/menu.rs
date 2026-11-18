//! A check menu for the filter bar: rows with a leading tick when checked.

use egui::{Align2, Area, Color32, FontId, Id, Key, Order, Rect, ScrollArea, Sense, Ui, vec2};

use self::layout::{
    FOOTER_HEIGHT, FOOTER_SEPARATOR, LEAD_GAP, MENU_ROWS, TICK_WIDTH, menu_height, paint_tick,
    tick_box,
};
use crate::popup::{menu_frame, menu_placement, row_frame, text_width};
use crate::tokens;

mod layout;

/// What the menu did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuResult {
    /// The row at this index was clicked. The menu stays open.
    Toggled(usize),
    /// The footer row was clicked. The menu closes.
    Footer,
    /// An outside click or Escape closed the menu.
    Closed,
    /// Nothing happened.
    Open,
}

/// Draw the menu under `button_rect`, or above it at the bottom edge. Each
/// row is a label and whether it is checked. The caller flips the state.
///
/// With a `footer`, a plain row under a separator closes the menu on a click.
#[must_use]
pub fn show_check_menu(
    ui: &Ui,
    id: Id,
    button_rect: Rect,
    rows: &[(String, bool)],
    footer: Option<&str>,
) -> MenuResult {
    let bounds = ui.ctx().content_rect().shrink(tokens::MENU_EDGE_MARGIN);
    let height = menu_height(rows.len()) + footer.map_or(0.0, |_| FOOTER_HEIGHT);
    let (pos, pivot) = menu_placement(button_rect, height, bounds);
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
                        .chain(footer.map(label))
                        .fold(label("No items"), f32::max);
                    let width = widest + 2.0 * tokens::MENU_ROW_PAD_X + TICK_WIDTH + LEAD_GAP;
                    // Only the rows scroll, so the footer stays reachable
                    // however many rows there are.
                    let hit = ScrollArea::vertical()
                        .max_height(MENU_ROWS * tokens::MENU_ROW_HEIGHT)
                        .auto_shrink([true, true])
                        .show(ui, |ui| {
                            if rows.is_empty() {
                                let size = vec2(width, tokens::MENU_ROW_HEIGHT);
                                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                                paint_label(ui, rect, "No items", tokens::TEXT_MUTED);
                            }
                            rows.iter()
                                .enumerate()
                                .fold(None, |hit, (i, (text, checked))| {
                                    let (rect, response, color) = row_frame(ui, width);
                                    if *checked {
                                        paint_tick(ui, tick_box(rect));
                                    }
                                    paint_label(ui, rect, text, color);
                                    hit.or(response.clicked().then_some(i))
                                })
                        })
                        .inner;
                    let Some(text) = footer else {
                        return hit.map(MenuResult::Toggled);
                    };
                    ui.add(egui::Separator::default().spacing(FOOTER_SEPARATOR));
                    let (rect, response, color) = row_frame(ui, width);
                    paint_label(ui, rect, text, color);
                    response
                        .clicked()
                        .then_some(MenuResult::Footer)
                        .or(hit.map(MenuResult::Toggled))
                })
                .inner
        });
    if let Some(result) = area.inner {
        return result;
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

/// Paint `text` in `row`, after the tick column.
fn paint_label(ui: &Ui, row: Rect, text: &str, color: Color32) {
    let at = tick_box(row).right_center() + vec2(LEAD_GAP, 0.0);
    ui.painter().text(
        at,
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(tokens::SIZE_BODY),
        color,
    );
}

#[cfg(test)]
mod tests;
