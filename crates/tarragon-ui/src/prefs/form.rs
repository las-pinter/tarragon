//! The rows of the Preferences dialog and the text the user typed in them.

use egui::{
    Align, Button, Color32, ComboBox, FontFamily, FontId, Frame, Label, Layout, Margin, Rect,
    RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, pos2, vec2,
};

use super::draft::{CANVAS, CAP, Draft, MULTI, NEUTRAL, PALETTE, SHARE, WORKERS};
use crate::theme::points;
use crate::widgets::small_button;
use crate::{fonts, tokens};

/// Width of the value side of a row, in points.
const FIELD_WIDTH: f32 = 220.0;
/// Width of the cache folder field, so the Browse button fits beside it.
const CACHE_FIELD_WIDTH: f32 = 140.0;
/// Height of the busy bar, in points.
const BUSY_BAR_HEIGHT: f32 = 6.0;
/// The block of the busy bar is this share of the track wide.
const BUSY_BLOCK_SHARE: f32 = 0.45;
/// The block runs this many track widths, so it starts and ends off the track.
const BUSY_RUN_SHARE: f32 = 1.5;
/// Seconds the block takes to cross the track.
const BUSY_PERIOD: f64 = 1.2;
/// The height of the dialog outside the scroll area: title, footnote, error,
/// and buttons, with room for the window frame and the screen edge.
const NON_SCROLL_HEIGHT: f32 = 220.0;
/// The scroll area is never shorter than this, however short the screen.
const MIN_SCROLL_HEIGHT: f32 = 80.0;
/// The tile grid sizes the user can pick.
pub(super) const TILE_GRIDS: &[&str] = &["1x1", "2x2", "3x3", "4x4"];
/// The cache formats the user can pick.
pub(super) const CACHE_FORMATS: &[&str] = &["jpeg"];

/// A button the user pressed.
pub(super) enum Action {
    Ok,
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

/// Draw the dialog body and return the button the user pressed, if any.
pub(super) fn contents(ui: &mut Ui, draft: &mut Draft, view: &View<'_>) -> Option<Action> {
    let mut action = None;
    ui.spacing_mut().item_spacing.y = tokens::SPACING_S;
    // The groups scroll on a short screen. The footnote, the error, and the
    // buttons stay outside, so OK is always in reach.
    let max_height = (ui.ctx().content_rect().height() - NON_SCROLL_HEIGHT).max(MIN_SCROLL_HEIGHT);
    ScrollArea::vertical()
        .max_height(max_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            group(ui, "Performance", |ui| {
                number_row(ui, draft, WORKERS, "Max PSD Workers");
                number_row(ui, draft, MULTI, "Max Multi-Preview");
                number_row(ui, draft, CANVAS, "Large Canvas Threshold (MP)");
                row(ui, "Tile Grid Size", tokens::TEXT_PRIMARY, |ui| {
                    choice(ui, "tile_grid", &mut draft.rest.tile_grid, TILE_GRIDS);
                });
            });
            group(ui, "Color Tagging", |ui| {
                check_row(
                    ui,
                    "Enable Color Tagging",
                    &mut draft.rest.color_tag_enabled,
                );
                number_row(ui, draft, PALETTE, "Color Tag Palette Size");
                number_row(ui, draft, SHARE, "Color Tag Minimum Share");
                number_row(ui, draft, NEUTRAL, "Neutral Saturation Threshold");
            });
            group(ui, "Cache", |ui| {
                row(ui, "Cache Directory", tokens::TEXT_PRIMARY, |ui| {
                    if cache_dir_field(ui, draft.rest.cache_dir.as_deref()) {
                        action = Some(Action::Browse);
                    }
                });
                row(ui, "Cache Format", tokens::TEXT_PRIMARY, |ui| {
                    choice(
                        ui,
                        "cache_format",
                        &mut draft.rest.cache_format,
                        CACHE_FORMATS,
                    );
                });
                check_row(
                    ui,
                    "Clear Full-Res Cache on Exit",
                    &mut draft.rest.clear_full_res,
                );
                number_row(ui, draft, CAP, "Cache size cap (GiB, 0 = off)");
                row(ui, "Cache Size", tokens::TEXT_PRIMARY, |ui| {
                    ui.set_width(FIELD_WIDTH);
                    ui.label(view.cache_size);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(!view.purging, |ui| {
                            if small_button(ui, "Purge Cache").clicked() {
                                action = Some(Action::Purge);
                            }
                        });
                    });
                });
                if view.purging {
                    busy_strip(ui);
                }
            });
            group(ui, "Debug", |ui| {
                check_row(ui, "Enable Debug Logging", &mut draft.rest.debug);
            });
        });
    let note = "Colour tagging changes apply at the next folder open. \
                The cache folder applies at the next start. Max PSD Workers, \
                Large Canvas Threshold, Tile Grid Size, and Cache Format are \
                saved, but nothing reads them yet.";
    ui.add(
        Label::new(
            RichText::new(note)
                .size(tokens::SIZE_SMALL)
                .color(tokens::TEXT_MUTED),
        )
        .wrap(),
    );
    if let Some(error) = view.error {
        ui.colored_label(tokens::CORAL_STRONG, error);
    }
    ui.separator();
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let enabled = !view.purging;
        // The layout runs right to left, so Cancel comes first to sit right of OK.
        if ui.add_enabled(enabled, Button::new("Cancel")).clicked() {
            action = Some(Action::Cancel);
        }
        let ok = ui.add_enabled(
            enabled && draft.parse().is_some(),
            Button::new("OK").fill(tokens::CORAL_MUTED),
        );
        if ok.clicked() {
            action = Some(Action::Ok);
        }
    });
    action
}

