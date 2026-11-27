//! Theme installation and the shared panel chrome.
//!
//! The theme maps the design tokens onto egui visuals and text styles. The
//! panel helpers style the title strip and the frame for every dock.

use egui::style::ScrollStyle;
use egui::{
    Color32, Context, CornerRadius, FontId, Frame, Margin, Shadow, Stroke, TextStyle, Visuals, vec2,
};

use crate::fonts;
use crate::tokens;

/// The width of a solid scroll bar, in points. The redesign shows no scroll
/// bar; the slim bar keeps it out of the way.
const SCROLL_BAR_WIDTH: f32 = 6.0;
/// The least length of a scroll handle, in points.
const SCROLL_HANDLE_MIN: f32 = 24.0;

/// Apply the theme tokens to the given context, and install the fonts.
///
/// Flat surfaces: no shadows, no border between panels, and depth from the
/// five surface steps. Menus and popups take the menu fill, which is
/// `window_fill`; dialogs set their own frame with `dialog_frame`.
pub fn apply_theme(ctx: &Context) {
    // Pin the dark theme, because `set_visuals` writes only the active slot.
    ctx.set_theme(egui::Theme::Dark);
    fonts::install(ctx);

    let mut visuals = Visuals::dark();
    // Menus and popups draw their frame from these three values.
    visuals.window_fill = tokens::SURFACE_HOVER;
    visuals.window_stroke = Stroke::NONE;
    visuals.window_shadow = Shadow::NONE;
    visuals.popup_shadow = Shadow::NONE;
    visuals.window_corner_radius = CornerRadius::same(tokens::RADIUS_XL);
    visuals.menu_corner_radius = CornerRadius::same(tokens::RADIUS_XL);
    visuals.panel_fill = tokens::BG_PRIMARY;
    visuals.override_text_color = Some(tokens::TEXT_PRIMARY);
    visuals.weak_text_color = Some(tokens::TEXT_SECONDARY);
    visuals.extreme_bg_color = tokens::BG_TERTIARY;
    visuals.text_edit_bg_color = Some(tokens::BG_TERTIARY);
    visuals.code_bg_color = tokens::BG_TERTIARY;
    visuals.faint_bg_color = tokens::BG_SECONDARY;
    visuals.hyperlink_color = tokens::CORAL_STRONG;
    visuals.warn_fg_color = tokens::AMBER_ACCENT;
    visuals.error_fg_color = tokens::BUCKET_RED;
    // A text field draws the selection stroke as its focus ring, and a chosen
    // option draws it as its text color. The light coral passes the contrast
    // bar on the deep coral fill, so one value serves both.
    visuals.selection.bg_fill = tokens::CORAL_DARK;
    visuals.selection.stroke = Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG);
    ctx.set_visuals(visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = vec2(tokens::SPACING_S, tokens::SPACING_S);
        style.spacing.button_padding = vec2(tokens::SPACING_M, tokens::SPACING_S);
        style.spacing.window_margin = Margin::same(points(tokens::SPACING_M));
        style.spacing.menu_margin = Margin::same(points(tokens::MENU_PADDING));
        style.spacing.indent = tokens::SPACING_L;
        style.spacing.scroll = scroll_style();

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
        // The noninteractive stroke draws separators: the raised step.
        widgets.noninteractive.bg_fill = tokens::BG_SECONDARY;
        widgets.noninteractive.weak_bg_fill = tokens::BG_SECONDARY;
        widgets.noninteractive.bg_stroke =
            Stroke::new(tokens::STROKE_WIDTH, tokens::SURFACE_HIGHLIGHT);
        widgets.noninteractive.fg_stroke = Stroke::new(tokens::STROKE_WIDTH, tokens::TEXT_PRIMARY);
        widgets.noninteractive.corner_radius = CornerRadius::same(tokens::RADIUS_M);
        for (widget, bg_fill, weak_bg_fill) in [
            (
                &mut widgets.inactive,
                tokens::BG_TERTIARY,
                tokens::BG_TERTIARY,
            ),
            (
                &mut widgets.hovered,
                tokens::SURFACE_HIGHLIGHT,
                tokens::SURFACE_HOVER,
            ),
            (
                &mut widgets.active,
                tokens::SURFACE_HIGHLIGHT,
                tokens::SURFACE_HIGHLIGHT,
            ),
            (
                &mut widgets.open,
                tokens::SURFACE_HIGHLIGHT,
                tokens::SURFACE_HIGHLIGHT,
            ),
        ] {
            widget.bg_fill = bg_fill;
            widget.weak_bg_fill = weak_bg_fill;
            // Flat controls draw no border; their fill tells their state.
            widget.bg_stroke = Stroke::NONE;
            // The foreground stroke paints checks and slider grabs, so coral
            // marks the action there.
            widget.fg_stroke = Stroke::new(tokens::STROKE_WIDTH, tokens::CORAL_BRIGHT);
            widget.corner_radius = CornerRadius::same(tokens::RADIUS_L);
            widget.expansion = 0.0;
        }
    });
}

