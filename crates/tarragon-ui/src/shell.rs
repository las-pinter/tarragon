//! Application shell: the window frame, the menu bar, and the dock panels.
//!
//! The frame uses plain egui panels with fixed sizes. A docking layout lands
//! later, so the panels cannot move or resize yet.

use egui::{
    Button, CentralPanel, Color32, Context, CornerRadius, FontFamily, FontId, Frame, Margin,
    MenuBar, Panel, RichText, TextStyle, Ui, ViewportCommand, Visuals, vec2,
};

use crate::fonts;
use crate::tokens;

/// The product name shown in the title bar.
pub const PRODUCT_NAME: &str = "Tarragon";

/// The Library dock name: the menu label, the panel id, and the panel title.
pub const DOCK_LIBRARY: &str = "Library";
/// The Gallery dock name: the menu label.
pub const DOCK_GALLERY: &str = "Gallery";
/// The Preview dock name: the menu label, the panel id, and the panel title.
pub const DOCK_PREVIEW: &str = "Preview";
/// The Log dock name: the menu label, the panel id, and the panel title.
pub const DOCK_LOG: &str = "Log";

/// Title bar height, in points.
const TITLEBAR_HEIGHT: f32 = 30.0;
/// Menu bar height, in points.
const MENUBAR_HEIGHT: f32 = 26.0;
/// Log panel share of the free workspace height.
const LOG_HEIGHT_FRACTION: f32 = 0.25;
/// Log panel minimum height, in points. `theme.css` sets a 150 px floor.
const LOG_HEIGHT_FLOOR: f32 = 150.0;

/// The window frame. It holds the panel flags and the default dock sizes.
#[expect(
    clippy::struct_excessive_bools,
    reason = "the four dock flags mirror the four View menu toggles"
)]
pub struct Shell {
    /// Whether the Library dock shows on the left.
    library_visible: bool,
    /// Whether the Gallery panel shows in the centre.
    gallery_visible: bool,
    /// Whether the Preview dock shows on the right.
    preview_visible: bool,
    /// Whether the Log panel shows under the gallery.
    log_visible: bool,
    /// Library dock width, in points.
    sidebar_width: f32,
    /// Preview dock width, in points.
    preview_width: f32,
}

impl Shell {
    /// Build the frame with the mockup defaults: the three panels shown and
    /// the log hidden.
    #[must_use]
    pub fn new() -> Self {
        Self {
            library_visible: true,
            gallery_visible: true,
            preview_visible: true,
            log_visible: false,
            sidebar_width: tokens::SIDEBAR_WIDTH,
            preview_width: tokens::PREVIEW_WIDTH,
        }
    }

    /// Library dock width, in points.
    #[must_use]
    pub fn sidebar_width(&self) -> f32 {
        self.sidebar_width
    }

    /// Preview dock width, in points.
    #[must_use]
    pub fn preview_width(&self) -> f32 {
        self.preview_width
    }

    /// Whether the Library dock is visible.
    #[must_use]
    pub fn library_visible(&self) -> bool {
        self.library_visible
    }

    /// Whether the Gallery panel is visible.
    #[must_use]
    pub fn gallery_visible(&self) -> bool {
        self.gallery_visible
    }

    /// Whether the Preview dock is visible.
    #[must_use]
    pub fn preview_visible(&self) -> bool {
        self.preview_visible
    }

    /// Whether the Log panel is visible.
    #[must_use]
    pub fn log_visible(&self) -> bool {
        self.log_visible
    }

    /// Show or hide the Log panel. Returns the new state.
    pub fn toggle_log(&mut self) -> bool {
        self.log_visible = !self.log_visible;
        self.log_visible
    }

    /// Restore the mockup defaults for the dock sizes and panel visibility.
    pub fn reset_layout(&mut self) {
        *self = Self::new();
    }

    /// Draw one frame of the shell into the given ui.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();

