//! The Library panel chrome: the section heads and the add action.

use egui::{
    Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Id, Margin, Response, RichText,
    Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::sidebar::row::ROW_HEIGHT;
use crate::{fonts, tokens};

/// The side padding of the panel, in points.
pub(super) const SIDE_PAD: i8 = 12;
/// The padding at the top and bottom edge of the panel, in points.
pub(super) const EDGE_PAD: i8 = 16;
/// The gap between two rows, in points.
pub(super) const ROW_GAP: i8 = 4;
/// The room above the Folders head, in points.
pub(super) const FOLDERS_HEAD_TOP: i8 = 4;
/// The room above the Favorites head, in points.
pub(super) const FAVORITES_HEAD_TOP: i8 = 20;
/// The room between the last favorite and the add action, in points.
pub(super) const ADD_GAP: i8 = 8;
/// The room below a head, in points.
const HEAD_BOTTOM: i8 = 8;
/// The side padding of a head, inside the panel padding, in points.
const HEAD_SIDE: i8 = 8;
/// The side padding of the add action, in points.
const ADD_PAD: f32 = 8.0;
/// The gap between the plus and the text of the add action, in points.
const ADD_ICON_GAP: f32 = 8.0;
/// The plus square of the add action, in points.
const PLUS_SIZE: f32 = 16.0;
/// The plus stroke width, in points.
const PLUS_STROKE: f32 = 1.6;
/// The id salt of the add action, so a test can focus it.
pub(super) const ADD_ID: &str = "tarragon-sidebar-add";
/// The text of the add action.
pub const ADD_TEXT: &str = "Add current folder";

/// Draw a section head: a small upper-case label with `top` points above it.
///
/// It matches the dock tab titles: caption size, the medium face, wide
/// letter spacing, and the secondary text color.
pub(super) fn head(ui: &mut Ui, text: &str, top: i8) {
    Frame::new()
        .inner_margin(Margin {
            left: SIDE_PAD + HEAD_SIDE,
            right: SIDE_PAD + HEAD_SIDE,
            top,
            // The row gap follows the head, so the first row sits
            // `HEAD_BOTTOM` below the label.
            bottom: HEAD_BOTTOM - ROW_GAP,
        })
        .show(ui, |ui| {
            ui.label(
                RichText::new(text.to_uppercase())
                    .size(tokens::SIZE_CAPTION)
                    .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
                    .extra_letter_spacing(tokens::SIZE_CAPTION * tokens::LABEL_SPACING_EM)
                    .color(tokens::TEXT_SECONDARY),
            );
        });
}

/// Draw the add action: a coral plus and text with no fill at rest and the
/// hover fill under the pointer or focus. It takes keyboard focus like a
/// button, shows the coral focus ring while focused, and reports its text to
/// screen readers.
pub(super) fn add_action(ui: &mut Ui) -> Response {
    let font = FontId::new(
        tokens::SIZE_BODY,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into()),
    );
    let galley = ui
        .painter()
        .layout_no_wrap(ADD_TEXT.to_owned(), font, tokens::CORAL_STRONG);
    let width = 2.0 * ADD_PAD + PLUS_SIZE + ADD_ICON_GAP + galley.size().x;
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let response = ui.interact(rect, Id::new(ADD_ID), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), ADD_TEXT));
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    let fill = if response.hovered() || response.has_focus() {
        tokens::SURFACE_HOVER
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(tokens::RADIUS_M), fill);
    // egui lets a focused button act on Space or Enter, so the focus shows.
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            CornerRadius::same(tokens::RADIUS_M),
            Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG),
            StrokeKind::Inside,
        );
    }
    let plus = egui::Rect::from_min_size(
        pos2(
            rect.min.x + ADD_PAD,
            (rect.center().y - PLUS_SIZE / 2.0).round(),
        ),
        vec2(PLUS_SIZE, PLUS_SIZE),
    );
    // The page draws the plus from 3 to 13 on its 16 unit square.
    let stroke = Stroke::new(PLUS_STROKE, tokens::CORAL_STRONG);
    let at = |x: f32, y: f32| plus.min + vec2(x, y);
    painter.line_segment([at(8.0, 3.0), at(8.0, 13.0)], stroke);
    painter.line_segment([at(3.0, 8.0), at(13.0, 8.0)], stroke);
    let text_y = rect.center().y - galley.size().y / 2.0;
    painter.galley(
        pos2(plus.max.x + ADD_ICON_GAP, text_y),
        galley,
        tokens::CORAL_STRONG,
    );
    response
}
