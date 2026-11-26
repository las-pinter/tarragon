//! Shared button widgets.

use egui::{Button, FontFamily, RichText};
use egui::{
    Color32, CornerRadius, CursorIcon, FontId, Pos2, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo, WidgetType, vec2,
};

use crate::fonts;
use crate::tokens;

/// The height of a flat button, in points, as the redesign components page.
pub const BUTTON_HEIGHT: f32 = 36.0;
/// The width and height of the toggle switch, in points.
const TOGGLE_SIZE: Vec2 = Vec2::new(36.0, 20.0);
/// The room between the toggle track and its knob, in points.
const TOGGLE_INSET: f32 = 2.0;
/// The knob diameter, in points.
const KNOB_DIAMETER: f32 = 16.0;

/// A small button: `SIZE_SMALL` text with 8 by 3 point padding, as `.btn.small`.
pub(crate) fn small_button(ui: &mut Ui, text: &str) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(8.0, 3.0);
        ui.add(Button::new(RichText::new(text).size(tokens::SIZE_SMALL)))
    })
    .inner
}

/// The height of an action button: the mockup `.btn` height.
pub(crate) const ACTION_BUTTON_HEIGHT: f32 = 30.0;

/// An action button: `SIZE_BODY` text with 12 by 6 point padding, as `.btn`.
pub(crate) fn action_button(ui: &mut Ui, text: &str) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = vec2(tokens::SPACING_M, 6.0);
        ui.add(
            Button::new(RichText::new(text).size(tokens::SIZE_BODY))
                .min_size(vec2(0.0, ACTION_BUTTON_HEIGHT)),
        )
    })
    .inner
}

/// A primary action: a coral fill with dark text, as the redesign shows. The
/// dark text is what keeps the label above the contrast bar on this fill.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    flat_button(
        ui,
        text,
        tokens::CORAL_BRIGHT,
        tokens::CORAL_STRONG,
        tokens::BG_PRIMARY,
    )
}

/// A secondary action: the hover fill with primary text, lifted by a step on
/// hover.
pub fn secondary_button(ui: &mut Ui, text: &str) -> Response {
    flat_button(
        ui,
        text,
        tokens::SURFACE_HOVER,
        tokens::SURFACE_HIGHLIGHT,
        tokens::TEXT_PRIMARY,
    )
}

/// A text action: no frame at rest, coral text, as the redesign shows.
pub fn text_button(ui: &mut Ui, text: &str) -> Response {
    let color = if ui.is_enabled() {
        tokens::CORAL_STRONG
    } else {
        tokens::TEXT_SECONDARY
    };
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        widgets.hovered.weak_bg_fill = tokens::SURFACE_HOVER;
        widgets.active.weak_bg_fill = tokens::SURFACE_HIGHLIGHT;
        widgets.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
        ui.spacing_mut().button_padding = vec2(tokens::SPACING_M, 0.0);
        ui.add(
            Button::new(label(text, color))
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(tokens::RADIUS_L))
                .min_size(vec2(0.0, BUTTON_HEIGHT)),
        )
    })
    .inner
}

/// Draw a flat button of the given fill and text color. The fill steps up on
/// hover and the disabled state takes the disabled fill, so the button never
/// looks enabled while it cannot act.
fn flat_button(
    ui: &mut Ui,
    text: &str,
    fill: Color32,
    hover_fill: Color32,
    color: Color32,
) -> Response {
    // A disabled button keeps the inactive state, so its fill is chosen here
    // rather than by the widget state.
    let disabled = !ui.is_enabled();
    let color = if disabled {
        tokens::TEXT_SECONDARY
    } else {
        color
    };
    let (fill, hover_fill) = if disabled {
        (tokens::BG_DISABLED, tokens::BG_DISABLED)
    } else {
        (fill, hover_fill)
    };
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        widgets.inactive.weak_bg_fill = fill;
        widgets.hovered.weak_bg_fill = hover_fill;
        widgets.active.weak_bg_fill = hover_fill;
        widgets.noninteractive.weak_bg_fill = tokens::BG_DISABLED;
        ui.spacing_mut().button_padding = vec2(tokens::SPACING_L, 0.0);
        ui.add(
            Button::new(label(text, color))
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(tokens::RADIUS_L))
                .min_size(vec2(0.0, BUTTON_HEIGHT)),
        )
    })
    .inner
}

/// Button text in the medium face, which carries the weight the redesign
/// gives a button label.
fn label(text: &str, color: Color32) -> RichText {
    RichText::new(text)
        .size(tokens::SIZE_BODY)
        .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into()))
        .color(color)
}

