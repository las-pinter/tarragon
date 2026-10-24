//! Layer tree builder for the compositor.
//!
//! The flat layer record list marks groups with section dividers. This module
//! turns that list into a tree of pixel layers and groups, then attaches each
//! clipping layer to the base layer below it.

use crate::blend::BlendMode;
use crate::content::LayerContent;
use crate::effects::Effects;
use crate::error::PsdError;
use crate::layer::{Clipping, LayerBounds, LayerRecord};
use crate::limits::MAX_GROUP_DEPTH;

/// One node of the resolved layer tree.
pub(crate) struct LayerNode {
    /// The layer index for a pixel layer, or the close-record index for a group.
    pub(crate) index: Option<usize>,
    /// True for a group, false for a pixel layer.
    pub(crate) is_group: bool,
    /// The child nodes of a group.
    pub(crate) children: Vec<LayerNode>,
    /// The clipping layers that belong to this base layer.
    pub(crate) clips: Vec<LayerNode>,
    /// The blend mode.
    pub(crate) blend: BlendMode,
    /// The layer opacity in `0.0..=1.0`.
    pub(crate) opacity: f32,
    /// The fill opacity in `0.0..=1.0`.
    pub(crate) fill_opacity: f32,
    /// True when the layer is visible.
    pub(crate) visible: bool,
    /// True when the layer clips to the base below it.
    clipping: bool,
    /// The parsed layer content.
    pub(crate) content: LayerContent,
    /// The parsed layer effects.
    pub(crate) effects: Option<Effects>,
    /// The layer rectangle, or the union of the child rectangles for a group.
    pub(crate) bounds: LayerBounds,
}

impl LayerNode {
    /// Build a pixel-layer node from one record.
    fn pixel(index: usize, layer: &LayerRecord) -> Self {
        Self {
            index: Some(index),
            is_group: false,
            children: Vec::new(),
            clips: Vec::new(),
            blend: BlendMode::from_key(layer.blend_key),
            opacity: f32::from(layer.opacity) / 255.0,
            fill_opacity: f32::from(layer.fill_opacity) / 255.0,
            visible: layer.visible,
            clipping: layer.clipping == Clipping::NonBase,
            content: layer.content.clone(),
            effects: layer.effects.clone(),
            bounds: layer.bounds,
        }
    }

    /// Build a group node from its close record, children, and frame.
    fn group(
        index: Option<usize>,
        layer: &LayerRecord,
        children: Vec<LayerNode>,
        bounds: LayerBounds,
    ) -> Self {
        Self {
            index,
            is_group: true,
            children,
            clips: Vec::new(),
            blend: BlendMode::from_key(layer.blend_key),
            opacity: f32::from(layer.opacity) / 255.0,
            fill_opacity: f32::from(layer.fill_opacity) / 255.0,
            visible: layer.visible,
            clipping: layer.clipping == Clipping::NonBase,
            content: layer.content.clone(),
            effects: layer.effects.clone(),
            bounds,
        }
    }
}

/// Build the layer tree from the flat record list (bottom to top).
///
/// A bounding divider opens a group, an open or closed folder closes it, and
/// the close record carries the group settings. Consecutive clipping layers
/// attach to the base below them.
///
/// # Errors
///
/// Returns [`PsdError::GroupsTooDeep`] when groups nest deeper than
/// [`MAX_GROUP_DEPTH`]. The cap also bounds the recursion of the compositor
/// walk and of the drop of the tree.
pub(crate) fn build_tree(layers: &[LayerRecord]) -> Result<Vec<LayerNode>, PsdError> {
    let mut cursor = 0;
    let (nodes, _) = parse_level(layers, &mut cursor, 0)?;
    Ok(attach_clips(nodes))
}

