//! Builders that the Preview tests share.

use super::{ColorState, Meta, Preview, TagPill};
use egui::Pos2;
use egui::epaint::Shape;

/// A tag pill with a presence state.
pub(super) fn pill(name: &str, state: ColorState) -> TagPill {
    TagPill {
        name: name.into(),
        state,
    }
}

/// A Preview that shows one file and the given tags.
pub(super) fn preview(tags: Vec<TagPill>) -> Preview {
    let mut preview = Preview::default();
    preview.set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    preview.set_tags(tags);
    preview
}

/// Push every painted text and its position into `found`.
pub(super) fn collect(shape: &Shape, found: &mut Vec<(String, Pos2)>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, found)),
        Shape::Text(t) => found.push((t.galley.job.text.clone(), t.pos)),
        _ => {}
    }
}
