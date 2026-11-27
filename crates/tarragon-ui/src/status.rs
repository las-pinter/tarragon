//! The status bar along the bottom of the window: one segment for each job
//! that runs, in start order, and the selection count at the right end.
//!
//! The sizes follow the footer of design/redesign/main-window.html.

use std::time::Duration;

use egui::{Align, CornerRadius, Label, Layout, Rect, RichText, Sense, Ui, vec2};

use crate::preview::size_text;
use crate::{busy, tokens};

/// The height of the bar, in points: the body line and 6 pt above and below.
pub const STATUS_HEIGHT: f32 = 28.0;
/// The side padding of the bar, in points.
pub const STATUS_PAD_X: i8 = 16;
/// The gap between two segments, and before the selection count, in points.
const SEGMENT_GAP: f32 = 16.0;
/// The gap between a track and its text, in points.
const TRACK_GAP: f32 = 8.0;
/// The size of a track, in points.
const TRACK_SIZE: egui::Vec2 = vec2(120.0, 4.0);
/// The corner of a track, in points.
const TRACK_RADIUS: u8 = 2;
/// How often a segment with counts redraws while nothing else asks.
const COUNT_REFRESH: Duration = Duration::from_millis(100);

/// One job that runs, with what it knows of its progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    /// A folder scan, with the supported files found so far. It has no total.
    Scan {
        /// Files found so far.
        found: u64,
    },
    /// The whole library loads. It has no count.
    Library,
    /// Thumbnails settle, counted in files.
    Thumbnails {
        /// Files settled.
        done: u64,
        /// Files in all.
        total: u64,
    },
    /// Colors are extracted, counted in files.
    Colors {
        /// Files settled.
        done: u64,
        /// Files in all.
        total: u64,
    },
    /// The cache is purged, counted in bytes. A zero total is not known yet.
    Purge {
        /// Bytes removed.
        done: u64,
        /// Bytes the cache held, or zero before the walk ends.
        total: u64,
    },
}

impl Job {
    /// The text after the track.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Scan { found } => {
                let noun = if found == 1 { "file" } else { "files" };
                format!("Scanning {} {noun}", grouped(found))
            }
            Self::Library => "Loading library".to_owned(),
            Self::Thumbnails { done, total } => {
                format!(
                    "Rendering thumbnails {} / {}",
                    grouped(done),
                    grouped(total)
                )
            }
            Self::Colors { done, total } => {
                format!("Indexing colors {} / {}", grouped(done), grouped(total))
            }
            Self::Purge { total: 0, .. } => "Purging cache".to_owned(),
            Self::Purge { done, total } => {
                format!("Purging cache {} / {}", size_text(done), size_text(total))
            }
        }
    }

    /// The filled share of the track, from 0 to 1, or `None` when the job has
    /// no total and the block slides instead.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a share for a 120 pt track needs no more than f32"
    )]
    pub fn share(self) -> Option<f32> {
        match self {
            Self::Scan { .. } | Self::Library | Self::Purge { total: 0, .. } => None,
            Self::Thumbnails { done, total }
            | Self::Colors { done, total }
            | Self::Purge { done, total } => {
                Some((done as f32 / total.max(1) as f32).clamp(0.0, 1.0))
            }
        }
    }
}

/// `value` with a comma between each group of three digits, such as
/// "1,204".
#[must_use]
pub fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Draw the bar's contents: the `jobs` from the left and, when anything is
/// selected, "N selected" at the right.
pub(crate) fn show(ui: &mut Ui, jobs: &[Job], selected: usize) {
    let row = ui.max_rect();
    ui.allocate_ui_with_layout(row.size(), Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = SEGMENT_GAP;
        if selected > 0 {
            ui.label(caption(&format!("{} selected", grouped(selected as u64))));
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = SEGMENT_GAP;
            for job in jobs {
                segment(ui, *job);
            }
        });
    });
    if jobs.iter().any(|job| job.share().is_some()) {
        // A count moves on another thread, and a result may not arrive to
        // wake the frame loop.
        ui.ctx().request_repaint_after(COUNT_REFRESH);
    }
}

/// One job: the amber track, then its text.
fn segment(ui: &mut Ui, job: Job) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = TRACK_GAP;
        let (rect, _) =
            ui.allocate_exact_size(vec2(TRACK_SIZE.x, ui.available_height()), Sense::hover());
        let track = Rect::from_center_size(rect.center(), TRACK_SIZE);
        let radius = CornerRadius::same(TRACK_RADIUS);
        ui.painter()
            .rect_filled(track, radius, tokens::SURFACE_HOVER);
        match job.share() {
            Some(share) => {
                let fill =
                    Rect::from_min_size(track.min, vec2(track.width() * share, track.height()));
                ui.painter().rect_filled(fill, radius, tokens::AMBER_ACCENT);
            }
            None => busy::paint_sliding_block(ui, track, TRACK_RADIUS),
        }
        ui.add(Label::new(caption(&job.text())).truncate());
    });
}

/// Bar text: the body size in the low text colour, which the panel fill
/// allows.
fn caption(text: &str) -> RichText {
    RichText::new(text)
        .size(tokens::SIZE_BODY)
        .color(tokens::TEXT_SECONDARY)
}

#[cfg(test)]
mod tests;