/// A toggle switch: a coral track with the knob at the right when on, a raised
/// track with the knob at the left when off. A click flips `on`.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let enabled = ui.is_enabled();
    let (rect, mut response) = ui.allocate_exact_size(TOGGLE_SIZE, Sense::click());
    if enabled && response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, *on, ""));
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let radius = CornerRadius::same(tokens::RADIUS_XL);
    let track = if !enabled {
        tokens::BG_DISABLED
    } else if *on {
        tokens::CORAL_BRIGHT
    } else {
        tokens::SURFACE_HIGHLIGHT
    };
    let knob = if !enabled {
        tokens::TEXT_SECONDARY
    } else if *on {
        tokens::BG_PRIMARY
    } else {
        tokens::TEXT_SECONDARY
    };
    let knob_x = if *on {
        rect.right() - TOGGLE_INSET - KNOB_DIAMETER
    } else {
        rect.left() + TOGGLE_INSET
    };
    let center = Pos2::new(knob_x + KNOB_DIAMETER / 2.0, rect.center().y);
    let painter = ui.painter();
    painter.rect_filled(rect, radius, track);
    painter.circle_filled(center, KNOB_DIAMETER / 2.0, knob);
    response
}

/// A ghost button: a transparent pill with a thin border, caption-size
/// secondary text, and a highlight fill on hover.
pub(crate) fn ghost_button(ui: &mut Ui, text: &str) -> Response {
    let font = FontId::proportional(tokens::SIZE_CAPTION);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, tokens::TEXT_SECONDARY);
    // The stylesheet pads 2 and 8 points inside a 1 point border.
    let pad = vec2(tokens::SPACING_S, tokens::SPACING_XS / 2.0);
    let border = Vec2::splat(tokens::STROKE_WIDTH);
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + (pad + border) * 2.0, Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let radius = CornerRadius::same(tokens::RADIUS_XL);
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, radius, tokens::SURFACE_HIGHLIGHT);
    }
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(tokens::STROKE_WIDTH, tokens::BORDER_INTERACTIVE),
        StrokeKind::Inside,
    );
    let color = if hovered {
        tokens::TEXT_PRIMARY
    } else {
        tokens::TEXT_SECONDARY
    };
    painter.galley(rect.min + pad + border, galley, color);
    response
}

#[cfg(test)]
mod tests {
    use egui::epaint::Shape;
    use egui::{Context, Pos2, Rect, vec2};

    use super::*;
    use crate::test_support;

    /// A 400 point wide screen with one warm-up frame run.
    fn ready() -> Context {
        test_support::context()
    }

    /// The shapes of one frame that draws with `draw`.
    fn shapes(ctx: &Context, draw: impl FnMut(&mut Ui)) -> Vec<Shape> {
        let output = ctx.run_ui(test_support::input(400.0, vec![]), draw);
        let shapes = output
            .shapes
            .iter()
            .map(|clipped| clipped.shape.clone())
            .collect();
        output.drop_without_applying_deltas();
        shapes
    }