        Panel::top("tarragon-titlebar")
            .exact_size(TITLEBAR_HEIGHT)
            .resizable(false)
            .frame(panel_frame(tokens::BG_SECONDARY, Margin::symmetric(10, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new(PRODUCT_NAME)
                            .size(tokens::SIZE_HEADING)
                            .family(FontFamily::Name(fonts::BOLD_FAMILY.into())),
                    );
                });
            });

        Panel::top("tarragon-menubar")
            .exact_size(MENUBAR_HEIGHT)
            .resizable(false)
            .frame(panel_frame(tokens::BG_SECONDARY, Margin::symmetric(6, 0)))
            .show(ui, |ui| {
                MenuBar::new().ui(ui, |ui| self.menu_bar(ui, &ctx));
            });

        if self.library_visible {
            Panel::left(format!("tarragon-{DOCK_LIBRARY}"))
                .exact_size(self.sidebar_width)
                .resizable(false)
                .frame(panel_frame(tokens::BG_SECONDARY, Margin::same(8)))
                .show(ui, |ui| {
                    panel_title(ui, DOCK_LIBRARY);
                });
        }

        if self.preview_visible {
            Panel::right(format!("tarragon-{DOCK_PREVIEW}"))
                .exact_size(self.preview_width)
                .resizable(false)
                .frame(panel_frame(tokens::BG_PRIMARY, Margin::same(8)))
                .show(ui, |ui| {
                    panel_title(ui, DOCK_PREVIEW);
                });
        }

        // A bottom panel here spans the gallery width only, as the mockup shows.
        if self.log_visible {
            let log_height = (ui.available_height() * LOG_HEIGHT_FRACTION).max(LOG_HEIGHT_FLOOR);
            Panel::bottom(format!("tarragon-{DOCK_LOG}"))
                .exact_size(log_height)
                .resizable(false)
                .frame(panel_frame(tokens::BG_SECONDARY, Margin::same(8)))
                .show(ui, |ui| {
                    panel_title(ui, DOCK_LOG);
                });
        }

        if self.gallery_visible {
            // The gallery draws no title strip: its top strip holds the tabs.
            CentralPanel::default()
                .frame(panel_frame(tokens::BG_PRIMARY, Margin::same(8)))
                .show(ui, |_| {});
        }
    }

    /// Draw the File, View, and Settings menus. The items are placeholders.
    fn menu_bar(&mut self, ui: &mut Ui, ctx: &Context) {
        ui.menu_button("File", |ui| {
            // The folder picker lands with the scan work.
            let _ = ui.button("Open Folder...");
            if ui.button("Quit").clicked() {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                // The menu must close before the window goes away.
                ui.close();
            }
        });
        ui.menu_button("View", |ui| {
            toggle_row(ui, DOCK_LIBRARY, &mut self.library_visible);
            toggle_row(ui, DOCK_GALLERY, &mut self.gallery_visible);
            toggle_row(ui, DOCK_PREVIEW, &mut self.preview_visible);
            toggle_row(ui, DOCK_LOG, &mut self.log_visible);
            ui.separator();
            if ui.button("Reset Window Layout").clicked() {
                self.reset_layout();
            }
        });
        ui.menu_button("Settings", |ui| {
            // The settings screens land with the settings work.
            let _ = ui.button("Preferences...");
        });
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

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

/// Draw one menu toggle row and flip the flag when the row is clicked.
fn toggle_row(ui: &mut Ui, label: &str, visible: &mut bool) {
    if ui.add(Button::new(label).selected(*visible)).clicked() {
        *visible = !*visible;
    }
}

/// Draw one panel title strip: body size amber text on the tertiary fill.
fn panel_title(ui: &mut Ui, text: &str) {
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
fn panel_frame(fill: Color32, margin: Margin) -> Frame {
    Frame::new().fill(fill).inner_margin(margin)
}

/// Convert a spacing token to an integer margin, in points.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Rust has no `TryFrom<f32>`, so the cast goes through a saturating `i32`"
)]
fn points(value: f32) -> i8 {
    // Saturate, so a token outside the i8 range cannot wrap to a wrong margin.
    i8::try_from(value as i32).unwrap_or(if value < 0.0 { i8::MIN } else { i8::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{FontId, Id, PanelState, Pos2, RawInput, Rect};

    /// The headless screen, wide and tall enough for both docks.
    fn input() -> RawInput {
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
            ..Default::default()
        }
    }

    /// Draw one headless frame.
    fn draw(ctx: &Context, shell: &mut Shell) {
        ctx.run_ui(input(), |ui| shell.show(ui))
            .drop_without_applying_deltas();
    }

    /// The stored state of one panel from the last frame.
    fn panel(ctx: &Context, id: &str) -> Option<PanelState> {
        PanelState::load(ctx, Id::new(id))
    }

    #[test]
    fn the_shell_draws_two_frames_after_the_theme_applies() {
        let ctx = Context::default();
        apply_theme(&ctx);

        ctx.run_ui(RawInput::default(), |_| {})
            .drop_without_applying_deltas();

        let mut shell = Shell::new();
        let mut drawn = 0;
        for _ in 0..2 {
            let output = ctx.run_ui(RawInput::default(), |ui| shell.show(ui));
            drawn = output.shapes.len();
            output.drop_without_applying_deltas();
        }

        assert!(drawn > 0, "the shell drew nothing");

        // The theme installs the Roboto fonts, so plain text shapes one glyph
        // per letter.
        let galley = ctx.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                "Tarragon".to_owned(),
                FontId::proportional(tokens::SIZE_BODY),
                tokens::TEXT_PRIMARY,
            )
        });
        let glyphs: usize = galley.rows.iter().map(|row| row.row.glyphs.len()).sum();
        assert_eq!(glyphs, 8, "the theme must install Roboto");
    }

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
    fn the_docks_draw_at_the_token_widths() {
        let ctx = Context::default();
        apply_theme(&ctx);

        let mut shell = Shell::new();
        draw(&ctx, &mut shell);

        let library = panel(&ctx, "tarragon-Library").expect("the library dock is drawn");
        let preview = panel(&ctx, "tarragon-Preview").expect("the preview dock is drawn");
        assert!(
            (library.size().x - tokens::SIDEBAR_WIDTH).abs() < 0.5,
            "library is {} points wide",
            library.size().x
        );
        assert!(
            (preview.size().x - tokens::PREVIEW_WIDTH).abs() < 0.5,
            "preview is {} points wide",
            preview.size().x
        );
    }

    #[test]
    fn the_log_toggle_changes_whether_the_panel_is_drawn() {
        let ctx = Context::default();
        apply_theme(&ctx);
        let mut shell = Shell::new();

        // Clear the stored state, so a hidden panel cannot leave a trace.
        let drawn = |ctx: &Context, shell: &mut Shell| {
            ctx.data_mut(|data| data.remove::<PanelState>(Id::new("tarragon-Log")));
            draw(ctx, shell);
            panel(ctx, "tarragon-Log").is_some()
        };

        assert!(!drawn(&ctx, &mut shell), "the log panel starts hidden");

        assert!(shell.toggle_log(), "the first toggle shows the log");
        assert!(drawn(&ctx, &mut shell), "the log panel is drawn");

        assert!(!shell.toggle_log(), "the second toggle hides the log");
        assert!(!drawn(&ctx, &mut shell), "the log panel is hidden again");
    }

    #[test]
    fn the_log_panel_starts_hidden_and_the_view_toggle_flips_it() {
        let mut shell = Shell::new();
        assert!(!shell.log_visible(), "the log panel starts hidden");

        assert!(shell.toggle_log(), "the first toggle shows the log");
        assert!(shell.log_visible(), "the log panel stays visible");

        assert!(!shell.toggle_log(), "the second toggle hides the log");
        assert!(!shell.log_visible(), "the log panel is hidden again");
    }

    #[test]
    fn the_dock_widths_start_at_the_token_values() {
        let shell = Shell::new();

        assert!((shell.sidebar_width() - tokens::SIDEBAR_WIDTH).abs() < f32::EPSILON);
        assert!((shell.preview_width() - tokens::PREVIEW_WIDTH).abs() < f32::EPSILON);
    }

    #[test]
    fn reset_layout_restores_the_defaults() {
        let mut shell = Shell::new();
        shell.toggle_log();
        shell.library_visible = false;
        shell.gallery_visible = false;
        shell.preview_visible = false;

        shell.reset_layout();

        assert!(shell.library_visible(), "library returns");
        assert!(shell.gallery_visible(), "gallery returns");
        assert!(shell.preview_visible(), "preview returns");
        assert!(!shell.log_visible(), "log hides again");
    }
}
