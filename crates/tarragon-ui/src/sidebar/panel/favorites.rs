//! The favorites section of the Library panel: rows, drag reorder, and the drop line.

use std::path::PathBuf;

use egui::{CornerRadius, Key, Rect, Ui, pos2};

use super::super::display_label;
use super::super::favorite::{
    FavoriteAction, paint_lifted_border, show_empty_favorites, show_favorite_row,
};
use super::super::reorder::{drop_slot, reordered};
use super::{Sidebar, SidebarEvent};
use crate::tokens;

impl Sidebar {
    /// Draw the favorites, or the empty text, and apply their actions.
    pub(super) fn show_favorites(&mut self, ui: &mut Ui) {
        if self.state.favorites().is_empty() {
            show_empty_favorites(ui);
            return;
        }
        if ui.input(|input| input.key_pressed(Key::Escape)) {
            self.dragging = None;
        }
        let (mut opened, mut dropped, mut rects) = (None, false, Vec::new());
        ui.push_id("favorites", |ui| {
            for (index, fav) in self.state.favorites().iter().enumerate() {
                let label = display_label(&fav.path, fav.label.as_deref(), self.state.roots());
                let chosen = self.selected_favorite.as_deref() == Some(fav.path.as_path());
                let lifted = self.dragging == Some(index);
                let (action, rect) =
                    show_favorite_row(ui, &label, chosen, fav.missing, lifted, index);
                rects.push(rect);
                match action {
                    FavoriteAction::Open => {
                        opened = Some(fav.path.clone());
                        ui.ctx().request_repaint();
                    }
                    FavoriteAction::DragStarted => self.dragging = Some(index),
                    FavoriteAction::Dropped => dropped = true,
                    FavoriteAction::None => {}
                }
            }
        });
        if let Some(path) = opened {
            self.open(path.clone());
            self.events.push(SidebarEvent::Navigate(path));
        }
        if let Some(rect) = self.dragging.and_then(|from| rects.get(from)) {
            paint_lifted_border(ui, *rect);
        }
        self.finish_drag(ui, &rects, dropped);
    }

    /// Show the drop line while dragging, and queue the new order on release.
    ///
    /// A lost release (no button down, no drop seen) ends the drag.
    fn finish_drag(&mut self, ui: &Ui, rects: &[Rect], dropped: bool) {
        let Some(from) = self.dragging else {
            return;
        };
        if !dropped && !ui.input(|input| input.pointer.any_down()) {
            self.dragging = None;
            return;
        }
        let spans: Vec<(f32, f32)> = rects.iter().map(|r| (r.min.y, r.max.y)).collect();
        let pointer = ui.input(|input| input.pointer.latest_pos());
        let slot = pointer.and_then(|pos| drop_slot(&spans, pos.y));
        let paths: Vec<PathBuf> = self
            .state
            .favorites()
            .iter()
            .map(|f| f.path.clone())
            .collect();
        let order = slot.and_then(|slot| Some((slot, reordered(&paths, from, slot)?)));
        ui.ctx().request_repaint();
        if dropped {
            self.dragging = None;
            if let Some((_, order)) = order {
                self.events.push(SidebarEvent::Reorder(order));
            }
        } else if let Some((slot, _)) = order {
            drop_line(ui, rects, slot);
        }
    }
}

/// The drop line rect for a slot, or none for an empty list or a slot past the end.
///
/// Slot `k` is centred on the top of row `k`; slot `n` on the bottom of the last row.
/// The mockup's 2 px amber line has 6 px side margins; the row rects are already inset by 6.
pub(super) fn drop_line_rect(rects: &[Rect], slot: usize) -> Option<Rect> {
    let (first, last) = (rects.first()?, rects.last()?);
    let y = if slot == rects.len() {
        last.max.y
    } else {
        rects.get(slot)?.min.y
    };
    Some(Rect::from_min_max(
        pos2(first.min.x, y - 1.0),
        pos2(first.max.x, y + 1.0),
    ))
}

/// Paint the drop line at a slot, on the row edge.
fn drop_line(ui: &Ui, rects: &[Rect], slot: usize) {
    if let Some(line) = drop_line_rect(rects, slot) {
        ui.painter()
            .rect_filled(line, CornerRadius::same(1), tokens::AMBER_ACCENT);
    }
}