    /// Whether a rect of this fill was painted.
    fn has_fill(shapes: &[Shape], fill: Color32) -> bool {
        shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == fill))
    }

    /// Whether text of this color was painted.
    fn has_text_color(shapes: &[Shape], color: Color32) -> bool {
        shapes.iter().any(|shape| match shape {
            Shape::Text(text) => text
                .galley
                .job
                .sections
                .iter()
                .any(|section| section.format.color == color),
            _ => false,
        })
    }

    /// The rect of the first circle of this color.
    fn circle(shapes: &[Shape], color: Color32) -> Option<Rect> {
        shapes.iter().find_map(|shape| match shape {
            Shape::Circle(circle) if circle.fill == color => Some(circle.visual_bounding_rect()),
            _ => None,
        })
    }

    #[test]
    fn a_primary_button_is_coral_with_dark_text() {
        let ctx = ready();
        let shapes = shapes(&ctx, |ui| {
            primary_button(ui, "Open");
        });
        assert!(has_fill(&shapes, tokens::CORAL_BRIGHT), "{shapes:?}");
        assert!(has_text_color(&shapes, tokens::BG_PRIMARY), "{shapes:?}");
    }

    #[test]
    fn a_secondary_button_is_the_hover_fill_with_primary_text() {
        let ctx = ready();
        let shapes = shapes(&ctx, |ui| {
            secondary_button(ui, "Reveal");
        });
        assert!(has_fill(&shapes, tokens::SURFACE_HOVER), "{shapes:?}");
        assert!(has_text_color(&shapes, tokens::TEXT_PRIMARY), "{shapes:?}");
    }

    #[test]
    fn a_hovered_primary_button_lifts_to_the_light_coral() {
        let ctx = ready();
        let mut rect = Rect::NOTHING;
        ctx.run_ui(test_support::input(400.0, vec![]), |ui| {
            rect = primary_button(ui, "Open").rect;
        })
        .drop_without_applying_deltas();
        ctx.run_ui(
            test_support::input(400.0, vec![egui::Event::PointerMoved(rect.center())]),
            |ui| {
                primary_button(ui, "Open");
            },
        )
        .drop_without_applying_deltas();
        let shapes = shapes(&ctx, |ui| {
            primary_button(ui, "Open");
        });
        assert!(has_fill(&shapes, tokens::CORAL_STRONG), "{shapes:?}");
    }

    #[test]
    fn a_disabled_primary_button_leaves_the_coral_fill_and_text() {
        let ctx = ready();
        let shapes = shapes(&ctx, |ui| {
            ui.add_enabled_ui(false, |ui| {
                primary_button(ui, "Open");
            });
        });

        assert!(
            has_fill(&shapes, tokens::BG_DISABLED.gamma_multiply(0.5)),
            "{shapes:?}"
        );
        assert!(
            !has_fill(&shapes, tokens::CORAL_BRIGHT.gamma_multiply(0.5)),
            "{shapes:?}"
        );
        assert!(
            has_text_color(&shapes, tokens::TEXT_SECONDARY),
            "{shapes:?}"
        );
    }

    #[test]
    fn a_text_button_keeps_its_side_padding() {
        let ctx = ready();
        let mut rect = Rect::NOTHING;
        let mut text_width = 0.0;
        ctx.run_ui(test_support::input(400.0, vec![]), |ui| {
            rect = text_button(ui, "Clear").rect;
            let font = FontId::new(
                tokens::SIZE_BODY,
                FontFamily::Name(crate::fonts::MEDIUM_FAMILY.into()),
            );
            text_width = ui
                .painter()
                .layout_no_wrap("Clear".to_owned(), font, tokens::TEXT_PRIMARY)
                .size()
                .x;
        })
        .drop_without_applying_deltas();
        assert!(
            rect.width() >= text_width + 2.0 * tokens::SPACING_M - 0.5,
            "{rect:?} is too narrow for {text_width}"
        );
    }

    #[test]
    fn a_text_button_draws_coral_text() {
        let ctx = ready();
        let shapes = shapes(&ctx, |ui| {
            text_button(ui, "Clear");
        });
        assert!(has_text_color(&shapes, tokens::CORAL_STRONG), "{shapes:?}");
    }

    #[test]
    fn a_toggle_click_flips_it() {
        let ctx = ready();
        let mut on = false;
        // The first frame gives the toggle its rect, so the click can find it.
        ctx.run_ui(test_support::input(400.0, vec![]), |ui| {
            toggle(ui, &mut on);
        })
        .drop_without_applying_deltas();
        let at = Pos2::new(18.0, 10.0);
        ctx.run_ui(test_support::input(400.0, test_support::click(at)), |ui| {
            toggle(ui, &mut on);
        })
        .drop_without_applying_deltas();
        assert!(on);
    }

    #[test]
    fn an_on_toggle_is_coral_with_the_knob_at_the_right() {
        let ctx = ready();
        let mut on = true;
        let shapes = shapes(&ctx, |ui| {
            toggle(ui, &mut on);
        });
        assert!(has_fill(&shapes, tokens::CORAL_BRIGHT), "{shapes:?}");
        let knob = circle(&shapes, tokens::BG_PRIMARY).expect("the on knob");
        assert!(knob.center().x > 18.0, "{knob:?} is not on the right");
    }

    #[test]
    fn an_off_toggle_is_raised_with_the_knob_at_the_left() {
        let ctx = ready();
        let mut on = false;
        let shapes = shapes(&ctx, |ui| {
            toggle(ui, &mut on);
        });
        assert!(has_fill(&shapes, tokens::SURFACE_HIGHLIGHT), "{shapes:?}");
        let knob = circle(&shapes, tokens::TEXT_SECONDARY).expect("the off knob");
        assert!(knob.center().x < 18.0, "{knob:?} is not on the left");
    }

    #[test]
    fn a_disabled_toggle_leaves_the_enabled_colours() {
        let ctx = ready();
        let mut on = false;
        let shapes = shapes(&ctx, |ui| {
            ui.add_enabled_ui(false, |ui| {
                toggle(ui, &mut on);
            });
        });
        assert!(
            has_fill(&shapes, tokens::BG_DISABLED.gamma_multiply(0.5)),
            "{shapes:?}"
        );
        assert!(
            !has_fill(&shapes, tokens::SURFACE_HIGHLIGHT.gamma_multiply(0.5)),
            "{shapes:?}"
        );
    }

    #[test]
    fn the_toggle_is_the_size_the_redesign_shows() {
        assert_eq!(TOGGLE_SIZE, vec2(36.0, 20.0));
        assert_eq!(TOGGLE_INSET + KNOB_DIAMETER + TOGGLE_INSET, TOGGLE_SIZE.y);
    }
}