/// A bordered group whose title sits on the top border.
fn group(ui: &mut Ui, title: &str, rows: impl FnOnce(&mut Ui)) {
    ui.add_space(tokens::SPACING_S);
    let margin = Margin {
        left: points(tokens::SPACING_S),
        right: points(tokens::SPACING_S),
        top: points(tokens::SPACING_L),
        bottom: points(tokens::SPACING_S),
    };
    let framed = Frame::new()
        .stroke(Stroke::new(tokens::STROKE_WIDTH, tokens::BG_TERTIARY))
        .corner_radius(tokens::RADIUS_S)
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            rows(ui);
        });
    let font = FontId::new(
        tokens::SIZE_BODY,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into()),
    );
    let galley = ui
        .painter()
        .layout_no_wrap(title.to_owned(), font, tokens::AMBER_ACCENT);
    let pos = framed.response.rect.left_top() + vec2(tokens::SPACING_S, -galley.size().y / 2.0);
    let gap = Rect::from_min_size(pos, galley.size()).expand2(vec2(tokens::SPACING_XS, 0.0));
    ui.painter().rect_filled(gap, 0.0, tokens::BG_SECONDARY);
    ui.painter().galley(pos, galley, tokens::AMBER_ACCENT);
}

/// One row: the label on the left, the value side on the right.
fn row(ui: &mut Ui, label: &str, color: Color32, value: impl FnOnce(&mut Ui)) {
    let label_width = ui.available_width() - FIELD_WIDTH - tokens::SPACING_M;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tokens::SPACING_M;
        ui.allocate_ui(vec2(label_width, 0.0), |ui| {
            ui.set_min_width(label_width);
            ui.label(RichText::new(label).color(color));
        });
        value(ui);
    });
}

fn number_row(ui: &mut Ui, draft: &mut Draft, index: usize, label: &str) {
    let color = if draft.valid(index) {
        tokens::TEXT_PRIMARY
    } else {
        tokens::CORAL_STRONG
    };
    row(ui, label, color, |ui| {
        let edit = TextEdit::singleline(&mut draft.numbers[index])
            .desired_width(FIELD_WIDTH)
            .text_color(color);
        ui.add(edit);
    });
}

fn check_row(ui: &mut Ui, label: &str, value: &mut bool) {
    row(ui, label, tokens::TEXT_PRIMARY, |ui| {
        ui.checkbox(value, "");
    });
}

fn choice(ui: &mut Ui, id: &str, current: &mut String, choices: &[&str]) {
    ComboBox::from_id_salt(id)
        .selected_text(current.as_str())
        .width(FIELD_WIDTH)
        .show_ui(ui, |ui| {
            for &choice in choices {
                ui.selectable_value(current, choice.to_owned(), choice);
            }
        });
}

/// The cache folder text and the Browse button. True when Browse was pressed.
fn cache_dir_field(ui: &mut Ui, dir: Option<&str>) -> bool {
    let (text, color) = match dir {
        Some(dir) => (dir, tokens::TEXT_PRIMARY),
        None => ("Default cache directory", tokens::TEXT_MUTED),
    };
    Frame::new()
        .fill(tokens::BG_TERTIARY)
        .corner_radius(tokens::RADIUS_S)
        .inner_margin(Margin::symmetric(
            points(tokens::SPACING_S),
            points(tokens::SPACING_XS),
        ))
        .show(ui, |ui| {
            ui.set_width(CACHE_FIELD_WIDTH);
            let label = Label::new(RichText::new(text).size(tokens::SIZE_CAPTION).color(color));
            ui.add(label.truncate()).on_hover_text(text);
        });
    small_button(ui, "Browse...").clicked()
}

/// How far the busy block has run, from 0 to 1, after `time` seconds.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the fraction is in 0 to 1, so f32 holds it"
)]
fn busy_phase(time: f64) -> f32 {
    (time / BUSY_PERIOD).fract() as f32
}

/// The "Purging cache..." label and a track with a block sliding across it.
fn busy_strip(ui: &mut Ui) {
    ui.horizontal(|ui| {
        let label = RichText::new("Purging cache...").size(tokens::SIZE_CAPTION);
        ui.label(label.color(tokens::TEXT_MUTED));
        let size = vec2(ui.available_width(), BUSY_BAR_HEIGHT);
        let (track, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter().with_clip_rect(track);
        painter.rect_filled(track, tokens::RADIUS_XS, tokens::BG_TERTIARY);
        // The block runs from off the left edge to off the right edge.
        let phase = busy_phase(ui.input(|i| i.time));
        let width = track.width() * BUSY_BLOCK_SHARE;
        let left = track.left() + track.width() * BUSY_RUN_SHARE * phase - width;
        let block = Rect::from_min_size(pos2(left, track.top()), vec2(width, track.height()));
        painter.rect_filled(block, tokens::RADIUS_XS, tokens::AMBER_ACCENT);
        ui.request_repaint();
    });
}
