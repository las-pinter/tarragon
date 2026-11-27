//! The body of the Preferences window: the title row, the group list beside
//! the shown group, and the footer with Cancel and Save.
//!
//! The sizes follow design/redesign/settings.html.

use egui::{
    Align, CornerRadius, CursorIcon, FontFamily, Frame, Label, Layout, Margin, Rect, RichText,
    ScrollArea, Sense, Stroke, Ui, UiBuilder, Vec2, WidgetInfo, WidgetType, vec2,
};

use super::draft::Draft;
use super::groups;
use super::nav::{self, Group};
use crate::{fonts, tokens, widgets};

/// The height of the title row, in points.
const TITLE_HEIGHT: f32 = 64.0;
/// The height of the footer, in points: the 36 pt buttons and 14 pt above and
/// below.
const FOOTER_HEIGHT: f32 = 64.0;
/// The side padding of the title row and the footer, in points.
const EDGE: f32 = 20.0;
/// The size of the close button, in points.
const CLOSE_SIZE: f32 = 32.0;
/// The close mark is drawn on a 14 unit square.
const CLOSE_GRID: f32 = 14.0;
/// The padding inside the group page: top, sides, bottom.
const PAGE_PAD: (i8, i8, i8) = (24, 28, 24);
/// The gap between the two footer buttons, in points.
const BUTTON_GAP: f32 = 8.0;
/// The screen reader label of the close button.
pub(super) const CLOSE_LABEL: &str = "Close";
/// The text of the button that saves.
pub(super) const SAVE_TEXT: &str = "Save";

/// A button the user pressed.
pub(super) enum Action {
    Save,
    Cancel,
    Purge,
    Browse,
}

/// What the app sets from outside the form.
pub(super) struct View<'a> {
    pub cache_size: &'a str,
    pub purging: bool,
    pub error: Option<&'a str>,
}

/// Draw the window body in a `size` area and return the button the user
/// pressed, if any.
pub(super) fn contents(
    ui: &mut Ui,
    size: Vec2,
    draft: &mut Draft,
    view: &View<'_>,
    group: &mut Group,
) -> Option<Action> {
    let mut action = None;
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let origin = ui.cursor().min;
    let title = Rect::from_min_size(origin, vec2(size.x, TITLE_HEIGHT));
    let footer = Rect::from_min_size(
        origin + vec2(0.0, size.y - FOOTER_HEIGHT),
        vec2(size.x, FOOTER_HEIGHT),
    );
    let body = Rect::from_min_max(title.left_bottom(), footer.right_top());
    ui.allocate_rect(Rect::from_min_size(origin, size), Sense::hover());

    let mut title_ui = ui.new_child(UiBuilder::new().max_rect(title));
    if title_row(&mut title_ui, view.purging) {
        action = Some(Action::Cancel);
    }

    let nav_rect = Rect::from_min_size(body.min, vec2(nav::WIDTH, body.height()));
    let mut nav_ui = ui.new_child(UiBuilder::new().max_rect(nav_rect));
    nav::show(&mut nav_ui, group, |g| groups::invalid(draft, g));

    let page = Rect::from_min_max(nav_rect.right_top(), body.max);
    let radius = CornerRadius {
        nw: tokens::RADIUS_XL,
        ..CornerRadius::ZERO
    };
    ui.painter().rect_filled(page, radius, tokens::BG_PRIMARY);
    let mut page_ui = ui.new_child(UiBuilder::new().max_rect(page));
    let shown = *group;
    ScrollArea::vertical()
        .id_salt("prefs_page")
        .auto_shrink([false, false])
        .show(&mut page_ui, |ui| {
            let (top, across, bottom) = PAGE_PAD;
            Frame::new()
                .inner_margin(Margin {
                    left: across,
                    right: across,
                    top,
                    bottom,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    groups::show(ui, shown, draft, view, &mut action);
                });
        });

    let mut footer_ui = ui.new_child(UiBuilder::new().max_rect(footer));
    if let Some(pressed) = footer_row(&mut footer_ui, draft, view) {
        action = Some(pressed);
    }
    action
}

/// The title and the close button. Returns whether the close button was
/// pressed; it waits like Cancel while a purge runs.
fn title_row(ui: &mut Ui, purging: bool) -> bool {
    let mut closed = false;
    let rect = ui.max_rect();
    ui.allocate_ui_with_layout(rect.size(), Layout::left_to_right(Align::Center), |ui| {
        ui.add_space(EDGE);
        let title = RichText::new("Preferences")
            .size(tokens::SIZE_HEADING)
            .family(FontFamily::Name(fonts::BOLD_FAMILY.into()))
            .color(tokens::TEXT_PRIMARY);
        ui.label(title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(EDGE);
            ui.add_enabled_ui(!purging, |ui| {
                closed = close_button(ui).clicked();
            });
        });
    });
    closed
}

/// A 32 pt button with a cross, lifted to the hover fill under the pointer.
fn close_button(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CLOSE_SIZE), Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, CLOSE_LABEL));
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    let radius = CornerRadius::same(tokens::RADIUS_L);
    if enabled && response.hovered() {
        painter.rect_filled(rect, radius, tokens::SURFACE_HOVER);
    }
    if response.has_focus() {
        let ring = Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
        painter.rect_stroke(rect, radius, ring, egui::StrokeKind::Inside);
    }
    let color = if enabled {
        tokens::TEXT_TERTIARY
    } else {
        tokens::TEXT_SECONDARY
    };
    let origin = rect.center() - Vec2::splat(CLOSE_GRID / 2.0);
    let at = |x: f32, y: f32| origin + vec2(x, y);
    let stroke = Stroke::new(1.6, color);
    painter.line_segment([at(3.5, 3.5), at(10.5, 10.5)], stroke);
    painter.line_segment([at(10.5, 3.5), at(3.5, 10.5)], stroke);
    response
}

/// The error at the left, then Cancel and Save at the right.
fn footer_row(ui: &mut Ui, draft: &Draft, view: &View<'_>) -> Option<Action> {
    let mut action = None;
    let rect = ui.max_rect();
    ui.allocate_ui_with_layout(rect.size(), Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        ui.add_space(EDGE);
        let enabled = !view.purging;
        // The layout runs right to left, so Save comes first to sit right of
        // Cancel.
        ui.add_enabled_ui(enabled && draft.parse().is_some(), |ui| {
            if widgets::primary_button(ui, SAVE_TEXT).clicked() {
                action = Some(Action::Save);
            }
        });
        ui.add_enabled_ui(enabled, |ui| {
            if widgets::secondary_button(ui, "Cancel").clicked() {
                action = Some(Action::Cancel);
            }
        });
        if let Some(error) = view.error {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(EDGE);
                let text = RichText::new(error).color(tokens::CORAL_STRONG);
                ui.add(Label::new(text).truncate());
            });
        }
    });
    action
}
