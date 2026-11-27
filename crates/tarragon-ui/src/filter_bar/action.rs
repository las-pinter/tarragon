//! The text actions at the end of the filter bar: "Tags", "Folders", "Clear".
//!
//! The sizes follow the filter row of design/redesign/main-window.html.

use egui::{
    Color32, CornerRadius, CursorIcon, FontFamily, FontId, Galley, Response, Sense, Stroke,
    StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2,
};
use std::sync::Arc;

use crate::{fonts, tokens};

/// The text of the action that opens the tag menu.
pub const TAGS_TEXT: &str = "Tags";
/// The text of the action that opens the folder menu.
pub const FOLDERS_TEXT: &str = "Folders";
/// The text of the action that drops every filter of the bar.
pub const CLEAR_TEXT: &str = "Clear";

/// The side padding of an action, in points.
const PAD_X: f32 = 4.0;
/// The height of an action, in points: the height of a chip.
pub(super) const ACTION_HEIGHT: f32 = 28.0;

/// How an action looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// A coral medium action that opens a menu.
    Menu,
    /// The quiet tertiary "Clear".
    Quiet,
}

impl Kind {
    /// The text colour; both pass on the panel fill and the hover step.
    const fn color(self) -> Color32 {
        match self {
            Self::Menu => tokens::CORAL_STRONG,
            Self::Quiet => tokens::TEXT_TERTIARY,
        }
    }

    /// The text font: medium for a menu action, regular for "Clear".
    fn font(self) -> FontId {
        let family = match self {
            Self::Menu => FontFamily::Name(fonts::MEDIUM_FAMILY.into()),
            Self::Quiet => FontFamily::Proportional,
        };
        FontId::new(tokens::SIZE_BODY, family)
    }
}

/// Lay out the action text.
fn layout(ui: &Ui, text: &str, kind: Kind) -> Arc<Galley> {
    ui.painter()
        .layout_no_wrap(text.to_owned(), kind.font(), kind.color())
}

/// The width an action takes, in points.
pub(super) fn action_width(ui: &Ui, text: &str, kind: Kind) -> f32 {
    layout(ui, text, kind).size().x + 2.0 * PAD_X
}

/// Draw one text action and return its response.
///
/// The hover step fills it under the pointer and while it has focus, and a
/// coral ring shows the keyboard focus, as on the sidebar add action.
pub(super) fn text_action(ui: &mut Ui, text: &str, kind: Kind) -> Response {
    let galley = layout(ui, text, kind);
    let size = vec2(galley.size().x + 2.0 * PAD_X, ACTION_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), text));
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    let radius = CornerRadius::same(tokens::RADIUS_M);
    if response.hovered() || response.has_focus() {
        painter.rect_filled(rect, radius, tokens::SURFACE_HOVER);
    }
    if response.has_focus() {
        let ring = Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
        painter.rect_stroke(rect, radius, ring, StrokeKind::Inside);
    }
    let y = rect.center().y - galley.size().y / 2.0;
    painter.galley(pos2(rect.min.x + PAD_X, y), galley, kind.color());
    response
}

#[cfg(test)]
mod tests;
