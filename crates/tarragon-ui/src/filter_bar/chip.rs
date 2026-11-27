//! A removable chip for one held filter: a dot, the name, and a round x.
//!
//! The sizes follow the chips of design/redesign/main-window.html.

use std::fmt::Debug;
use std::hash::Hash;

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, CornerRadius, CursorIcon, FontFamily, FontId, Rect, Response, Sense, Stroke, Ui,
    WidgetInfo, WidgetType, pos2, vec2,
};

use crate::swatch::{Swatch, paint_round};
use crate::{fonts, tokens};

/// The chip height, in points.
pub const CHIP_HEIGHT: f32 = 28.0;
/// The corner radius: half the height, so the ends are round.
const CHIP_RADIUS: u8 = 14;
/// The padding left of the dot, in points.
const PAD_LEFT: f32 = 10.0;
/// The padding right of the x, in points.
const PAD_RIGHT: f32 = 6.0;
/// The gap between the dot, the name, and the x, in points.
const GAP: f32 = 6.0;
/// The dot diameter, in points.
const DOT: f32 = 8.0;
/// The side of the round x button, in points.
const CROSS: f32 = 20.0;
/// The x mark is drawn on a 10 unit square, from 2 to 8 on both axes.
const MARK: f32 = 10.0;
/// The stroke of the x mark, in points.
const MARK_STROKE: f32 = 1.6;

/// The mark in front of the chip name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dot {
    /// A colour filter: the swatch in small.
    Swatch(Swatch),
    /// A tag or folder filter: a plain dot of this colour.
    Fill(Color32),
}

/// Draw a chip with `dot`, `label`, and a remove x. `tooltip`, when given,
/// shows over the chip body. A name too wide for one line of the row ends in
/// an ellipsis and, with no `tooltip`, shows whole as the tooltip. Returns
/// whether the x was clicked.
///
/// `id_salt` keys the x, so its focus survives a reflow of the row; it must
/// be unique within the ui. egui's `Id::with` asks for `Debug` too.
///
/// The x is a button of its own, so it takes the keyboard focus, acts on
/// Space or Enter, and tells a screen reader which filter it removes. The gap
/// between chips belongs to the caller row.
pub fn show_chip(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    dot: Dot,
    label: &str,
    tooltip: Option<&str>,
) -> bool {
    let font = FontId::new(
        tokens::SIZE_BODY,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into()),
    );
    // A chip never grows past one line of the row: the name ends in an
    // ellipsis, and the whole name moves to the tooltip.
    let fixed = PAD_LEFT + DOT + GAP + GAP + CROSS + PAD_RIGHT;
    let mut job = LayoutJob::simple_singleline(label.to_owned(), font, tokens::TEXT_PRIMARY);
    job.wrap = TextWrapping {
        max_width: (ui.max_rect().width() - fixed).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let name = ui.painter().layout_job(job);
    let cut = name.elided;
    let width = fixed + name.size().x;
    let (rect, body) = ui.allocate_exact_size(vec2(width, CHIP_HEIGHT), Sense::hover());
    if let Some(tip) = tooltip.or(cut.then_some(label)) {
        body.on_hover_text(tip);
    }
    let cross_rect = cross_rect(rect);
    let cross = ui.interact(
        cross_rect,
        ui.id().with(("chip-x", id_salt)),
        Sense::click(),
    );
    cross.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::Button,
            ui.is_enabled(),
            format!("Remove {label}"),
        )
    });
    let cross = cross.on_hover_cursor(CursorIcon::PointingHand);

    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(CHIP_RADIUS), tokens::SURFACE_HOVER);
    let dot_center = pos2(rect.min.x + PAD_LEFT + DOT / 2.0, rect.center().y);
    match dot {
        Dot::Swatch(swatch) => paint_round(painter, dot_center, DOT / 2.0, swatch),
        Dot::Fill(color) => {
            painter.circle_filled(dot_center, DOT / 2.0, color);
        }
    }
    let name_x = rect.min.x + PAD_LEFT + DOT + GAP;
    let name_y = rect.center().y - name.size().y / 2.0;
    painter.galley(pos2(name_x, name_y), name, tokens::TEXT_PRIMARY);
    paint_cross(ui, &cross, cross_rect);
    cross.clicked()
}

/// The round x button at the right end of the chip.
fn cross_rect(chip: Rect) -> Rect {
    Rect::from_center_size(
        pos2(chip.max.x - PAD_RIGHT - CROSS / 2.0, chip.center().y),
        vec2(CROSS, CROSS),
    )
}

/// Paint the x: tertiary at rest; on hover or focus the raised step behind
/// it and the primary text; a coral ring while it has the keyboard focus.
fn paint_cross(ui: &Ui, response: &Response, rect: Rect) {
    let painter = ui.painter();
    let lit = response.hovered() || response.has_focus();
    let color = if lit {
        painter.circle_filled(rect.center(), CROSS / 2.0, tokens::SURFACE_HIGHLIGHT);
        tokens::TEXT_PRIMARY
    } else {
        tokens::TEXT_TERTIARY
    };
    if response.has_focus() {
        let ring = Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
        painter.circle_stroke(rect.center(), CROSS / 2.0 - ring.width / 2.0, ring);
    }
    let origin = rect.center() - vec2(MARK / 2.0, MARK / 2.0);
    let at = |x: f32, y: f32| origin + vec2(x, y);
    let stroke = Stroke::new(MARK_STROKE, color);
    painter.line_segment([at(2.0, 2.0), at(8.0, 8.0)], stroke);
    painter.line_segment([at(8.0, 2.0), at(2.0, 8.0)], stroke);
}

#[cfg(test)]
mod tests;
