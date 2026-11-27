//! The four groups of the Preferences window: a heading, a line on what the
//! group does and when it applies, then its inputs.

use egui::{Align, CornerRadius, Frame, Label, Layout, Margin, RichText, Ui, vec2};

use super::draft::{CANVAS, CAP, Draft, MULTI, NEUTRAL, PALETTE, SHARE, WORKERS};
use super::fields::{self, FIELD_HEIGHT};
use super::form::{Action, View};
use super::nav::Group;
use crate::swatch::bucket_color;
use crate::{busy, fonts, tokens, widgets};

/// The gap between the blocks of a group, in points.
pub(super) const BLOCK_GAP: f32 = 20.0;
/// The gap between a heading and its line, in points.
const HEADING_GAP: f32 = 4.0;
/// The gap between the bucket bars, in points.
const BAR_GAP: f32 = 6.0;
/// The height of a bucket bar, in points.
const BAR_HEIGHT: f32 = 24.0;
/// The gap inside the slider block, in points.
const SLIDER_GAP: f32 = 10.0;
/// The tile grid sizes the user can pick.
pub(super) const TILE_GRIDS: &[&str] = &["1x1", "2x2", "3x3", "4x4"];
/// The cache formats the user can pick.
pub(super) const CACHE_FORMATS: &[&str] = &["jpeg"];
/// The text of the cache folder field when the default folder is used.
pub(super) const DEFAULT_CACHE_DIR: &str = "Default cache directory";

/// The line under each heading: what the group does and when a change applies.
#[must_use]
pub(super) const fn description(group: Group) -> &'static str {
    match group {
        Group::Performance => {
            "How much work runs at once and how many pictures a multi-selection \
             shows. Max PSD workers, the large canvas threshold, and the tile \
             grid size are saved, but nothing reads them yet."
        }
        Group::ColorTagging => {
            "Sort each image into color buckets for the gallery filter. Changes \
             apply at the next folder open."
        }
        Group::Cache => {
            "Where rendered thumbnails and previews are kept. The cache folder \
             and the size cap apply at the next start. The cache format is \
             saved, but nothing reads it yet."
        }
        Group::Debug => "Write debug messages to the log. The change applies at once.",
    }
}

/// The groups that hold a number that does not parse.
#[must_use]
pub(super) fn invalid(draft: &Draft, group: Group) -> bool {
    let numbers: &[usize] = match group {
        Group::Performance => &[WORKERS, MULTI, CANVAS],
        Group::ColorTagging => &[PALETTE, SHARE, NEUTRAL],
        Group::Cache => &[CAP],
        Group::Debug => &[],
    };
    numbers.iter().any(|&index| !draft.valid(index))
}

/// Draw `group`. A pressed Browse or Purge button sets `action`.
pub(super) fn show(
    ui: &mut Ui,
    group: Group,
    draft: &mut Draft,
    view: &View<'_>,
    action: &mut Option<Action>,
) {
    ui.spacing_mut().item_spacing.y = BLOCK_GAP;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = HEADING_GAP;
        let heading = RichText::new(group.title())
            .size(tokens::SIZE_TITLE)
            .family(egui::FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
            .color(tokens::TEXT_PRIMARY);
        ui.label(heading);
        let line = RichText::new(description(group)).color(tokens::TEXT_SECONDARY);
        ui.add(Label::new(line).wrap());
    });
    match group {
        Group::Performance => {
            fields::pair(ui, |ui, column| {
                let (index, label) =
                    [(WORKERS, "Max PSD workers"), (MULTI, "Max multi-preview")][column];
                fields::number(ui, draft, index, label);
            });
            fields::pair(ui, |ui, column| {
                if column == 0 {
                    fields::number(ui, draft, CANVAS, "Large canvas threshold (MP)");
                } else {
                    let current = &mut draft.rest.tile_grid;
                    fields::choice(ui, "tile_grid", "Tile grid size", current, TILE_GRIDS);
                }
            });
        }
        Group::ColorTagging => {
            let enabled = &mut draft.rest.color_tag_enabled;
            fields::toggle_row(ui, "Enable color tagging", enabled);
            fields::pair(ui, |ui, column| {
                let (index, label) = [(PALETTE, "Palette size"), (SHARE, "Minimum share")][column];
                fields::number(ui, draft, index, label);
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = SLIDER_GAP;
                fields::threshold(ui, draft, NEUTRAL, "Neutral saturation threshold");
                bucket_bars(ui);
                let hint = RichText::new("Colors below the threshold count as Neutral.")
                    .color(tokens::TEXT_SECONDARY);
                ui.label(hint);
            });
        }
        Group::Cache => cache(ui, draft, view, action),
        Group::Debug => fields::toggle_row(ui, "Enable debug logging", &mut draft.rest.debug),
    }
}

