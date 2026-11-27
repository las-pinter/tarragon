//! The gallery header: the breadcrumb, the folder heading with its count, and
//! the scope tabs.

use egui::{
    Align, CornerRadius, FontFamily, FontId, Frame, Id, Label, Layout, Margin, Response, RichText,
    Sense, Ui, Vec2, WidgetInfo, WidgetType, vec2,
};

use super::{Gallery, Scope};
use crate::fonts;
use crate::theme::points;
use crate::tokens;

/// The padding inside the segmented track, in points.
const TRACK_PAD: f32 = 3.0;
/// The padding of one segment on each axis, in points.
const SEGMENT_PAD: Vec2 = Vec2::new(14.0, 7.0);
/// The room between the heading and the count beside it, in points.
const COUNT_GAP: f32 = 8.0;
/// The room between the heading block and the scope tabs, in points, as the
/// design's header gap.
const HEADER_GAP: f32 = 12.0;
/// The room between the breadcrumb and the heading, in points, as the
/// design's heading margin.
const CRUMB_GAP: f32 = 2.0;
/// The letter spacing of the heading, in ems, as the design tightens it.
const HEADING_SPACING: f32 = -0.02;

impl Gallery {
    /// Draw the header and switch the scope when a tab is clicked.
    ///
    /// The tabs stay beside the heading while the row holds them; a narrower
    /// pane wraps them onto their own line, as the design's header does.
    pub(super) fn header(&mut self, ui: &mut Ui) {
        let track = track_size(ui);
        let count_width = ui
            .painter()
            .layout_no_wrap(self.count_text(), count_font(), tokens::TEXT_SECONDARY)
            .size()
            .x;
        let mut picked = None;
        if ui.available_width() < track.x + HEADER_GAP + count_width {
            // The design wraps the tabs under the heading when the row is full.
            self.heading(ui);
            ui.add_space(HEADER_GAP);
            picked = self.scope_tabs(ui, track);
        } else {
            ui.horizontal(|ui| {
                // The block and the tabs take the room that is left, so the
                // row adds no spacing of its own between them.
                ui.spacing_mut().item_spacing.x = 0.0;
                let left = (ui.available_width() - track.x - HEADER_GAP).max(0.0);
                ui.allocate_ui_with_layout(
                    vec2(left, track.y),
                    Layout::top_down(Align::Min),
                    |ui| {
                        // The block takes its whole share, so the tabs sit at
                        // the right end of the row as the design draws them.
                        ui.set_min_width(left);
                        self.heading(ui);
                    },
                );
                ui.add_space(HEADER_GAP);
                picked = self.scope_tabs(ui, track);
            });
        }
        if let Some(scope) = picked {
            self.set_scope(scope);
        }
    }

