//! The shared shape of the tag and folder filters: a menu button and chips.

use std::fmt::Debug;
use std::hash::Hash;

use egui::{Id, Ui, Vec2};

use super::chip::show_chip;
use super::menu::{MenuResult, show_check_menu};
use crate::tokens;
use crate::widgets::small_button;

/// What the user changed this frame.
pub(super) enum Edit {
    /// The x of the chip at this index was clicked.
    Chip(usize),
    /// The menu row at this index was clicked.
    Row(usize),
}

/// Draw a wrapping row of an optional menu button and the chips.
///
/// `chips` hold a label and a tooltip. `rows` hold a label and a tick. With no
/// `button` there is no menu. `id_salt` keys the open state of the menu.
pub(super) fn show_list(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    button: Option<&str>,
    chips: &[(String, String)],
    rows: &[(String, bool)],
) -> Option<Edit> {
    let open_id = Id::new(id_salt).with("open");
    let mut open = button.is_some() && ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
    let mut edit = None;
    let rect = ui
        .horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(tokens::SPACING_S);
            let rect = button.map(|label| {
                let response = small_button(ui, label);
                open ^= response.clicked();
                response.rect
            });
            for (i, (label, tip)) in chips.iter().enumerate() {
                if show_chip(ui, label, tip) {
                    edit = Some(Edit::Chip(i));
                }
            }
            rect
        })
        .inner;
    if let (true, Some(rect)) = (open, rect) {
        match show_check_menu(ui, open_id.with("menu"), rect, rows) {
            MenuResult::Toggled(i) => edit = Some(Edit::Row(i)),
            MenuResult::Closed => open = false,
            MenuResult::Open => {}
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    edit
}
