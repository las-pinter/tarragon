//! Minimum Gallery width, kept by moving a split fraction.

use super::Pane;
use egui_dock::{DockState, Node};

/// Slack for float noise only, so a fraction set here does not count as short on the next call.
const SLACK: f32 = 0.01;

/// Widens the Gallery to at least `min` points by moving the fraction of its
/// nearest side-by-side split. Returns true when a fraction changed.
///
/// `separator` is the divider width of the dock style. The fraction sets the
/// divider midpoint, so each side loses half the divider.
///
/// `egui_dock` has no per-pane minimum, and its divider limit is global and
/// acts only on a drag. This runs after the dock is drawn, so it also holds
/// when the window shrinks. It reads the split rects of the last draw.
pub fn keep_gallery_wide(dock: &mut DockState<Pane>, min: f32, separator: f32) -> bool {
    let tree = dock.main_surface_mut();
    let Some((mut node, _)) = tree.find_tab(&Pane::Gallery) else {
        return false;
    };
    // A split above the Log shares the Gallery width, so only a Horizontal one counts.
    while let Some(parent) = node.parent() {
        if let Node::Horizontal(split) = &mut tree[parent] {
            let range = split.rect.width();
            let wanted = (min + separator * 0.5) / range;
            if !range.is_finite() || wanted > 1.0 {
                return false;
            }
            let left = node.is_left();
            let share = if left {
                split.fraction
            } else {
                1.0 - split.fraction
            };
            if share * range - separator * 0.5 >= min - SLACK {
                return false;
            }
            // The Gallery wins over its neighbour, by design: a narrow range leaves the neighbour less.
            split.fraction = if left { wanted } else { 1.0 - wanted };
            return true;
        }
        node = parent;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{default_dock, first_dock};
    use crate::tokens::{GALLERY_MIN_WIDTH, WINDOW_SIZE};
    use egui::{Pos2, Rect, vec2};
    use egui_dock::NodeIndex;

    /// The `egui_dock` default divider width.
    const SEP: f32 = 1.0;

    fn rect(left: f32, width: f32) -> Rect {
        Rect::from_min_size(Pos2::new(left, 0.0), vec2(width, 800.0))
    }

    fn split_mut(dock: &mut DockState<Pane>, node: NodeIndex) -> &mut egui_dock::SplitNode {
        match &mut dock.main_surface_mut()[node] {
            Node::Horizontal(split) | Node::Vertical(split) => split,
            _ => panic!("not a split"),
        }
    }

    /// Pane width as the last draw would give it, for the Horizontal parent at `node`.
    /// The divider midpoint sits on the fraction, so each side loses half the divider.
    fn gallery_width(dock: &mut DockState<Pane>, node: NodeIndex, left: bool) -> f32 {
        let split = split_mut(dock, node);
        let share = if left {
            split.fraction
        } else {
            1.0 - split.fraction
        };
        share * split.rect.width() - SEP * 0.5
    }

    /// The default layout with the split rects of a window `width` wide.
    fn drawn(width: f32) -> DockState<Pane> {
        let mut dock = default_dock();
        let root = NodeIndex::root();
        let library = split_mut(&mut dock, root);
        library.rect = rect(0.0, width);
        let left = library.fraction * width;
        split_mut(&mut dock, root.right()).rect = rect(left, width - left);
        dock
    }

    #[test]
    fn a_narrow_gallery_between_two_panes_is_widened() {
        let mut dock = drawn(500.0);
        let rest = NodeIndex::root().right();
        assert!(gallery_width(&mut dock, rest, true) < 260.0);
        assert!(keep_gallery_wide(&mut dock, 260.0, SEP));
        assert!((gallery_width(&mut dock, rest, true) - 260.0).abs() < 0.01);
        // The neighbour keeps what is left, even when that is narrow.
        let range = split_mut(&mut dock, rest).rect.width();
        let neighbour = gallery_width(&mut dock, rest, false);
        assert!((neighbour - (range - 260.0 - SEP)).abs() < 0.01);
    }

    #[test]
    fn a_wide_enough_gallery_is_untouched() {
        let mut dock = drawn(WINDOW_SIZE[0]);
        let before = crate::layout::to_json(&dock);
        assert!(!keep_gallery_wide(&mut dock, GALLERY_MIN_WIDTH, SEP));
        assert_eq!(crate::layout::to_json(&dock), before);
    }

    #[test]
    fn a_second_call_changes_nothing() {
        let mut dock = drawn(500.0);
        assert!(keep_gallery_wide(&mut dock, 260.0, SEP));
        assert!(!keep_gallery_wide(&mut dock, 260.0, SEP));
    }

    #[test]
    fn the_gallery_as_left_child_is_widened() {
        let mut dock = first_dock();
        let rest = NodeIndex::root().right();
        split_mut(&mut dock, rest).rect = rect(0.0, 600.0);
        split_mut(&mut dock, rest).fraction = 0.2;
        assert!(keep_gallery_wide(&mut dock, 260.0, SEP));
        assert!((gallery_width(&mut dock, rest, true) - 260.0).abs() < 0.01);
    }

    #[test]
    fn the_gallery_as_right_child_is_widened() {
        let mut dock = DockState::new(vec![Pane::Preview]);
        let tree = dock.main_surface_mut();
        tree.split_right(NodeIndex::root(), 0.8, vec![Pane::Gallery]);
        split_mut(&mut dock, NodeIndex::root()).rect = rect(0.0, 600.0);
        assert!(keep_gallery_wide(&mut dock, 260.0, SEP));
        assert!((gallery_width(&mut dock, NodeIndex::root(), false) - 260.0).abs() < 0.01);
    }

    #[test]
    fn the_gallery_above_the_log_uses_the_split_beside_it() {
        let mut dock = drawn(500.0);
        let rest = NodeIndex::root().right();
        assert!(matches!(
            dock.main_surface()[rest.left()],
            Node::Vertical(_)
        ));
        assert!(keep_gallery_wide(&mut dock, 260.0, SEP));
        assert!((gallery_width(&mut dock, rest, true) - 260.0).abs() < 0.01);
        // The vertical split between Gallery and Log is not touched.
        assert!((split_mut(&mut dock, rest.left()).fraction - 0.75).abs() < 1e-6);
    }

    #[test]
    fn a_lone_gallery_changes_nothing() {
        let mut dock = DockState::new(vec![Pane::Gallery]);
        assert!(!keep_gallery_wide(&mut dock, 260.0, SEP));
        let mut none = DockState::new(vec![Pane::Library]);
        assert!(!keep_gallery_wide(&mut none, 260.0, SEP));
    }

    #[test]
    fn a_parent_narrower_than_the_minimum_changes_nothing() {
        let mut dock = DockState::new(vec![Pane::Preview]);
        dock.main_surface_mut()
            .split_right(NodeIndex::root(), 0.8, vec![Pane::Gallery]);
        split_mut(&mut dock, NodeIndex::root()).rect = rect(0.0, 200.0);
        assert!(!keep_gallery_wide(&mut dock, 260.0, SEP));
        assert!((split_mut(&mut dock, NodeIndex::root()).fraction - 0.8).abs() < 1e-6);
    }

    #[test]
    fn an_undrawn_dock_changes_nothing() {
        let mut dock = default_dock();
        assert!(!keep_gallery_wide(&mut dock, 260.0, SEP));
    }

    #[test]
    fn the_default_layout_keeps_the_default_widths() {
        let mut dock = drawn(WINDOW_SIZE[0]);
        let root = NodeIndex::root();
        assert!(!keep_gallery_wide(&mut dock, GALLERY_MIN_WIDTH, SEP));
        assert!((gallery_width(&mut dock, root.right(), true) - (570.0 - SEP * 0.5)).abs() < 0.01);
        assert!((split_mut(&mut dock, root).fraction * WINDOW_SIZE[0] - 220.0).abs() < 0.5);
    }
}
