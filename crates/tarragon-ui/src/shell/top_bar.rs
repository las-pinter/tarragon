//! The top bar: the logo, the menus, the search field, and the icon buttons.

use std::sync::Arc;
use std::time::Instant;

use egui::epaint::{PathShape, PathStroke};
use egui::{
    Align, Color32, CornerRadius, FontFamily, FontId, Frame, Galley, Id, Layout, Margin, Pos2,
    RichText, Sense, TextEdit, Ui, Vec2, vec2,
};

use super::Shell;
use super::prefs::MenuEvent;
use crate::fonts;
use crate::theme::points;
use crate::tokens;
use crate::widgets::{self, Icon};

/// The height of the top bar, in points: the search field and the bar padding.
pub(super) const TOP_BAR_HEIGHT: f32 = SEARCH_HEIGHT + 2.0 * BAR_PAD_Y;
/// The side padding of the bar, in points.
pub(super) const BAR_PAD_X: i8 = 16;
/// The room above and below the search field, in points, as the design shows
/// it. It only sizes the bar: the panel has no vertical margin and centres the
/// row in that height.
const BAR_PAD_Y: f32 = 10.0;
/// The room between the logo and the wordmark, in points.
const LOGO_GAP: f32 = 10.0;
/// The gap between two menu buttons, in points, as the design shows it.
pub(super) const MENU_GAP: f32 = 2.0;
/// The side and vertical padding of a menu button, in points.
pub(super) const MENU_PAD: Vec2 = Vec2::new(10.0, 6.0);
/// The logo square, in points.
const LOGO_SIZE: f32 = 22.0;
/// The logo corner radius, in points, as the redesign draws it.
const LOGO_RADIUS: u8 = 5;
/// The widest the search field grows, in points.
const SEARCH_MAX_WIDTH: f32 = 560.0;
/// The least width of the search field, in points.
const SEARCH_MIN_WIDTH: f32 = 160.0;
/// The height of the search field, in points.
const SEARCH_HEIGHT: f32 = 36.0;
/// The search field side padding, in points.
const SEARCH_PAD_X: f32 = 12.0;
/// The painted size of the search mark, in points.
const SEARCH_ICON: f32 = 16.0;
/// The room between the search mark, the text, and the hint chip, in points.
const SEARCH_GAP: f32 = 8.0;
/// The hint chip padding on each axis, in points.
const CHIP_PAD: Vec2 = Vec2::new(6.0, 2.0);
/// The id of the search field, so a shortcut can focus it by name.
pub(crate) const SEARCH_ID: &str = "tarragon-gallery-search";
/// The hint text of the search field. The text filter matches file names only,
/// so the hint names no other field.
const SEARCH_HINT: &str = "Search file names";
/// The shortcut chip of the search field.
const SHORTCUT_HINT: &str = "Ctrl K";

impl Shell {
    /// Draw the row of the top bar, inside the panel that the shell draws.
    ///
    /// The search field drives the gallery search, and the two icon buttons
    /// toggle the Log panel and open Preferences.
    pub(super) fn top_bar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            logo(ui);
            ui.add_space(LOGO_GAP);
            wordmark(ui);
            ui.add_space(tokens::SPACING_L);
            self.menus(ui, ctx);
            ui.add_space(tokens::SPACING_L);
            self.search_row(ui);
        });
    }

    /// Draw the search field between the menus and the icon buttons.
    ///
    /// A window too narrow to hold the field leaves it out, and one too narrow
    /// for the icons leaves them out too, so nothing overlaps the menus.
    fn search_row(&mut self, ui: &mut Ui) {
        let buttons = 2.0 * widgets::ICON_BUTTON_SIZE + tokens::SPACING_XS;
        let room = (ui.available_width() - buttons - tokens::SPACING_L).max(0.0);
        if room < SEARCH_MIN_WIDTH {
            // A hidden field cannot take focus, so a shortcut pressed now must
            // not steal the keys once the window grows.
            self.gallery.drop_search_focus_request();
            ui.add_space(room);
        } else {
            let width = room.min(SEARCH_MAX_WIDTH);
            // The field keeps the middle of the room that is left, as the
            // design centers it.
            ui.add_space((room - width) / 2.0);
            self.search_field(ui, width);
        }
        if ui.available_width() < buttons {
            return;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::icon_button(ui, Icon::Settings, false).clicked() {
                self.menu_events.push(MenuEvent::OpenPreferences);
            }
            ui.add_space(tokens::SPACING_XS);
            if widgets::icon_button(ui, Icon::Log, self.log_visible()).clicked() {
                self.toggle_log();
            }
        });
    }

    /// Draw the search field: the search mark, the editable text, and the
    /// shortcut chip.
    fn search_field(&mut self, ui: &mut Ui, width: f32) {
        let chip_text = chip_galley(ui);
        let chip = chip_size(&chip_text);
        let frame = Frame::new()
            .fill(tokens::BG_TERTIARY)
            .corner_radius(CornerRadius::same(tokens::RADIUS_L))
            .inner_margin(Margin::symmetric(points(SEARCH_PAD_X), 0));
        let now = Instant::now();
        // The field sizes its parts from the width it was given, so the bar's
        // buttons keep their places. A width that cannot hold the pad keeps a
        // content width of zero rather than a negative one.
        let text_width =
            (width - 2.0 * SEARCH_PAD_X - SEARCH_ICON - 2.0 * SEARCH_GAP - chip.x).max(0.0);
        let response = frame.show(ui, |ui| {
            ui.set_min_size(vec2((width - 2.0 * SEARCH_PAD_X).max(0.0), SEARCH_HEIGHT));
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = SEARCH_GAP;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(SEARCH_ICON), Sense::hover());
                widgets::paint_icon(ui.painter(), rect, Icon::Search, tokens::TEXT_SECONDARY);
                let output = TextEdit::singleline(self.gallery.search_mut())
                    .id(Id::new(SEARCH_ID))
                    .hint_text(SEARCH_HINT)
                    .text_color(tokens::TEXT_PRIMARY)
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .desired_width(text_width)
                    .show(ui);
                let (focused, changed) = (output.response.has_focus(), output.response.changed());
                self.gallery.apply_search_focus(ui, output);
                hint_chip(ui, chip_text);
                (focused, changed)
            })
            .inner
        });
        let (focused, changed) = response.inner;
        if focused {
            ui.painter().rect_stroke(
                response.response.rect,
                CornerRadius::same(tokens::RADIUS_L),
                egui::Stroke::new(2.0, tokens::CORAL_STRONG),
                egui::StrokeKind::Inside,
            );
        }
        if changed {
            // The gallery asks for the wake frame again on every pass.
            self.gallery.arm_search(now);
        }
    }
}

