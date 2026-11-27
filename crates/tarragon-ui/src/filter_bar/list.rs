//! A text action that opens a check menu: the shared shape of "Tags" and
//! "Folders".

use std::fmt::Debug;
use std::hash::Hash;

use egui::{Id, Ui};

use super::action::{Kind, text_action};
use super::menu::{MenuResult, show_check_menu};

/// What the user picked in a menu this frame.
pub(super) enum Edit {
    /// The menu row at this index was clicked.
    Row(usize),
    /// The menu footer row was clicked.
    Footer,
}

/// What the filter bar did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// The filter changed.
    pub changed: bool,
    /// A menu went from closed to open.
    pub opened: bool,
    /// The "Manage tags..." row was clicked.
    pub manage_tags: bool,
}

impl std::ops::BitOrAssign for Shown {
    fn bitor_assign(&mut self, other: Self) {
        self.changed |= other.changed;
        self.opened |= other.opened;
        self.manage_tags |= other.manage_tags;
    }
}

/// The memory key of a menu's open state.
fn open_id(id_salt: impl Hash + Debug) -> Id {
    Id::new(id_salt).with("open")
}

/// Close the menu of `id_salt`. The bar calls it while the action is hidden,
/// so a menu left open does not come back open with its action.
pub(super) fn close_menu(ui: &Ui, id_salt: impl Hash + Debug) {
    ui.data_mut(|d| d.insert_temp(open_id(id_salt), false));
}

/// Draw the action `label` and, while it is open, its check menu.
///
/// `rows` hold a label and a tick. `footer` is an optional last menu row.
/// `id_salt` keys the open state of the menu. The flag is true when the menu
/// opened this frame.
pub(super) fn show_menu_action(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    label: &str,
    rows: &[(String, bool)],
    footer: Option<&str>,
) -> (Option<Edit>, bool) {
    let open_id = open_id(id_salt);
    let was_open = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
    let response = text_action(ui, label, Kind::Menu);
    let open = was_open ^ response.clicked();
    let opened = open && !was_open;
    let (edit, open) = if open {
        match show_check_menu(ui, open_id.with("menu"), response.rect, rows, footer) {
            MenuResult::Toggled(i) => (Some(Edit::Row(i)), true),
            MenuResult::Footer => (Some(Edit::Footer), false),
            MenuResult::Closed => (None, false),
            MenuResult::Open => (None, true),
        }
    } else {
        (None, false)
    };
    ui.data_mut(|d| d.insert_temp(open_id, open));
    (edit, opened)
}
