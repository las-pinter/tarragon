//! The list of settings groups at the left of the Preferences window.

use egui::{
    Align2, CornerRadius, CursorIcon, FontFamily, FontId, Sense, Stroke, StrokeKind, Ui,
    WidgetInfo, WidgetType, vec2,
};

use crate::{fonts, tokens};

/// The width of the list, padding included, in points.
pub(super) const WIDTH: f32 = 200.0;
/// The height of one row, in points.
const ROW_HEIGHT: f32 = 34.0;
/// The room above the list and at its sides, in points.
const PAD: (f32, f32) = (12.0, 4.0);
/// The gap between two rows, in points.
const ROW_GAP: f32 = 2.0;
/// The text inset of a row, in points.
const TEXT_INSET: f32 = 10.0;

/// One group of settings, shown alone at the right of the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Group {
    /// Decoding and the multi-selection preview.
    #[default]
    Performance,
    /// The colour buckets of the gallery filter.
    ColorTagging,
    /// The cache folder, its size, and the purge.
    Cache,
    /// The log level.
    Debug,
}

impl Group {
    /// Every group, in list order.
    pub const ALL: [Self; 4] = [
        Self::Performance,
        Self::ColorTagging,
        Self::Cache,
        Self::Debug,
    ];

    /// The name the list and the heading show.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Performance => "Performance",
            Self::ColorTagging => "Color tagging",
            Self::Cache => "Cache",
            Self::Debug => "Debug",
        }
    }
}

/// Draw the list. A click on a row shows its group. `invalid` names the groups
/// that hold a number that does not parse; their names show in coral.
pub(super) fn show(ui: &mut Ui, current: &mut Group, invalid: impl Fn(Group) -> bool) {
    ui.add_space(PAD.1);
    for group in Group::ALL {
        let (outer, _) = ui.allocate_exact_size(vec2(WIDTH, ROW_HEIGHT), Sense::hover());
        let rect = outer.shrink2(vec2(PAD.0, 0.0));
        let response = ui
            .interact(
                rect,
                ui.id().with(("prefs_group", group.title())),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        if response.clicked() {
            *current = group;
        }
        let selected = *current == group;
        response.widget_info(|| {
            WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, group.title())
        });
        let (fill, text, family) = if selected {
            (
                tokens::CORAL_DARK,
                tokens::CORAL_STRONG,
                FontFamily::Name(fonts::MEDIUM_FAMILY.into()),
            )
        } else if response.hovered() {
            (
                tokens::SURFACE_HOVER,
                tokens::TEXT_PRIMARY,
                FontFamily::Proportional,
            )
        } else {
            (
                egui::Color32::TRANSPARENT,
                tokens::TEXT_PRIMARY,
                FontFamily::Proportional,
            )
        };
        let text = if invalid(group) {
            tokens::CORAL_STRONG
        } else {
            text
        };
        let painter = ui.painter();
        let radius = CornerRadius::same(tokens::RADIUS_M);
        painter.rect_filled(rect, radius, fill);
        if response.has_focus() {
            let ring = Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
            painter.rect_stroke(rect, radius, ring, StrokeKind::Inside);
        }
        painter.text(
            rect.left_center() + vec2(TEXT_INSET, 0.0),
            Align2::LEFT_CENTER,
            group.title(),
            FontId::new(tokens::SIZE_BODY, family),
            text,
        );
        ui.add_space(ROW_GAP);
    }
}
