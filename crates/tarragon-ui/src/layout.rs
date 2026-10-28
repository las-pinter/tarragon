//! Dock layout for the four main panels.

use crate::tokens::{PREVIEW_WIDTH, SIDEBAR_WIDTH, WINDOW_SIZE};
use egui::Rect;
use egui_dock::{DockState, Node, NodeIndex, Tree};
use serde::{Deserialize, Serialize};

/// Window width the default split fractions are computed for.
const DEFAULT_WIDTH: f32 = WINDOW_SIZE[0];
/// Share of the height that the Gallery keeps above the Log.
const GALLERY_HEIGHT: f32 = 0.75;
/// Share of the width that the Library takes on the left.
const LIBRARY_SHARE: f32 = SIDEBAR_WIDTH / DEFAULT_WIDTH;
/// Share of the width that the Gallery keeps left of the Preview, once the Library is split off.
const GALLERY_SHARE: f32 = 1.0 - PREVIEW_WIDTH / (DEFAULT_WIDTH - SIDEBAR_WIDTH);

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
    let mut dock = DockState::new(vec![Pane::Library]);
    let tree = dock.main_surface_mut();
    let [_, right] = tree.split_right(NodeIndex::root(), LIBRARY_SHARE, vec![Pane::Gallery]);
    let [gallery, _] = tree.split_right(right, GALLERY_SHARE, vec![Pane::Preview]);
    tree.split_below(gallery, GALLERY_HEIGHT, vec![Pane::Log]);
    dock
}

/// Builds the layout of a fresh start: the default layout with the Log hidden.
#[must_use]
pub fn first_dock() -> DockState<Pane> {
    let mut dock = default_dock();
    set_visible(&mut dock, Pane::Log, false);
    dock
}

/// Whether `pane` is in the dock. A hidden panel is one that is not in the tree.
#[must_use]
pub fn is_visible(dock: &DockState<Pane>, pane: Pane) -> bool {
    dock.find_tab(&pane).is_some()
}

/// Shows or hides `pane`. A pane that comes back takes its default place.
///
/// The dock close button calls the same `remove_tab`, so a closed tab and a
/// hidden pane give the same tree.
pub fn set_visible(dock: &mut DockState<Pane>, pane: Pane, visible: bool) {
    match (visible, dock.find_tab(&pane)) {
        (false, Some(path)) => {
            dock.remove_tab(path);
        }
        (true, None) => show(dock.main_surface_mut(), pane),
        _ => {}
    }
}

/// The node that holds every pane but the Library, or `None` when the Library
/// is the only pane.
fn rest(tree: &Tree<Pane>) -> Option<NodeIndex> {
    let root = NodeIndex::root();
    match tree.find_tab(&Pane::Library) {
        Some((node, _)) if node == root => None,
        Some((node, _)) if node == root.left() && matches!(tree[root], Node::Horizontal(_)) => {
            Some(root.right())
        }
        _ => Some(root),
    }
}

