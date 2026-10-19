//! Layer tree builder for the compositor.
//!
//! The flat layer record list marks groups with section dividers. This module
//! turns that list into a tree of pixel layers and groups, then attaches each
//! clipping layer to the base layer below it.

use crate::blend::BlendMode;
use crate::content::LayerContent;
use crate::effects::Effects;
use crate::layer::{Clipping, LayerBounds, LayerRecord};

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
pub(crate) fn build_tree(layers: &[LayerRecord]) -> Vec<LayerNode> {
    let mut cursor = 0;
    let (nodes, _) = parse_level(layers, &mut cursor);
    attach_clips(nodes)
}

/// Parse one tree level. Returns the nodes and the close-record index.
fn parse_level(layers: &[LayerRecord], cursor: &mut usize) -> (Vec<LayerNode>, Option<usize>) {
    let mut nodes = Vec::new();
    while *cursor < layers.len() {
        let index = *cursor;
        let layer = &layers[index];
        match layer.section {
            Some(section) if section.opens_group() => {
                let divider_bounds = layer.bounds;
                *cursor += 1;
                let (children, close_index) = parse_level(layers, cursor);
                let group_layer = close_index.map_or(layer, |close| &layers[close]);
                let children = attach_clips(children);
                let bounds = group_bounds(&children).unwrap_or(divider_bounds);
                nodes.push(LayerNode::group(close_index, group_layer, children, bounds));
                continue;
            }
            Some(section) if section.closes_group() => {
                *cursor += 1;
                return (nodes, Some(index));
            }
            _ => {}
        }
        *cursor += 1;
        nodes.push(LayerNode::pixel(index, layer));
    }
    (nodes, None)
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
    use super::build_tree;
    use crate::blend::BlendMode;
    use crate::layer::{BlendingRanges, Clipping, LayerBounds, LayerRecord, LayerSection};

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
        let tree = build_tree(&layers);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].index, Some(0));
        assert!(tree[1].is_group);
        assert_eq!(tree[1].children.len(), 1);
        assert_eq!(tree[1].blend, BlendMode::Normal);
    }

    #[test]
    fn a_clipping_layer_attaches_to_the_base_below() {
        let layers = vec![
            record(None, Clipping::Base),
            record(None, Clipping::NonBase),
            record(None, Clipping::NonBase),
        ];
        let tree = build_tree(&layers);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].clips.len(), 2);
    }
}
