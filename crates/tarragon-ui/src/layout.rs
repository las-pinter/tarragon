//! Dock layout for the four main panels.

use crate::tokens::{PREVIEW_WIDTH, SIDEBAR_WIDTH};
use egui::Rect;
use egui_dock::{DockState, Node, NodeIndex};
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

/// Every pane, for the duplicate check on a loaded layout.
const ALL: [Pane; 4] = [Pane::Library, Pane::Gallery, Pane::Preview, Pane::Log];

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

/// Writes the dock as JSON for the saved window state.
///
/// # Panics
///
/// Never in practice: every rect is zeroed first, so serialization cannot fail.
#[must_use]
pub fn to_json(dock: &DockState<Pane>) -> String {
    // An undrawn dock holds infinite rects, which JSON writes as null and cannot read back.
    // The next frame sets every rect again, so zero is a safe stand-in.
    let mut dock = dock.clone();
    for (_, node) in dock.iter_all_nodes_mut() {
        match node {
            Node::Leaf(leaf) => (leaf.rect, leaf.viewport) = (Rect::ZERO, Rect::ZERO),
            Node::Horizontal(split) | Node::Vertical(split) => split.rect = Rect::ZERO,
            Node::Empty => {}
        }
    }
    serde_json::to_string(&dock).expect("every rect is finite, so the dock serializes")
}

/// Reads a saved dock. A pane that is not in the tree is hidden, so any subset is
/// valid. Gives `None` for no panes or a pane twice.
#[must_use]
pub fn from_json(text: &str) -> Option<DockState<Pane>> {
    let dock: DockState<Pane> = serde_json::from_str(text).ok()?;
    // A stale or hand-edited layout must not show a panel twice.
    let panes: Vec<Pane> = dock.iter_all_tabs().map(|(_, pane)| *pane).collect();
    let once = ALL
        .iter()
        .all(|one| panes.iter().filter(|p| *p == one).count() <= 1);
    // A fraction outside 0..=1 would make egui_dock draw a negative-size panel.
    let fractions_ok = dock.iter_all_nodes().all(|(_, node)| match node {
        Node::Horizontal(split) | Node::Vertical(split) => (0.0..=1.0).contains(&split.fraction),
        _ => true,
    });
    (!panes.is_empty() && once && fractions_ok).then_some(dock)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_saved_layout_loads_back_the_same() {
        let json = to_json(&default_dock());
        let back = from_json(&json).expect("default layout loads");
        assert_eq!(to_json(&back), json);
        assert_eq!(
            node_of(&back, Pane::Log),
            node_of(&default_dock(), Pane::Log)
        );
    }

    #[test]
    fn a_layout_without_one_pane_loads_without_it() {
        let three = to_json(&DockState::new(vec![
            Pane::Library,
            Pane::Gallery,
            Pane::Preview,
        ]));
        let back = from_json(&three).expect("a subset loads");
        assert_eq!(back.iter_all_tabs().count(), 3);
        assert!(back.main_surface().find_tab(&Pane::Log).is_none());
    }

    #[test]
    fn a_layout_that_is_not_valid_gives_none() {
        let good = to_json(&default_dock());
        let none = to_json(&DockState::new(vec![]));
        let wide = good.replacen("0.171875", "1.5", 1);
        assert_ne!(wide, good);
        let twice = to_json(&DockState::new(vec![
            Pane::Library,
            Pane::Library,
            Pane::Gallery,
        ]));
        assert!(good.contains("\"Log\""));
        for bad in [
            String::new(),
            "not json".into(),
            good.replace("\"Log\"", "\"Bogus\""),
            none,
            twice,
            wide,
        ] {
            assert!(from_json(&bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_moved_divider_survives_a_round_trip() {
        let mut dock = default_dock();
        let Node::Horizontal(split) = &mut dock.main_surface_mut()[NodeIndex::root()] else {
            panic!("root is a horizontal split");
        };
        split.fraction = 0.4;
        let back = from_json(&to_json(&dock)).expect("layout loads");
        assert!((fraction(&back.main_surface()[NodeIndex::root()]) - 0.4).abs() < 1e-6);
    }
}