/// Puts a hidden pane back so that every leaf keeps the width it has in the
/// default layout. Each split below repeats one split of `default_dock`.
fn show(tree: &mut Tree<Pane>, pane: Pane) {
    let root = NodeIndex::root();
    if tree.is_empty() {
        tree.push_to_first_leaf(pane);
        return;
    }
    let node_of = |tree: &Tree<Pane>, other| tree.find_tab(&other).map(|(node, _)| node);
    let (gallery, log) = (node_of(tree, Pane::Gallery), node_of(tree, Pane::Log));
    let tabs = vec![pane];
    match (pane, rest(tree)) {
        (Pane::Library, _) => tree.split_left(root, LIBRARY_SHARE, tabs),
        // Only the Library is shown: the new pane takes the rest.
        (_, None) => tree.split_right(root, LIBRARY_SHARE, tabs),
        (Pane::Preview, Some(rest)) => tree.split_right(rest, GALLERY_SHARE, tabs),
        // The Gallery goes above the Log, else it takes the middle slot.
        (Pane::Gallery, Some(rest)) => match log {
            Some(log) => tree.split_above(log, GALLERY_HEIGHT, tabs),
            None => tree.split_left(rest, GALLERY_SHARE, tabs),
        },
        (Pane::Log, Some(rest)) => match gallery {
            Some(gallery) => tree.split_below(gallery, GALLERY_HEIGHT, tabs),
            None => tree.split_left(rest, GALLERY_SHARE, tabs),
        },
    };
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
/// valid. Gives `None` for no panes, a pane twice, an empty leaf, or a floating window.
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
        Node::Leaf(_) | Node::Empty => true,
    });
    // The dock never keeps an empty leaf, so one in a saved layout is damage.
    let leaves_ok = dock
        .iter_all_nodes()
        .all(|(_, node)| !matches!(node, Node::Leaf(leaf) if leaf.is_empty()));
    // The panels never float, so a tab outside the main surface is damage.
    let docked = dock.iter_all_tabs().all(|(path, _)| path.surface.is_main());
    (!panes.is_empty() && once && fractions_ok && leaves_ok && docked).then_some(dock)
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
        let mut hollow = default_dock();
        let Node::Leaf(leaf) = &mut hollow.main_surface_mut()[NodeIndex::root().left()] else {
            panic!("the Library is a leaf");
        };
        leaf.tabs.clear();
        let mut floating = DockState::new(vec![Pane::Library, Pane::Gallery]);
        floating.add_window(vec![Pane::Log]);
        for bad in [
            to_json(&floating),
            to_json(&hollow),
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

    #[test]
    fn the_first_dock_is_the_default_without_the_log() {
        let dock = first_dock();
        assert!(!is_visible(&dock, Pane::Log));
        for pane in [Pane::Library, Pane::Gallery, Pane::Preview] {
            assert!(is_visible(&dock, pane));
        }
        let tree = dock.main_surface();
        assert_eq!(node_of(&dock, Pane::Library), NodeIndex::root().left());
        assert!(matches!(
            tree[NodeIndex::root().right()],
            Node::Horizontal(_)
        ));
    }

    /// The used nodes with their index: the tree without its trailing padding.
    fn shape(dock: &DockState<Pane>) -> Vec<String> {
        dock.main_surface()
            .iter()
            .enumerate()
            .filter_map(|(index, node)| match node {
                Node::Leaf(leaf) => Some(format!("{index} {:?}", leaf.tabs)),
                Node::Horizontal(split) | Node::Vertical(split) => {
                    Some(format!("{index} split {}", split.fraction))
                }
                Node::Empty => None,
            })
            .collect()
    }

    #[test]
    fn a_hidden_pane_returns_to_the_default_tree() {
        for start in [default_dock as fn() -> _, first_dock] {
            let before = shape(&start());
            for pane in ALL {
                let mut dock = start();
                if !is_visible(&dock, pane) {
                    continue;
                }
                set_visible(&mut dock, pane, false);
                assert!(!is_visible(&dock, pane));
                set_visible(&mut dock, pane, true);
                assert_eq!(shape(&dock), before, "{pane:?}");
            }
        }
    }

    #[test]
    fn a_layout_saved_after_hide_and_show_loads_back_the_same() {
        let mut dock = default_dock();
        set_visible(&mut dock, Pane::Library, false);
        set_visible(&mut dock, Pane::Log, false);
        set_visible(&mut dock, Pane::Log, true);
        let back = from_json(&to_json(&dock)).expect("the layout loads");
        assert_eq!(shape(&back), shape(&dock));
        assert!(!is_visible(&back, Pane::Library));
        assert!(is_visible(&back, Pane::Log));
    }

    #[test]
    fn two_hidden_panes_return_to_the_default_tree_in_either_order() {
        for order in [[Pane::Gallery, Pane::Log], [Pane::Log, Pane::Gallery]] {
            let mut dock = default_dock();
            set_visible(&mut dock, Pane::Gallery, false);
            set_visible(&mut dock, Pane::Log, false);
            for pane in order {
                set_visible(&mut dock, pane, true);
            }
            assert_eq!(shape(&dock), shape(&default_dock()), "{order:?}");
        }
    }

    #[test]
    fn the_log_takes_the_middle_slot_when_the_gallery_is_hidden() {
        let mut dock = first_dock();
        set_visible(&mut dock, Pane::Gallery, false);
        set_visible(&mut dock, Pane::Log, true);
        let rest = NodeIndex::root().right();
        assert_eq!(node_of(&dock, Pane::Log), rest.left());
        assert_eq!(node_of(&dock, Pane::Preview), rest.right());
    }

    #[test]
    fn a_pane_next_to_a_lone_library_takes_the_rest() {
        let mut dock = DockState::new(vec![Pane::Library]);
        set_visible(&mut dock, Pane::Gallery, true);
        assert_eq!(node_of(&dock, Pane::Gallery), NodeIndex::root().right());
        assert_eq!(node_of(&dock, Pane::Library), NodeIndex::root().left());
    }

    #[test]
    fn the_last_pane_can_hide_and_come_back() {
        let mut dock = DockState::new(vec![Pane::Gallery]);
        set_visible(&mut dock, Pane::Gallery, false);
        assert_eq!(dock.iter_all_tabs().count(), 0);
        set_visible(&mut dock, Pane::Gallery, true);
        assert!(is_visible(&dock, Pane::Gallery));
    }
}