    /// Draw the breadcrumb above the heading line.
    ///
    /// The spacing lives in a scope, so the strip below the header keeps the
    /// theme's own rhythm in both the row and the wrapped paths.
    fn heading(&self, ui: &mut Ui) {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = CRUMB_GAP;
            if let Some(crumb) = self.breadcrumb() {
                ui.add(
                    Label::new(
                        RichText::new(crumb)
                            .size(tokens::SIZE_BODY)
                            .color(tokens::TEXT_SECONDARY),
                    )
                    .truncate(),
                );
            }
            self.heading_line(ui);
        });
    }

    /// Draw the folder heading with the count beside it.
    ///
    /// The heading gives way first, so a long folder name cuts short while the
    /// count stays whole. A pane too narrow for both cuts the count too, so the
    /// row never grows past its share.
    fn heading_line(&self, ui: &mut Ui) {
        let count = self.count_text();
        let count_width = ui
            .painter()
            .layout_no_wrap(count.clone(), count_font(), tokens::TEXT_SECONDARY)
            .size()
            .x;
        let name = self.scope_name();
        ui.horizontal(|ui| {
            let room = ui.available_width();
            let count_room = count_width.min(room);
            // A short name keeps the count beside it; a long name gives way.
            // The region shrinks to its content, so a short name leaves the
            // count beside it while a long one truncates at the row's share.
            let name_room = (room - count_room - COUNT_GAP).max(0.0);
            // Without room for the name, the gap goes too, so the count stays
            // inside the row.
            ui.spacing_mut().item_spacing.x = if name_room > 0.0 { COUNT_GAP } else { 0.0 };
            ui.allocate_ui_with_layout(
                vec2(name_room, 0.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(name)
                                .size(tokens::SIZE_DISPLAY)
                                .family(FontFamily::Name(fonts::BOLD_FAMILY.into()))
                                .color(tokens::TEXT_PRIMARY)
                                .extra_letter_spacing(HEADING_SPACING * tokens::SIZE_DISPLAY),
                        )
                        .truncate(),
                    );
                },
            );
            ui.allocate_ui_with_layout(
                vec2(count_room, 0.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(count)
                                .size(tokens::SIZE_BODY)
                                .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
                                .color(tokens::TEXT_SECONDARY),
                        )
                        .truncate(),
                    );
                },
            );
        });
    }

    /// Draw the Folder and All images tabs. Returns the picked scope.
    fn scope_tabs(&self, ui: &mut Ui, track: Vec2) -> Option<Scope> {
        let mut picked = None;
        ui.allocate_ui_with_layout(track, Layout::top_down(Align::Min), |ui| {
            Frame::new()
                .fill(tokens::BG_TERTIARY)
                .corner_radius(CornerRadius::same(tokens::RADIUS_L))
                .inner_margin(Margin::same(points(TRACK_PAD)))
                .show(ui, |ui| {
                    // The segments touch, as `track_size` measures them.
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.horizontal(|ui| {
                        for scope in [Scope::Folder, Scope::All] {
                            if scope_tab(ui, scope, self.scope == scope).clicked() {
                                picked = Some(scope);
                            }
                        }
                    });
                });
        });
        picked
    }
}

/// The font of the count beside the heading and of the scope tabs: the redesign
/// gives both the medium weight.
fn count_font() -> FontId {
    FontId::new(
        tokens::SIZE_BODY,
        FontFamily::Name(fonts::MEDIUM_FAMILY.into()),
    )
}

/// The size of the segmented track that holds both scope tabs.
fn track_size(ui: &Ui) -> Vec2 {
    let font = count_font();
    let width: f32 = [Scope::Folder, Scope::All]
        .into_iter()
        .map(|scope| {
            ui.painter()
                .layout_no_wrap(scope.label().to_owned(), font.clone(), tokens::TEXT_PRIMARY)
                .size()
                .x
                + 2.0 * SEGMENT_PAD.x
        })
        .sum();
    // Any capital gives the line height of the segment font.
    let line = ui
        .painter()
        .layout_no_wrap("X".to_owned(), font, tokens::TEXT_PRIMARY)
        .size()
        .y;
    vec2(
        width + 2.0 * TRACK_PAD,
        line + 2.0 * SEGMENT_PAD.y + 2.0 * TRACK_PAD,
    )
}

/// Draw one scope segment and return its click response.
///
/// The active segment takes the raised fill and primary text; an inactive one
/// has no fill at rest and lifts under the pointer.
fn scope_tab(ui: &mut Ui, scope: Scope, active: bool) -> Response {
    let color = if active {
        tokens::TEXT_PRIMARY
    } else {
        tokens::TEXT_TERTIARY
    };
    let galley = ui
        .painter()
        .layout_no_wrap(scope.label().to_owned(), count_font(), color);
    let size = galley.size() + 2.0 * SEGMENT_PAD;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(
        rect,
        Id::new(("tarragon-scope-tab", scope.label())),
        Sense::click(),
    );
    response.widget_info(|| {
        WidgetInfo::selected(
            WidgetType::SelectableLabel,
            ui.is_enabled(),
            active,
            scope.label(),
        )
    });
    let radius = CornerRadius::same(tokens::RADIUS_M);
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, radius, tokens::SURFACE_HIGHLIGHT);
    } else if response.hovered() || response.has_focus() {
        painter.rect_filled(rect, radius, tokens::SURFACE_HOVER);
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(tokens::STROKE_WIDTH, tokens::CORAL_STRONG),
            egui::StrokeKind::Inside,
        );
    }
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    response
}