/// The solid scroll bars the flat look uses: a slim track with a raised
/// handle, and no floating overlay.
fn scroll_style() -> ScrollStyle {
    let mut scroll = ScrollStyle::solid();
    scroll.bar_width = SCROLL_BAR_WIDTH;
    scroll.handle_min_length = SCROLL_HANDLE_MIN;
    scroll.bar_inner_margin = tokens::SPACING_XS / 2.0;
    scroll.bar_outer_margin = 0.0;
    scroll.foreground_color = false;
    scroll
}

/// The frame of a dialog window: the panel fill, a large radius, and no
/// shadow. Menus and popups take the menu fill from the visuals instead.
pub(crate) fn dialog_frame() -> Frame {
    Frame::new()
        .fill(tokens::BG_SECONDARY)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(tokens::RADIUS_XL))
        .inner_margin(Margin::same(points(tokens::SPACING_S)))
        .shadow(Shadow::NONE)
}

/// Style the dock tab bar as the panel title strip: tertiary fill, the strip
/// padding, and no boxes or lines around the tab.
///
/// This style sets no text color. The theme's `override_text_color` wins over
/// it, so the title color comes from the title text itself in the dock tab
/// viewer.
///
/// `text_height` is the title text size; the bar adds the strip padding. The
/// 8 px side padding is fixed inside `egui_dock` and equals `SPACING_S`, so it
/// is not set here.
pub(crate) fn dock_title_style(style: &mut egui_dock::Style, text_height: f32) {
    // The design draws no border between two panels. The separator keeps its
    // grab area and shows while the pointer hovers it or drags it.
    style.separator.color_idle = Color32::TRANSPARENT;
    let bar = &mut style.tab_bar;
    bar.bg_fill = tokens::BG_TERTIARY;
    bar.height = text_height + 2.0 * tokens::SPACING_XS;
    bar.corner_radius = CornerRadius::ZERO;
    // The line under the bar would show as a separator the mockup lacks.
    bar.hline_color = tokens::BG_TERTIARY;

    let tab = &mut style.tab;
    for state in [
        &mut tab.active,
        &mut tab.inactive,
        &mut tab.focused,
        &mut tab.hovered,
        &mut tab.active_with_kb_focus,
        &mut tab.inactive_with_kb_focus,
        &mut tab.focused_with_kb_focus,
    ] {
        state.bg_fill = tokens::BG_TERTIARY;
        state.outline_color = Color32::TRANSPARENT;
        state.corner_radius = CornerRadius::ZERO;
    }
    // The panels own their insets and borders, so the tab body adds none.
    tab.tab_body.inner_margin = Margin::ZERO;
    tab.tab_body.stroke = Stroke::NONE;
}

