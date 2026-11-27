//! The row under the picture: the file name over its details line, then the
//! open button and the caret that lists the other apps.
//!
//! The sizes follow the Preview of design/redesign/main-window.html.

use egui::{
    Align, CornerRadius, CursorIcon, FontFamily, Label, Layout, Popup, RichText, Sense, Stroke, Ui,
    Vec2, WidgetInfo, WidgetType, vec2,
};

use super::{Meta, PreviewEvent};
use crate::{fonts, tokens, widgets};

/// The height of the row and of both buttons, in points.
pub const ROW_HEIGHT: f32 = 34.0;
/// The gap between the text and the buttons, in points.
const GAP: f32 = 12.0;
/// The gap between the open button and the caret, in points.
const BUTTON_GAP: f32 = 8.0;
/// The caret is drawn on a 14 unit square.
const CARET_GRID: f32 = 14.0;
/// The stroke of the caret, in points.
const CARET_STROKE: f32 = 1.6;
/// The screen reader label of the caret button.
pub const CARET_LABEL: &str = "Open with another app";

/// What the open button shows: its text and the menu rows of the caret.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenButton {
    /// The button text, such as "Open in Krita".
    pub label: String,
    /// The apps the caret lists, the button's own first.
    pub choices: Vec<String>,
}

/// Draw the row. The buttons show only with `open`.
pub fn show(ui: &mut Ui, meta: &Meta, open: Option<&OpenButton>, events: &mut Vec<PreviewEvent>) {
    let size = vec2(ui.available_width(), ROW_HEIGHT);
    ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        if let Some(open) = open {
            show_caret(ui, open, events);
            ui.scope(|ui| {
                ui.spacing_mut().interact_size.y = ROW_HEIGHT;
                if widgets::primary_button(ui, &open.label).clicked() {
                    events.push(PreviewEvent::Open);
                }
            });
            ui.add_space(GAP - BUTTON_GAP);
        }
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let name = RichText::new(&meta.name)
                .size(tokens::SIZE_TITLE)
                .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
                .color(tokens::TEXT_PRIMARY);
            ui.add(Label::new(name).truncate());
            let line = RichText::new(meta.line())
                .size(tokens::SIZE_BODY)
                .color(tokens::TEXT_SECONDARY);
            ui.add(Label::new(line).truncate());
        });
    });
}

/// The square caret button and its menu. A row click queues the app index.
fn show_caret(ui: &mut Ui, open: &OpenButton, events: &mut Vec<PreviewEvent>) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ROW_HEIGHT), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, CARET_LABEL));
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    let fill = if response.hovered() || response.has_focus() {
        tokens::SURFACE_HIGHLIGHT
    } else {
        tokens::SURFACE_HOVER
    };
    let radius = CornerRadius::same(tokens::RADIUS_L);
    painter.rect_filled(rect, radius, fill);
    if response.has_focus() {
        let ring = Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
        painter.rect_stroke(rect, radius, ring, egui::StrokeKind::Inside);
    }
    let origin = rect.center() - Vec2::splat(CARET_GRID / 2.0);
    let at = |x: f32, y: f32| origin + vec2(x, y);
    painter.add(egui::epaint::PathShape::line(
        vec![at(3.5, 5.5), at(7.0, 9.0), at(10.5, 5.5)],
        Stroke::new(CARET_STROKE, tokens::TEXT_PRIMARY),
    ));
    Popup::menu(&response).show(|ui| {
        for (index, choice) in open.choices.iter().enumerate() {
            if ui.button(choice).clicked() {
                events.push(PreviewEvent::OpenWith(index));
            }
        }
    });
}

#[cfg(test)]
mod tests;