/// Parse one tree level. Returns the nodes and the close-record index.
fn parse_level(
    layers: &[LayerRecord],
    cursor: &mut usize,
    depth: usize,
) -> Result<(Vec<LayerNode>, Option<usize>), PsdError> {
    let mut nodes = Vec::new();
    while *cursor < layers.len() {
        let index = *cursor;
        let layer = &layers[index];
        match layer.section {
            Some(section) if section.opens_group() => {
                if depth >= MAX_GROUP_DEPTH {
                    return Err(PsdError::GroupsTooDeep {
                        limit: MAX_GROUP_DEPTH,
                    });
                }
                let divider_bounds = layer.bounds;
                *cursor += 1;
                let (children, close_index) = parse_level(layers, cursor, depth + 1)?;
                let group_layer = close_index.map_or(layer, |close| &layers[close]);
                let children = attach_clips(children);
                let bounds = group_bounds(&children).unwrap_or(divider_bounds);
                nodes.push(LayerNode::group(close_index, group_layer, children, bounds));
                continue;
            }
            Some(section) if section.closes_group() => {
                *cursor += 1;
                return Ok((nodes, Some(index)));
            }
            _ => {}
        }
        *cursor += 1;
        nodes.push(LayerNode::pixel(index, layer));
    }
    Ok((nodes, None))
}

/// The deepest group nesting in a tree. A tree without groups has depth zero.
///
/// The recursion is bounded by [`MAX_GROUP_DEPTH`], which `build_tree` enforces.
pub(crate) fn group_depth(nodes: &[LayerNode]) -> usize {
    nodes
        .iter()
        .flat_map(|node| std::iter::once(node).chain(&node.clips))
        .map(|node| {
            if node.is_group {
                1 + group_depth(&node.children)
            } else {
                0
            }
        })
        .max()
        .unwrap_or(0)
}

/// The effects load of a tree, which sizes the memory budget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct EffectsLoad {
    /// The most nodes with active effects on one root-to-leaf path. Each such
    /// node pads the viewport again inside the padded viewport of its parent.
    pub(crate) padded_levels: usize,
    /// The most paints one node holds at once.
    pub(crate) paints: usize,
}

/// Measure the effects load of a tree.
///
/// The children of a group and the clips of a base layer both build inside the
/// padded viewport of that node, so both count as nested. The recursion is
/// bounded by [`MAX_GROUP_DEPTH`], which `build_tree` enforces.
///
/// Hidden nodes count too, although the compositor skips them. The over-count
/// is deliberate and safe: it can only make the estimate larger.
pub(crate) fn effects_load(nodes: &[LayerNode]) -> EffectsLoad {
    let mut load = EffectsLoad::default();
    for node in nodes {
        let active = node.effects.as_ref().filter(|effects| effects.is_active());
        let below = effects_load(&node.children);
        let clipped = effects_load(&node.clips);
        let nested = below.padded_levels.max(clipped.padded_levels);
        let own = usize::from(active.is_some());
        load.padded_levels = load.padded_levels.max(own + nested);
        let held = active.map_or(0, Effects::held_paints);
        load.paints = load.paints.max(held).max(below.paints).max(clipped.paints);
    }
    load
}

/// Attach consecutive clipping layers to the base below them.
fn attach_clips(nodes: Vec<LayerNode>) -> Vec<LayerNode> {
    let mut result: Vec<LayerNode> = Vec::new();
    let mut base: Option<LayerNode> = None;
    for node in nodes {
        if node.clipping {
            match base.as_mut() {
                Some(base) => base.clips.push(node),
                None => result.push(node),
            }
        } else {
            if let Some(previous) = base.take() {
                result.push(previous);
            }
            base = Some(node);
        }
    }
    if let Some(previous) = base {
        result.push(previous);
    }
    result
}

