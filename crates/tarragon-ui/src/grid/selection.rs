//! Selection state for the gallery grid.

use std::collections::BTreeSet;

use egui::Modifiers;

/// The set of selected cell indices, plus the anchor of the last plain click.
///
/// A plain click sets the anchor. A shift click extends from it. A change of
/// the view drops hidden indices at [`Selection::retain_from`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    indices: BTreeSet<usize>,
    anchor: Option<usize>,
}

impl Selection {
    /// Drop every selected index.
    pub fn clear(&mut self) {
        self.indices.clear();
        self.anchor = None;
    }

    /// Return whether `index` is selected.
    #[must_use]
    pub fn contains(&self, index: usize) -> bool {
        self.indices.contains(&index)
    }

    /// Add `index` when absent, or remove it when present.
    pub fn toggle(&mut self, index: usize) {
        if !self.indices.insert(index) {
            self.indices.remove(&index);
        }
    }

    /// Select `index` and no other. This call sets the anchor.
    pub fn select_only(&mut self, index: usize) {
        self.indices.clear();
        self.indices.insert(index);
        self.anchor = Some(index);
    }

    /// Select every index from `anchor` to `target`, in either direction.
    ///
    /// The anchor of this selection does not change.
    pub fn extend_range(&mut self, anchor: usize, target: usize) {
        let (low, high) = if anchor <= target {
            (anchor, target)
        } else {
            (target, anchor)
        };
        self.indices = (low..=high).collect();
    }

    /// Select the display positions from `anchor` to `target`, in either
    /// direction, and map each position through `order`.
    fn extend_positions(&mut self, anchor: usize, target: usize, order: &[usize]) {
        let (low, high) = if anchor <= target {
            (anchor, target)
        } else {
            (target, anchor)
        };
        let Some(positions) = order.get(low..=high) else {
            return;
        };
        self.indices = positions.iter().copied().collect();
    }

