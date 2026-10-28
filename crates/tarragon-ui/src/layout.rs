//! Dock layout for the four main panels.

use crate::tokens::{PREVIEW_WIDTH, SIDEBAR_WIDTH};
use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

/// Window width the default split fractions are computed for.
const DEFAULT_WIDTH: f32 = 1280.0;
/// Share of the height that the Gallery keeps above the Log.
const GALLERY_HEIGHT: f32 = 0.75;

/// One dockable panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pane {
    /// Folder tree and favorites.
    Library,
    /// Thumbnail grid.
    Gallery,
    /// Large image view.
    Preview,
    /// Event log.
    Log,
}

/// Builds the first-run layout: Library, Gallery, Preview, and Log under Gallery.
#[must_use]
pub fn default_dock() -> DockState<Pane> {
    // Splits are fractions, so the pixel widths are turned into shares of the parent.
    let rest = DEFAULT_WIDTH - SIDEBAR_WIDTH;
    let mut dock = DockState::new(vec![Pane::Library]);
    let tree = dock.main_surface_mut();
    let [_, right] = tree.split_right(
        NodeIndex::root(),
        SIDEBAR_WIDTH / DEFAULT_WIDTH,
        vec![Pane::Gallery],
    );
    let [gallery, _] = tree.split_right(right, 1.0 - PREVIEW_WIDTH / rest, vec![Pane::Preview]);
    tree.split_below(gallery, GALLERY_HEIGHT, vec![Pane::Log]);
    dock
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_dock::Node;

    fn node_of(dock: &DockState<Pane>, pane: Pane) -> NodeIndex {
        dock.main_surface().find_tab(&pane).expect("pane in dock").0
    }

    #[test]
    fn default_dock_has_each_pane_once() {
        let dock = default_dock();
        let panes: Vec<Pane> = dock.iter_all_tabs().map(|(_, p)| *p).collect();
        assert_eq!(panes.len(), 4);
        for pane in [Pane::Library, Pane::Gallery, Pane::Preview, Pane::Log] {
            assert_eq!(panes.iter().filter(|p| **p == pane).count(), 1);
        }
    }

    #[test]
    fn default_dock_places_panes() {
        let dock = default_dock();
        let tree = dock.main_surface();
        let root = NodeIndex::root();
        let rest = root.right();
        let middle = rest.left();
        // Side-by-side splits are Horizontal nodes; stacked splits are Vertical.
        assert!(matches!(tree[root], Node::Horizontal(_)));
        assert!(matches!(tree[rest], Node::Horizontal(_)));
        assert!(matches!(tree[middle], Node::Vertical(_)));
        assert_eq!(node_of(&dock, Pane::Library), root.left());
        assert_eq!(node_of(&dock, Pane::Gallery), middle.left());
        assert_eq!(node_of(&dock, Pane::Log), middle.right());
        assert_eq!(node_of(&dock, Pane::Preview), rest.right());
    }

    fn fraction(node: &Node<Pane>) -> f32 {
        match node {
            Node::Horizontal(split) | Node::Vertical(split) => split.fraction,
            _ => panic!("not a split node"),
        }
    }

    #[test]
    fn default_dock_split_fractions_follow_the_token_widths() {
        let dock = default_dock();
        let tree = dock.main_surface();
        let root = NodeIndex::root();
        let rest = DEFAULT_WIDTH - SIDEBAR_WIDTH;
        let expected = [
            (root, SIDEBAR_WIDTH / DEFAULT_WIDTH),
            (root.right(), 1.0 - PREVIEW_WIDTH / rest),
            (root.right().left(), GALLERY_HEIGHT),
        ];
        for (index, want) in expected {
            let got = fraction(&tree[index]);
            assert!((got - want).abs() < 1e-6, "{index:?}: {got} != {want}");
        }
    }
}
