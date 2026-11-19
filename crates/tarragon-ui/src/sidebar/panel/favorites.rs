//! The favorites section of the Library panel: rows and drag reorder.
//!
//! During a drag the lifted row follows the pointer, and the other rows slide
//! to the places they take if the row is dropped now.

use std::path::{Path, PathBuf};

use egui::{Id, Key, Ui, vec2};

use super::super::display_label;
use super::super::favorite::{
    FAVORITE_HEIGHT, FavoriteAction, paint_lifted_border, paint_lifted_row, show_empty_favorites,
    show_favorite_row,
};
use super::super::reorder::{drop_slot, new_places, reordered};
use super::{Sidebar, SidebarEvent};

/// The time a row takes to slide to a new place, in seconds.
const SLIDE_TIME: f32 = 0.15;

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
        let count = self.state.favorites().len();
        let pitch = FAVORITE_HEIGHT + ui.spacing().item_spacing.y;
        let top = ui.cursor().min.y;
        let spans: Vec<(f32, f32)> = (0..count)
            .map(|index| {
                let row_top = top + place_y(index, pitch);
                (row_top, row_top + FAVORITE_HEIGHT)
            })
            .collect();
        let slot = self.dragging.and_then(|_| pointer_slot(ui, &spans));
        let places = match (self.dragging, slot) {
            (Some(from), Some(slot)) => new_places(count, from, slot),
            _ => (0..count).collect(),
        };
        let lifted_y = self
            .dragging
            .map(|from| lifted_offset(ui, from, count, pitch));
        let (mut opened, mut dropped, mut lifted_rect) = (None, false, None);
        ui.push_id("favorites", |ui| {
            for (index, fav) in self.state.favorites().iter().enumerate() {
                let label = display_label(&fav.path, fav.label.as_deref(), self.state.roots());
                let chosen = self.selected_favorite.as_deref() == Some(fav.path.as_path());
                let lifted = self.dragging == Some(index);
                let (target, time) = match lifted_y {
                    Some(y) if lifted => (y, 0.0),
                    _ => (place_y(places[index], pitch), SLIDE_TIME),
                };
                let shown = ui
                    .ctx()
                    .animate_value_with_time(slide_id(&fav.path), target, time);
                let shift = shown - place_y(index, pitch);
                let (action, home) =
                    show_favorite_row(ui, &label, chosen, fav.missing, lifted, index, shift);
                if lifted {
                    lifted_rect =
                        Some((home.translate(vec2(0.0, shift)), label, chosen, fav.missing));
                }
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
        if let Some((rect, label, chosen, missing)) = lifted_rect {
            paint_lifted_row(ui, &label, chosen, missing, rect);
            paint_lifted_border(ui, rect);
        }
        self.finish_drag(ui, slot, dropped);
    }

    /// Queue the new order on release, and keep frames coming while dragging.
    ///
    /// A lost release (no button down, no drop seen) ends the drag.
    fn finish_drag(&mut self, ui: &Ui, slot: Option<usize>, dropped: bool) {
        let Some(from) = self.dragging else {
            return;
        };
        if !dropped && !ui.input(|input| input.pointer.any_down()) {
            self.dragging = None;
            return;
        }
        ui.ctx().request_repaint();
        if !dropped {
            return;
        }
        self.dragging = None;
        let paths: Vec<PathBuf> = self
            .state
            .favorites()
            .iter()
            .map(|f| f.path.clone())
            .collect();
        if let Some(order) = slot.and_then(|slot| reordered(&paths, from, slot)) {
            self.events.push(SidebarEvent::Reorder(order));
        }
    }
}

/// The top of place `place` below the top of the list, in points.
#[expect(
    clippy::cast_precision_loss,
    reason = "a favorites list never nears 2^24 rows"
)]
fn place_y(place: usize, pitch: f32) -> f32 {
    place as f32 * pitch
}

/// The animation id of a favorite's place: keyed by path, so a row keeps its
/// place when the order changes.
fn slide_id(path: &Path) -> Id {
    Id::new("favorite-slide").with(path)
}

/// The insertion slot under the pointer, or none outside the clip.
///
/// Inside the clip, a pointer above the list means the first slot and one
/// below it the last slot, which matches where the lifted row is held.
pub(super) fn pointer_slot(ui: &Ui, spans: &[(f32, f32)]) -> Option<usize> {
    let pointer = ui.input(|input| input.pointer.latest_pos());
    let inside = pointer.filter(|pos| ui.clip_rect().contains(*pos))?;
    let (top, bottom) = (spans.first()?.0, spans.last()?.1);
    drop_slot(spans, inside.y.clamp(top, bottom))
}

/// The top of the lifted row below the top of the list.
///
/// The row keeps the grab point under the pointer and stays between the
/// first and the last place.
fn lifted_offset(ui: &Ui, from: usize, count: usize, pitch: f32) -> f32 {
    let moved = ui.input(|input| {
        let pointer = &input.pointer;
        match (pointer.press_origin(), pointer.latest_pos()) {
            (Some(origin), Some(pos)) => pos.y - origin.y,
            _ => 0.0,
        }
    });
    let last = place_y(count.saturating_sub(1), pitch);
    (place_y(from, pitch) + moved).clamp(0.0, last)
}