    /// Return the number of selected indices.
    #[must_use]
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Return whether no index is selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Iterate the selected indices in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.indices.iter().copied()
    }

    /// Keep only the selected indices that `order` holds.
    ///
    /// `order` holds the visible item indices. The anchor is dropped when it
    /// leaves that view.
    pub fn retain_from(&mut self, order: &[usize]) {
        let visible: BTreeSet<usize> = order.iter().copied().collect();
        self.indices.retain(|index| visible.contains(index));
        if self.anchor.is_some_and(|anchor| !visible.contains(&anchor)) {
            self.anchor = None;
        }
    }

    /// Apply one cell click and its modifier keys.
    ///
    /// `position` is the display position of the clicked tile, and `order`
    /// maps a display position to an item index. A plain click selects only
    /// the cell. Control or command toggles the cell. Shift selects the range
    /// from the last anchor to the cell in display order.
    pub fn apply_click(&mut self, position: usize, order: &[usize], modifiers: Modifiers) {
        let Some(&index) = order.get(position) else {
            return;
        };
        if modifiers.shift {
            let anchor = self
                .anchor
                .and_then(|anchor| order.iter().position(|&item| item == anchor))
                .unwrap_or(position);
            self.extend_positions(anchor, position, order);
        } else if modifiers.command || modifiers.ctrl {
            self.toggle(index);
            if self.indices.contains(&index) {
                self.anchor = Some(index);
            }
        } else {
            self.select_only(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An order where the display position equals the item index.
    const IDENTITY_ORDER: [usize; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

    #[test]
    fn a_new_selection_is_empty() {
        let selection = Selection::default();
        assert!(selection.is_empty());
        assert_eq!(selection.len(), 0);
        assert!(!selection.contains(0));
        assert_eq!(selection.iter().count(), 0);
    }

    #[test]
    fn toggle_adds_and_removes_one_index() {
        let mut selection = Selection::default();
        selection.toggle(4);
        assert!(selection.contains(4));
        assert_eq!(selection.len(), 1);
        selection.toggle(4);
        assert!(!selection.contains(4));
        assert!(selection.is_empty());
    }

    #[test]
    fn select_only_replaces_the_set() {
        let mut selection = Selection::default();
        selection.select_only(2);
        selection.select_only(7);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [7]);
        assert!(!selection.contains(2));
    }

    #[test]
    fn clear_drops_every_index() {
        let mut selection = Selection::default();
        selection.select_only(3);
        selection.toggle(5);
        selection.clear();
        assert!(selection.is_empty());
        assert!(!selection.contains(3));
    }

    #[test]
    fn extend_range_covers_both_directions() {
        let mut selection = Selection::default();
        selection.extend_range(2, 5);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [2, 3, 4, 5]);
        selection.extend_range(5, 2);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [2, 3, 4, 5]);
        selection.extend_range(3, 3);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [3]);
    }

    #[test]
    fn iter_is_ascending_through_btree_order() {
        let mut selection = Selection::default();
        for index in [9, 1, 4] {
            selection.toggle(index);
        }
        assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 4, 9]);
    }

    #[test]
    fn retain_from_drops_indices_outside_the_visible_order() {
        let mut selection = Selection::default();
        for index in [1, 4, 7] {
            selection.toggle(index);
        }
        selection.retain_from(&[4, 5]);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [4]);
    }

    #[test]
    fn retain_from_keeps_a_visible_anchor() {
        let mut selection = Selection::default();
        selection.select_only(3);
        selection.retain_from(&[2, 3, 4]);
        selection.apply_click(0, &[2, 3, 4], Modifiers::SHIFT);
        assert_eq!(
            selection.iter().collect::<Vec<_>>(),
            [2, 3],
            "the anchor still starts a shift range"
        );
    }

    #[test]
    fn retain_from_drops_an_anchor_that_left_the_view() {
        let mut selection = Selection::default();
        selection.select_only(9);
        selection.retain_from(&[0, 1]);
        assert_eq!(selection, Selection::default(), "the anchor left too");
    }

    #[test]
    fn a_plain_click_selects_only_that_cell() {
        let mut selection = Selection::default();
        selection.select_only(1);
        selection.apply_click(6, &IDENTITY_ORDER, Modifiers::NONE);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [6]);
    }

    #[test]
    fn a_control_click_toggles_the_cell() {
        let mut selection = Selection::default();
        selection.select_only(1);
        selection.apply_click(6, &IDENTITY_ORDER, Modifiers::CTRL);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 6]);
        selection.apply_click(1, &IDENTITY_ORDER, Modifiers::COMMAND);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [6]);
    }

    #[test]
    fn a_shift_click_extends_from_the_last_plain_click() {
        let mut selection = Selection::default();
        selection.apply_click(1, &IDENTITY_ORDER, Modifiers::NONE);
        selection.apply_click(4, &IDENTITY_ORDER, Modifiers::SHIFT);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 2, 3, 4]);
        selection.apply_click(2, &IDENTITY_ORDER, Modifiers::SHIFT);
        assert_eq!(
            selection.iter().collect::<Vec<_>>(),
            [1, 2],
            "the anchor stays at the plain click"
        );
    }

    #[test]
    fn a_shift_click_extends_over_display_positions() {
        // The order is not monotonic, so item indices and positions differ.
        let order = [3, 1, 0, 2];
        let mut selection = Selection::default();
        selection.apply_click(0, &order, Modifiers::NONE);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [3]);
        selection.apply_click(2, &order, Modifiers::SHIFT);
        assert_eq!(
            selection.iter().collect::<Vec<_>>(),
            [0, 1, 3],
            "positions 0, 1, and 2 map to indices 3, 1, and 0"
        );
        assert_eq!(selection.len(), 3, "only the three tiles in the range");
    }

    #[test]
    fn a_ctrl_click_that_deselects_keeps_the_anchor() {
        let mut selection = Selection::default();
        selection.apply_click(2, &IDENTITY_ORDER, Modifiers::NONE);
        selection.apply_click(5, &IDENTITY_ORDER, Modifiers::CTRL);
        selection.apply_click(2, &IDENTITY_ORDER, Modifiers::CTRL);
        assert!(!selection.contains(2));

        selection.apply_click(7, &IDENTITY_ORDER, Modifiers::SHIFT);

        assert_eq!(selection.iter().collect::<Vec<_>>(), [5, 6, 7]);
    }

    #[test]
    fn a_shift_click_without_an_anchor_selects_one_cell() {
        let mut selection = Selection::default();
        selection.apply_click(5, &IDENTITY_ORDER, Modifiers::SHIFT);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [5]);
    }
}