/// Draw the logo mark: a coral square with a dark leaf, as the redesign shows.
fn logo(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(LOGO_SIZE), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(LOGO_RADIUS), tokens::CORAL_BRIGHT);
    // The two bezier halves of the leaf, sampled and closed.
    let scale = rect.width() / LOGO_SIZE;
    let at = |x: f32, y: f32| rect.min + vec2(x, y) * scale;
    let mut path = cubic(at(6.0, 16.0), at(8.0, 9.0), at(12.0, 6.0), at(16.0, 6.0));
    path.extend(cubic(
        at(16.0, 6.0),
        at(16.0, 10.0),
        at(13.0, 15.0),
        at(6.0, 16.0),
    ));
    painter.add(egui::Shape::Path(PathShape {
        points: path,
        closed: true,
        fill: tokens::BG_PRIMARY,
        stroke: PathStroke::NONE,
    }));
}

/// The points of one cubic bezier, without its start point.
fn cubic(from: Pos2, first: Pos2, second: Pos2, to: Pos2) -> Vec<Pos2> {
    const STEPS: u8 = 10;
    (1..=STEPS)
        .map(|step| {
            let t = f32::from(step) / f32::from(STEPS);
            let left = 1.0 - t;
            let weights = [
                left * left * left,
                3.0 * left * left * t,
                3.0 * left * t * t,
                t * t * t,
            ];
            Pos2::new(
                weights[0] * from.x
                    + weights[1] * first.x
                    + weights[2] * second.x
                    + weights[3] * to.x,
                weights[0] * from.y
                    + weights[1] * first.y
                    + weights[2] * second.y
                    + weights[3] * to.y,
            )
        })
        .collect()
}

/// Draw the product name in the bold face at the title size.
fn wordmark(ui: &mut Ui) {
    ui.label(
        RichText::new(super::PRODUCT_NAME)
            .size(tokens::SIZE_TITLE)
            .family(FontFamily::Name(fonts::BOLD_FAMILY.into()))
            .color(tokens::TEXT_PRIMARY)
            .extra_letter_spacing(-0.01 * tokens::SIZE_TITLE),
    );
}

/// Draw one menu button and the menu that opens from it.
///
/// A menu button has no fill at rest and the hover fill under the pointer, as
/// the redesign top bar shows it. The gap between two buttons comes from the
/// row that draws them.
pub(super) fn menu(ui: &mut Ui, label: &str, menus: impl FnOnce(&mut Ui)) {
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        for state in [
            &mut widgets.inactive,
            &mut widgets.noninteractive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            state.weak_bg_fill = Color32::TRANSPARENT;
        }
        widgets.hovered.weak_bg_fill = tokens::SURFACE_HOVER;
        widgets.open.weak_bg_fill = tokens::SURFACE_HOVER;
        widgets.active.weak_bg_fill = tokens::SURFACE_HOVER;
        ui.spacing_mut().button_padding = MENU_PAD;
        ui.menu_button(
            RichText::new(label)
                .size(tokens::SIZE_BODY)
                .color(tokens::TEXT_TERTIARY),
            menus,
        );
    });
}

/// A one-line hint chip in the search field, around the laid-out `galley`.
fn hint_chip(ui: &mut Ui, galley: Arc<Galley>) {
    let (rect, _) = ui.allocate_exact_size(chip_size(&galley), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same(tokens::RADIUS_S),
        tokens::SURFACE_HOVER,
    );
    painter.galley(rect.min + CHIP_PAD, galley, tokens::TEXT_TERTIARY);
}

/// The shortcut hint, laid out once per frame for both the size and the paint.
fn chip_galley(ui: &Ui) -> Arc<Galley> {
    ui.painter().layout_no_wrap(
        SHORTCUT_HINT.to_owned(),
        FontId::monospace(tokens::SIZE_CAPTION),
        tokens::TEXT_TERTIARY,
    )
}

/// The size of the hint chip around `galley`, in points.
fn chip_size(galley: &Galley) -> Vec2 {
    galley.size() + 2.0 * CHIP_PAD
}

#[cfg(test)]
mod tests;