/// The cache group: the folder, the format and the cap, the exit clear, and
/// the size with the purge.
fn cache(ui: &mut Ui, draft: &mut Draft, view: &View<'_>, action: &mut Option<Action>) {
    fields::labelled(ui, "Cache folder", tokens::TEXT_TERTIARY, |ui| {
        let row = vec2(ui.available_width(), FIELD_HEIGHT);
        ui.allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = tokens::SPACING_S;
            // Right to left: the button first, then the field takes the rest.
            if widgets::secondary_button(ui, "Browse...").clicked() {
                *action = Some(Action::Browse);
            }
            let (text, color) = match draft.rest.cache_dir.as_deref() {
                Some(dir) => (dir.to_owned(), tokens::TEXT_PRIMARY),
                None => (DEFAULT_CACHE_DIR.to_owned(), tokens::TEXT_SECONDARY),
            };
            let pad = fields::FIELD_PAD;
            let inner = vec2(
                (ui.available_width() - 2.0 * f32::from(pad)).max(0.0),
                FIELD_HEIGHT,
            );
            Frame::new()
                .fill(tokens::BG_TERTIARY)
                .corner_radius(CornerRadius::same(tokens::RADIUS_L))
                .inner_margin(Margin::symmetric(pad, 0))
                .show(ui, |ui| {
                    ui.allocate_ui_with_layout(inner, Layout::left_to_right(Align::Center), |ui| {
                        ui.set_min_size(inner);
                        let label = Label::new(RichText::new(&text).color(color)).truncate();
                        ui.add(label).on_hover_text(&text);
                    });
                });
        });
    });
    fields::pair(ui, |ui, column| {
        if column == 0 {
            let current = &mut draft.rest.cache_format;
            fields::choice(ui, "cache_format", "Cache format", current, CACHE_FORMATS);
        } else {
            fields::number(ui, draft, CAP, "Cache size cap (GiB, 0 = off)");
        }
    });
    let clear = &mut draft.rest.clear_full_res;
    fields::toggle_row(ui, "Clear full-size cache on exit", clear);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = fields::LABEL_GAP;
        let row = vec2(ui.available_width(), FIELD_HEIGHT);
        ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = tokens::SPACING_S;
            ui.label(RichText::new("Cache size").color(tokens::TEXT_TERTIARY));
            ui.label(RichText::new(view.cache_size).color(tokens::TEXT_PRIMARY));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(!view.purging, |ui| {
                    if widgets::secondary_button(ui, "Purge cache").clicked() {
                        *action = Some(Action::Purge);
                    }
                });
            });
        });
        if view.purging {
            busy::strip(ui, "Purging cache...");
        }
    });
}

/// The eleven bucket colours, neutral first, as equal bars across the width.
#[expect(clippy::cast_precision_loss, reason = "eleven bars, far below 2^24")]
fn bucket_bars(ui: &mut Ui) {
    let buckets: Vec<_> = crate::swatch::Swatch::ALL
        .iter()
        .filter_map(|swatch| match swatch {
            crate::swatch::Swatch::Bucket(bucket) => Some(*bucket),
            crate::swatch::Swatch::Transparent => None,
        })
        .collect();
    // The page puts neutral, the last bucket, first.
    let order = buckets
        .iter()
        .rev()
        .take(1)
        .chain(&buckets[..buckets.len() - 1]);
    let count = buckets.len() as f32;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, BAR_HEIGHT), egui::Sense::hover());
    let bar = (width - BAR_GAP * (count - 1.0)) / count;
    for (index, bucket) in order.enumerate() {
        let x = rect.left() + index as f32 * (bar + BAR_GAP);
        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(bar, BAR_HEIGHT));
        ui.painter().rect_filled(
            cell,
            CornerRadius::same(tokens::RADIUS_S),
            bucket_color(*bucket),
        );
    }
}
