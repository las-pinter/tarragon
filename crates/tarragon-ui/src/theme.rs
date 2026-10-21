//! Theme installation and the shared panel chrome.
//!
//! The theme maps the stylesheet tokens onto egui visuals and text styles. The
//! panel helpers draw the title strip and the frame for every dock.

use egui::{
    Color32, Context, CornerRadius, FontFamily, FontId, Frame, Margin, RichText, TextStyle, Ui,
    Visuals, vec2,
};

use crate::fonts;
use crate::tokens;

/// Apply the theme tokens to the given context, and install the fonts.
pub fn apply_theme(ctx: &Context) {
    // Pin the dark theme, because `set_visuals` writes only the active slot.
    ctx.set_theme(egui::Theme::Dark);
    fonts::install(ctx);

    let mut visuals = Visuals::dark();
    visuals.window_fill = tokens::BG_SECONDARY;
    visuals.panel_fill = tokens::BG_PRIMARY;
    visuals.override_text_color = Some(tokens::TEXT_PRIMARY);
    visuals.weak_text_color = Some(tokens::TEXT_SECONDARY);
    visuals.window_corner_radius = CornerRadius::same(tokens::RADIUS_M);
    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = vec2(tokens::SPACING_S, tokens::SPACING_S);
        style.spacing.button_padding = vec2(tokens::SPACING_S, tokens::SPACING_XS);
        style.spacing.window_margin = Margin::same(points(tokens::SPACING_S));
        style.spacing.menu_margin = Margin::same(points(tokens::SPACING_XS));
        style.spacing.indent = tokens::SPACING_L;

        style.text_styles = [
            (TextStyle::Body, FontId::proportional(tokens::SIZE_BODY)),
            (TextStyle::Button, FontId::proportional(tokens::SIZE_BODY)),
            (
                TextStyle::Heading,
                FontId::proportional(tokens::SIZE_HEADING),
            ),
            (TextStyle::Small, FontId::proportional(tokens::SIZE_SMALL)),
            (TextStyle::Monospace, FontId::monospace(tokens::SIZE_LOG)),
        ]
        .into();

        let widgets = &mut style.visuals.widgets;
        widgets.noninteractive.bg_stroke.color = tokens::SEPARATOR;
        for (widget, fill, border) in [
            (
                &mut widgets.inactive,
                tokens::BG_TERTIARY,
                tokens::BORDER_SUBTLE,
            ),
            (
                &mut widgets.hovered,
                tokens::SURFACE_HOVER,
                tokens::BORDER_CARD,
            ),
            (
                &mut widgets.active,
                tokens::SURFACE_HIGHLIGHT,
                tokens::BORDER_INTERACTIVE,
            ),
        ] {
            widget.weak_bg_fill = fill;
            widget.bg_stroke.color = border;
            widget.fg_stroke.color = tokens::CORAL_STRONG;
            widget.corner_radius = CornerRadius::same(tokens::RADIUS_XS);
        }
    });
}

/// Draw one panel title strip: body size amber text on the tertiary fill.
pub(crate) fn panel_title(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(tokens::BG_TERTIARY)
        .inner_margin(Margin::symmetric(
            points(tokens::SPACING_S),
            points(tokens::SPACING_XS),
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(text)
                    .size(tokens::SIZE_BODY)
                    .color(tokens::AMBER_ACCENT)
                    .family(FontFamily::Name(fonts::MEDIUM_FAMILY.into())),
            );
        });
}

/// The frame for one panel: a fill colour and a small inset.
pub(crate) fn panel_frame(fill: Color32, margin: Margin) -> Frame {
    Frame::new().fill(fill).inner_margin(margin)
}

/// Convert a spacing token to an integer margin, in points.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Rust has no `TryFrom<f32>`, so the cast goes through a saturating `i32`"
)]
pub(crate) fn points(value: f32) -> i8 {
    // Saturate, so a token outside the i8 range cannot wrap to a wrong margin.
    i8::try_from(value as i32).unwrap_or(if value < 0.0 { i8::MIN } else { i8::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_theme_sets_the_type_scale() {
        let ctx = Context::default();
        apply_theme(&ctx);

        let style = ctx.style_of(egui::Theme::Dark);
        let size = |text: TextStyle| style.text_styles[&text].size;
        assert!((size(TextStyle::Body) - tokens::SIZE_BODY).abs() < f32::EPSILON);
        assert!((size(TextStyle::Button) - tokens::SIZE_BODY).abs() < f32::EPSILON);
        assert!((size(TextStyle::Heading) - tokens::SIZE_HEADING).abs() < f32::EPSILON);
        assert!((size(TextStyle::Small) - tokens::SIZE_SMALL).abs() < f32::EPSILON);
        assert!((size(TextStyle::Monospace) - tokens::SIZE_LOG).abs() < f32::EPSILON);
    }

    #[test]
    fn the_theme_sets_the_widget_tokens() {
        let ctx = Context::default();
        apply_theme(&ctx);

        let style = ctx.style_of(egui::Theme::Dark);
        let widgets = &style.visuals.widgets;
        assert_eq!(widgets.noninteractive.bg_stroke.color, tokens::SEPARATOR);
        for (widget, fill, border) in [
            (
                &widgets.inactive,
                tokens::BG_TERTIARY,
                tokens::BORDER_SUBTLE,
            ),
            (&widgets.hovered, tokens::SURFACE_HOVER, tokens::BORDER_CARD),
            (
                &widgets.active,
                tokens::SURFACE_HIGHLIGHT,
                tokens::BORDER_INTERACTIVE,
            ),
        ] {
            assert_eq!(widget.weak_bg_fill, fill);
            assert_eq!(widget.bg_stroke.color, border);
            assert_eq!(widget.fg_stroke.color, tokens::CORAL_STRONG);
            assert_eq!(widget.corner_radius, CornerRadius::same(tokens::RADIUS_XS));
        }
    }

    #[test]
    fn points_saturate_outside_the_margin_range() {
        assert_eq!(points(tokens::SPACING_S), 8);
        assert_eq!(points(1000.0), i8::MAX);
        assert_eq!(points(-1000.0), i8::MIN);
    }
}