/// The union of the child rectangles, or `None` when no child has area.
fn group_bounds(children: &[LayerNode]) -> Option<LayerBounds> {
    let mut bounds: Option<LayerBounds> = None;
    for child in children {
        for node in std::iter::once(child).chain(&child.clips) {
            let rect = node.bounds;
            if rect.width() <= 0 || rect.height() <= 0 {
                continue;
            }
            bounds = Some(match bounds {
                Some(current) => LayerBounds {
                    top: current.top.min(rect.top),
                    left: current.left.min(rect.left),
                    bottom: current.bottom.max(rect.bottom),
                    right: current.right.max(rect.right),
                },
                None => rect,
            });
        }
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::{build_tree, effects_load, group_depth};
    use crate::blend::BlendMode;
    use crate::error::PsdError;
    use crate::layer::{BlendingRanges, Clipping, LayerBounds, LayerRecord, LayerSection};
    use crate::limits::MAX_GROUP_DEPTH;

    /// A minimal layer record for tree tests.
    fn record(section: Option<LayerSection>, clipping: Clipping) -> LayerRecord {
        LayerRecord {
            name: String::new(),
            bounds: LayerBounds {
                top: 0,
                left: 0,
                bottom: 1,
                right: 1,
            },
            channels: Vec::new(),
            blend_key: *b"norm",
            opacity: 255,
            fill_opacity: 255,
            clipping,
            flags: 0,
            visible: true,
            mask: None,
            blending_ranges: BlendingRanges {
                composite: [0; 4],
                channels: Vec::new(),
            },
            section,
            content: crate::content::LayerContent::Pixels,
            has_vector_mask: false,
            effects: None,
        }
    }

    #[test]
    fn a_group_marker_wraps_its_children() {
        let layers = vec![
            record(None, Clipping::Base),
            record(Some(LayerSection::BoundingSectionDivider), Clipping::Base),
            record(None, Clipping::Base),
            record(Some(LayerSection::OpenFolder), Clipping::Base),
        ];
        let tree = build_tree(&layers).expect("the tree builds");
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].index, Some(0));
        assert!(tree[1].is_group);
        assert_eq!(tree[1].children.len(), 1);
        assert_eq!(tree[1].blend, BlendMode::Normal);
    }

    #[test]
    fn groups_deeper_than_the_cap_are_an_error() {
        let nest = |depth: usize| {
            let mut layers = Vec::new();
            for _ in 0..depth {
                layers.push(record(
                    Some(LayerSection::BoundingSectionDivider),
                    Clipping::Base,
                ));
            }
            for _ in 0..depth {
                layers.push(record(Some(LayerSection::OpenFolder), Clipping::Base));
            }
            layers
        };
        assert!(build_tree(&nest(MAX_GROUP_DEPTH)).is_ok());
        assert!(matches!(
            build_tree(&nest(MAX_GROUP_DEPTH + 1)),
            Err(PsdError::GroupsTooDeep { .. })
        ));
        assert!(build_tree(&nest(30_000)).is_err());
    }

    #[test]
    fn the_group_depth_counts_nested_groups() {
        let layers = vec![
            record(Some(LayerSection::BoundingSectionDivider), Clipping::Base),
            record(Some(LayerSection::BoundingSectionDivider), Clipping::Base),
            record(None, Clipping::Base),
            record(Some(LayerSection::OpenFolder), Clipping::Base),
            record(Some(LayerSection::OpenFolder), Clipping::Base),
            record(None, Clipping::Base),
        ];
        let tree = build_tree(&layers).expect("the tree builds");
        assert_eq!(group_depth(&tree), 2);
        assert_eq!(group_depth(&[]), 0);
    }

    #[test]
    fn a_clipping_layer_attaches_to_the_base_below() {
        let layers = vec![
            record(None, Clipping::Base),
            record(None, Clipping::NonBase),
            record(None, Clipping::NonBase),
        ];
        let tree = build_tree(&layers).expect("the tree builds");
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].clips.len(), 2);
    }

    fn with_overlay(mut layer: LayerRecord) -> LayerRecord {
        use crate::effects::{Effect, EffectCommon, Effects, Overlay};
        layer.effects = Some(Effects {
            master: true,
            items: vec![Effect::ColorOverlay(Overlay {
                common: EffectCommon {
                    enabled: true,
                    blend: BlendMode::Normal,
                    opacity: 1.0,
                },
                color: [1.0; 3],
            })],
        });
        layer
    }

    #[test]
    fn nested_effects_count_once_per_level_on_a_path() {
        let layers = vec![
            record(None, Clipping::Base),
            with_overlay(record(None, Clipping::Base)),
            record(Some(LayerSection::BoundingSectionDivider), Clipping::Base),
            with_overlay(record(None, Clipping::Base)),
            with_overlay(record(Some(LayerSection::OpenFolder), Clipping::Base)),
        ];
        let tree = build_tree(&layers).expect("the tree builds");
        assert_eq!(effects_load(&tree).padded_levels, 2);
        assert_eq!(effects_load(&tree).paints, 0);
        assert_eq!(effects_load(&[]).padded_levels, 0);
    }

    #[test]
    fn siblings_with_effects_do_not_nest() {
        let layers = vec![
            with_overlay(record(None, Clipping::Base)),
            with_overlay(record(None, Clipping::Base)),
        ];
        let tree = build_tree(&layers).expect("the tree builds");
        assert_eq!(effects_load(&tree).padded_levels, 1);
    }

    #[test]
    fn a_clip_layer_with_effects_nests_in_its_base() {
        let layers = vec![
            with_overlay(record(None, Clipping::Base)),
            with_overlay(record(None, Clipping::NonBase)),
        ];
        let tree = build_tree(&layers).expect("the tree builds");
        assert_eq!(effects_load(&tree).padded_levels, 2);
    }

    fn with_items(mut layer: LayerRecord, items: Vec<crate::effects::Effect>) -> LayerRecord {
        layer.effects = Some(crate::effects::Effects {
            master: true,
            items,
        });
        layer
    }

    fn outer_bevel() -> crate::effects::Effect {
        use crate::effects::{Bevel, BevelStyle, Contour, Effect, EffectCommon};
        Effect::Bevel(Bevel {
            enabled: true,
            style: BevelStyle::Outer,
            depth: 1.0,
            size: 4.0,
            soften: 0.0,
            angle: 120.0,
            altitude: 30.0,
            use_global_light: false,
            up: true,
            highlight: EffectCommon::default(),
            highlight_color: [1.0; 3],
            shadow: EffectCommon::default(),
            shadow_color: [0.0; 3],
            gloss: Contour::default(),
        })
    }

    fn drop_shadow() -> crate::effects::Effect {
        use crate::effects::{Contour, Effect, EffectCommon, Shadow};
        Effect::DropShadow(Shadow {
            common: EffectCommon::default(),
            color: [0.0; 3],
            angle: 120.0,
            distance: 5.0,
            size: 4.0,
            spread: 0.0,
            use_global_light: false,
            knocks_out: false,
            contour: Contour::default(),
        })
    }

    #[test]
    fn the_paint_maximum_comes_from_a_nested_group_child() {
        let layers = vec![
            record(Some(LayerSection::BoundingSectionDivider), Clipping::Base),
            with_items(record(None, Clipping::Base), vec![outer_bevel()]),
            with_items(
                record(Some(LayerSection::OpenFolder), Clipping::Base),
                vec![drop_shadow()],
            ),
        ];
        let tree = build_tree(&layers).expect("the tree builds");
        let load = effects_load(&tree);
        assert_eq!(load.paints, 2);
        assert_eq!(load.padded_levels, 2);
    }

    #[test]
    fn the_paint_maximum_comes_from_a_clip_layer() {
        let layers = vec![
            with_items(record(None, Clipping::Base), vec![drop_shadow()]),
            with_items(record(None, Clipping::NonBase), vec![drop_shadow()]),
        ];
        let tree = build_tree(&layers).expect("the tree builds");
        assert_eq!(effects_load(&tree).paints, 1);
        let beveled = vec![
            with_items(record(None, Clipping::Base), vec![drop_shadow()]),
            with_items(record(None, Clipping::NonBase), vec![outer_bevel()]),
        ];
        let tree = build_tree(&beveled).expect("the tree builds");
        assert_eq!(effects_load(&tree).paints, 2);
    }
}