/// The frame for one panel: a fill color and a small inset.
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
    fn the_theme_has_no_shadows() {
        let ctx = Context::default();
        apply_theme(&ctx);

        let visuals = &ctx.style_of(egui::Theme::Dark).visuals;
        assert_eq!(visuals.window_shadow, Shadow::NONE);
        assert_eq!(visuals.popup_shadow, Shadow::NONE);
        assert_eq!(visuals.window_stroke, Stroke::NONE);
        assert_eq!(
            visuals.window_corner_radius,
            CornerRadius::same(tokens::RADIUS_XL)
        );
        assert_eq!(
            visuals.menu_corner_radius,
            CornerRadius::same(tokens::RADIUS_XL)
        );
    }

    #[test]
    fn the_theme_sets_flat_widget_tokens() {
        let ctx = Context::default();
        apply_theme(&ctx);

        let style = ctx.style_of(egui::Theme::Dark);
        let widgets = &style.visuals.widgets;
        assert_eq!(
            widgets.noninteractive.bg_stroke,
            Stroke::new(tokens::STROKE_WIDTH, tokens::SURFACE_HIGHLIGHT)
        );
        for (widget, bg_fill, weak_bg_fill) in [
            (&widgets.inactive, tokens::BG_TERTIARY, tokens::BG_TERTIARY),
            (
                &widgets.hovered,
                tokens::SURFACE_HIGHLIGHT,
                tokens::SURFACE_HOVER,
            ),
            (
                &widgets.active,
                tokens::SURFACE_HIGHLIGHT,
                tokens::SURFACE_HIGHLIGHT,
            ),
            (
                &widgets.open,
                tokens::SURFACE_HIGHLIGHT,
                tokens::SURFACE_HIGHLIGHT,
            ),
        ] {
            assert_eq!(widget.bg_fill, bg_fill);
            assert_eq!(widget.weak_bg_fill, weak_bg_fill);
            assert_eq!(widget.bg_stroke, Stroke::NONE);
            assert_eq!(
                widget.fg_stroke,
                Stroke::new(tokens::STROKE_WIDTH, tokens::CORAL_BRIGHT)
            );
            assert_eq!(widget.corner_radius, CornerRadius::same(tokens::RADIUS_L));
            assert_eq!(widget.expansion, 0.0);
        }
    }

    #[test]
    fn a_text_field_gets_a_coral_focus_ring() {
        let ctx = crate::test_support::context();
        apply_theme(&ctx);
        let id = egui::Id::new("focus-field");
        let mut text = String::new();
        // The first frame gives the field its rect and asks for focus.
        ctx.run_ui(crate::test_support::input(400.0, vec![]), |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(id))
                .request_focus();
        })
        .drop_without_applying_deltas();
        let output = ctx.run_ui(crate::test_support::input(400.0, vec![]), |ui| {
            ui.add(egui::TextEdit::singleline(&mut text).id(id));
        });
        let ring = output.shapes.iter().any(|clipped| {
            matches!(
                &clipped.shape,
                egui::epaint::Shape::Rect(rect)
                    if rect.stroke == Stroke::new(tokens::FOCUS_RING_WIDTH, tokens::CORAL_STRONG)
                        && rect.fill == tokens::BG_TERTIARY
            )
        });
        output.drop_without_applying_deltas();
        assert!(ring, "the focused field has no coral ring");
    }

    #[test]
    fn a_chosen_option_takes_the_light_coral_text() {
        let ctx = Context::default();
        apply_theme(&ctx);

        let visuals = &ctx.style_of(egui::Theme::Dark).visuals;
        assert_eq!(visuals.selection.bg_fill, tokens::CORAL_DARK);
        assert_eq!(visuals.selection.stroke.color, tokens::CORAL_STRONG);
    }

    #[test]
    fn a_dialog_takes_the_panel_fill_and_no_shadow() {
        let ctx = Context::default();
        apply_theme(&ctx);

        let frame = dialog_frame();
        assert_eq!(frame.fill, tokens::BG_SECONDARY);
        assert_eq!(frame.stroke, Stroke::NONE);
        assert_eq!(frame.corner_radius, CornerRadius::same(tokens::RADIUS_XL));
        assert_eq!(frame.shadow, Shadow::NONE);
    }

    #[test]
    fn points_saturate_outside_the_margin_range() {
        assert_eq!(points(tokens::SPACING_S), 8);
        assert_eq!(points(1000.0), i8::MAX);
        assert_eq!(points(-1000.0), i8::MIN);
    }
}
