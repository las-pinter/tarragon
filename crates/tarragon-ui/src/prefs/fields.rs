//! The inputs of the Preferences groups: a label over its field, two fields
//! side by side, the toggle rows, and the threshold slider.

use egui::{
    Align, ComboBox, CornerRadius, EventFilter, Key, Label, Layout, Margin, Modifiers, RichText,
    Sense, Stroke, StrokeKind, TextEdit, Ui, WidgetInfo, vec2,
};

use super::draft::Draft;
use crate::{tokens, widgets};

/// The height of a text or choice field, in points.
pub(super) const FIELD_HEIGHT: f32 = 36.0;
/// The gap between a label and its field, in points.
pub(super) const LABEL_GAP: f32 = 6.0;
/// The gap between two fields side by side, in points.
const COLUMN_GAP: f32 = 16.0;
/// The gap between a toggle and its label, in points.
const TOGGLE_GAP: f32 = 12.0;
/// The side padding inside a field, in points.
pub(super) const FIELD_PAD: i8 = 12;
/// The height of the slider, in points.
const SLIDER_HEIGHT: f32 = 20.0;
/// The height of the slider rail, in points.
const RAIL_HEIGHT: f32 = 4.0;
/// The diameter of the slider knob, in points.
const KNOB: f32 = 16.0;
/// How far one arrow key moves the slider.
pub(super) const STEP: f32 = 0.01;

/// Two inputs side by side, each half the width less the gap. `add` draws
/// the input of column 0, then of column 1.
pub(super) fn pair(ui: &mut Ui, mut add: impl FnMut(&mut Ui, usize)) {
    let width = ((ui.available_width() - COLUMN_GAP) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = COLUMN_GAP;
        for column in 0..2 {
            ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                add(ui, column);
            });
        }
    });
}

/// A label in `color` over the field that `add` draws.
pub(super) fn labelled(ui: &mut Ui, label: &str, color: egui::Color32, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = LABEL_GAP;
        ui.label(RichText::new(label).size(tokens::SIZE_BODY).color(color));
        add(ui);
    });
}

/// A text field for the number `index` of `draft`. A number that does not
/// parse turns its label and its text coral.
pub(super) fn number(ui: &mut Ui, draft: &mut Draft, index: usize, label: &str) {
    let valid = draft.valid(index);
    let (label_color, text_color) = if valid {
        (tokens::TEXT_TERTIARY, tokens::TEXT_PRIMARY)
    } else {
        (tokens::CORAL_STRONG, tokens::CORAL_STRONG)
    };
    labelled(ui, label, label_color, |ui| {
        let width = ui.available_width();
        let edit = TextEdit::singleline(&mut draft.numbers[index])
            .text_color(text_color)
            .margin(Margin::symmetric(FIELD_PAD, 0))
            .vertical_align(Align::Center)
            .min_size(vec2(width, FIELD_HEIGHT))
            .desired_width(f32::INFINITY);
        ui.add(edit);
    });
}

/// A choice field: the current choice, and a menu of `choices`.
pub(super) fn choice(ui: &mut Ui, id: &str, label: &str, current: &mut String, choices: &[&str]) {
    labelled(ui, label, tokens::TEXT_TERTIARY, |ui| {
        let width = ui.available_width();
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = FIELD_HEIGHT;
            ComboBox::from_id_salt(id)
                .selected_text(current.as_str())
                .width(width)
                .show_ui(ui, |ui| {
                    for &choice in choices {
                        ui.selectable_value(current, choice.to_owned(), choice);
                    }
                });
        });
    });
}

/// A toggle with its label at the right. A click on the label flips it too.
pub(super) fn toggle_row(ui: &mut Ui, label: &str, on: &mut bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = TOGGLE_GAP;
        widgets::toggle(ui, on, label);
        let text =
            Label::new(RichText::new(label).color(tokens::TEXT_PRIMARY)).sense(Sense::click());
        if ui.add(text).clicked() {
            *on = !*on;
        }
    });
}

/// The value of the slider number `index`, held to 0 to 1. Text that does not
/// parse reads as 0.
pub(super) fn slider_value(draft: &Draft, index: usize) -> f32 {
    draft.numbers[index]
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0)
}

/// The page's slider for the number `index` of `draft`, from 0 to 1: the label
/// and the value over a rail with a coral fill up to the knob. A click or a
/// drag sets the value, and the arrow keys step it while it has the focus.
pub(super) fn threshold(ui: &mut Ui, draft: &mut Draft, index: usize, label: &str) {
    let value = slider_value(draft, index);
    let row = vec2(ui.available_width(), 0.0);
    ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
        ui.label(RichText::new(label).color(tokens::TEXT_TERTIARY));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format!("{value:.2}")).color(tokens::TEXT_PRIMARY));
        });
    });
    let size = vec2(ui.available_width(), SLIDER_HEIGHT);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let travel = (rect.width() - KNOB).max(1.0);
    let mut next = value;
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        next = ((pos.x - rect.left() - KNOB / 2.0) / travel).clamp(0.0, 1.0);
        // A pointer press does not focus a painted widget, so the arrow keys
        // would not reach it after a click.
        response.request_focus();
    }
    if response.has_focus() {
        let filter = EventFilter {
            horizontal_arrows: true,
            vertical_arrows: true,
            ..EventFilter::default()
        };
        ui.memory_mut(|memory| memory.set_focus_lock_filter(response.id, filter));
        // Each press is one step, also when several arrive in one frame.
        let presses = |input: &mut egui::InputState, keys: [Key; 2]| -> u16 {
            keys.into_iter()
                .map(|key| input.count_and_consume_key(Modifiers::NONE, key))
                .sum::<usize>()
                .try_into()
                .unwrap_or(u16::MAX)
        };
        let (down, up) = ui.input_mut(|input| {
            (
                presses(input, [Key::ArrowLeft, Key::ArrowDown]),
                presses(input, [Key::ArrowRight, Key::ArrowUp]),
            )
        });
        next = (next + STEP * (f32::from(up) - f32::from(down))).clamp(0.0, 1.0);
    }
    if (next - value).abs() > f32::EPSILON {
        draft.numbers[index] = format!("{next:.2}");
        response.mark_changed();
    }
    response.widget_info(|| WidgetInfo::slider(true, f64::from(next), label));
    let painter = ui.painter();
    let rail = egui::Rect::from_center_size(rect.center(), vec2(rect.width(), RAIL_HEIGHT));
    let radius = CornerRadius::same(tokens::RADIUS_S / 2);
    painter.rect_filled(rail, radius, tokens::SURFACE_HOVER);
    let knob_x = rect.left() + KNOB / 2.0 + next * travel;
    let filled = egui::Rect::from_min_max(rail.min, egui::pos2(knob_x, rail.max.y));
    painter.rect_filled(filled, radius, tokens::CORAL_BRIGHT);
    let center = egui::pos2(knob_x, rect.center().y);
    painter.circle_filled(center, KNOB / 2.0, tokens::TEXT_PRIMARY);
    if response.has_focus() {
        let ring = Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
        let knob = egui::Rect::from_center_size(center, egui::Vec2::splat(KNOB));
        painter.rect_stroke(
            knob,
            CornerRadius::same(tokens::RADIUS_L),
            ring,
            StrokeKind::Outside,
        );
    }
}
