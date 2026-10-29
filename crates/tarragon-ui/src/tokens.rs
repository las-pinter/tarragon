//! Theme tokens: palette, spacing, radii, type scale, and motion values.
//!
//! The values come from the `:root` block of `design/mockups/theme.css`. Each
//! doc comment names its stylesheet token, so the two stay traceable. The two
//! exceptions are `BUCKET_PINK`, `CHECKER_LIGHT`, `CHECKER_DARK`, and
//! `WINDOW_SIZE`, which have no stylesheet row. Colours convert to
//! `egui::Color32` at compile time.

use egui::Color32;

// The whole `:root` block of `theme.css` is ported on purpose, plus
// `BUCKET_PINK`, the checker greys and `WINDOW_SIZE`. Some values have no
// consumer yet.

/// `--bg-primary`: window and gallery background.
pub const BG_PRIMARY: Color32 = Color32::from_rgb(0x16, 0x15, 0x1a);
/// `--bg-secondary`: window chrome and side panels.
pub const BG_SECONDARY: Color32 = Color32::from_rgb(0x1c, 0x1b, 0x22);
/// `--bg-tertiary`: rail and input background.
pub const BG_TERTIARY: Color32 = Color32::from_rgb(0x21, 0x1f, 0x29);
/// `--surface-highlight`: pressed and highlighted surface.
pub const SURFACE_HIGHLIGHT: Color32 = Color32::from_rgb(0x3d, 0x3b, 0x44);
/// `--surface-hover`: hovered surface.
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x2a, 0x28, 0x36);

/// `--coral-strong`: strong coral accent.
pub const CORAL_STRONG: Color32 = Color32::from_rgb(0xf0, 0x99, 0x7b);
/// `--coral-muted`: muted coral accent.
pub const CORAL_MUTED: Color32 = Color32::from_rgb(0xd8, 0x5a, 0x30);
/// `--coral-dark`: deep coral fill.
pub const CORAL_DARK: Color32 = Color32::from_rgb(0x4a, 0x1b, 0x0c);
/// `--coral-bright`: bright coral accent.
pub const CORAL_BRIGHT: Color32 = Color32::from_rgb(0xe0, 0x65, 0x40);
/// `--amber-accent`: filter swatch accent.
pub const AMBER_ACCENT: Color32 = Color32::from_rgb(0xfa, 0xc7, 0x75);
/// `--amber-light`: light amber.
pub const AMBER_LIGHT: Color32 = Color32::from_rgb(0xff, 0xd4, 0x80);
/// `--amber-dark`: amber fill behind text.
pub const AMBER_DARK: Color32 = Color32::from_rgb(0x41, 0x24, 0x02);

/// `--text-primary`: primary text.
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xec, 0xe9, 0xf2);
/// `--text-secondary`: secondary text.
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x8d, 0x8a, 0x98);
/// `--text-tertiary`: tertiary text.
pub const TEXT_TERTIARY: Color32 = Color32::from_rgb(0xa8, 0xa5, 0xb0);
/// `--text-muted`: muted text.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x65, 0x62, 0x6f);

/// `--border-subtle`: faint 6% white border.
pub const BORDER_SUBTLE: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 15);
/// `--border-card`: 8% white border.
pub const BORDER_CARD: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 20);
/// `--border-interactive`: 12% white border.
pub const BORDER_INTERACTIVE: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 31);
/// `--border-inactive`: inactive border.
pub const BORDER_INACTIVE: Color32 = Color32::from_rgb(0x55, 0x55, 0x55);
/// `--border-disabled`: disabled border.
pub const BORDER_DISABLED: Color32 = Color32::from_rgb(0x3a, 0x38, 0x45);

/// `--bg-disabled`: disabled background.
pub const BG_DISABLED: Color32 = Color32::from_rgb(0x1a, 0x18, 0x20);
/// `--bg-log-panel`: log line background.
pub const BG_LOG_PANEL: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x2e);
/// `--highlight-disabled`: disabled highlight.
pub const HIGHLIGHT_DISABLED: Color32 = Color32::from_rgb(0x4a, 0x3a, 0x35);
/// `--separator`: separator line.
pub const SEPARATOR: Color32 = Color32::from_rgb(0x1e, 0x1d, 0x23);

/// `--badge-bg-vinous`: vinous badge fill.
pub const BADGE_BG_VINOUS: Color32 = Color32::from_rgb(0x4a, 0x1b, 0x0c);
/// `--badge-fg-vinous`: vinous badge text.
pub const BADGE_FG_VINOUS: Color32 = Color32::from_rgb(0xf0, 0x99, 0x7b);
/// `--badge-bg-sage`: sage badge fill.
pub const BADGE_BG_SAGE: Color32 = Color32::from_rgb(0x1a, 0x3a, 0x2a);
/// `--badge-fg-sage`: sage badge text.
pub const BADGE_FG_SAGE: Color32 = Color32::from_rgb(0x7b, 0xc8, 0x8f);
/// `--badge-bg-navy`: navy badge fill.
pub const BADGE_BG_NAVY: Color32 = Color32::from_rgb(0x1a, 0x2a, 0x3a);
/// `--badge-fg-navy`: navy badge text.
pub const BADGE_FG_NAVY: Color32 = Color32::from_rgb(0x7b, 0xa8, 0xc8);
/// `--badge-bg-umber`: umber badge fill.
pub const BADGE_BG_UMBER: Color32 = Color32::from_rgb(0x3a, 0x2a, 0x1a);
/// `--badge-fg-umber`: umber badge text.
pub const BADGE_FG_UMBER: Color32 = Color32::from_rgb(0xc8, 0xa8, 0x7b);
/// `--badge-bg-plum`: plum badge fill.
pub const BADGE_BG_PLUM: Color32 = Color32::from_rgb(0x2a, 0x1a, 0x3a);
/// `--badge-fg-plum`: plum badge text.
pub const BADGE_FG_PLUM: Color32 = Color32::from_rgb(0xa8, 0x7b, 0xc8);
/// `--badge-bg-teal`: teal badge fill.
pub const BADGE_BG_TEAL: Color32 = Color32::from_rgb(0x1a, 0x3a, 0x3a);
/// `--badge-fg-teal`: teal badge text.
pub const BADGE_FG_TEAL: Color32 = Color32::from_rgb(0x7b, 0xc8, 0xc8);
/// `--badge-bg-olive`: olive badge fill.
pub const BADGE_BG_OLIVE: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x1a);
/// `--badge-fg-olive`: olive badge text.
pub const BADGE_FG_OLIVE: Color32 = Color32::from_rgb(0xc8, 0xc8, 0x7b);
/// `--badge-bg-azure`: azure badge fill.
pub const BADGE_BG_AZURE: Color32 = Color32::from_rgb(0x2e, 0x86, 0xab);
/// `--badge-fg-azure`: azure badge text.
pub const BADGE_FG_AZURE: Color32 = Color32::from_rgb(0xd4, 0xee, 0xf7);
/// `--badge-bg-neutral`: neutral badge fill.
pub const BADGE_BG_NEUTRAL: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
/// `--badge-fg-neutral`: neutral badge text.
pub const BADGE_FG_NEUTRAL: Color32 = Color32::from_rgb(0xa8, 0xa5, 0xb0);

/// `--bucket-red`: red colour bucket.
pub const BUCKET_RED: Color32 = Color32::from_rgb(0xe7, 0x4c, 0x3c);
/// `--bucket-orange`: orange colour bucket.
pub const BUCKET_ORANGE: Color32 = Color32::from_rgb(0xf3, 0x9c, 0x12);
/// `--bucket-yellow`: yellow colour bucket.
pub const BUCKET_YELLOW: Color32 = Color32::from_rgb(0xf1, 0xc4, 0x0f);
/// `--bucket-green`: green colour bucket.
pub const BUCKET_GREEN: Color32 = Color32::from_rgb(0x27, 0xae, 0x60);
/// `--bucket-teal`: teal colour bucket.
pub const BUCKET_TEAL: Color32 = Color32::from_rgb(0x1a, 0xbc, 0x9c);
/// `--bucket-cyan`: cyan colour bucket.
pub const BUCKET_CYAN: Color32 = Color32::from_rgb(0x00, 0xbc, 0xd4);
/// `--bucket-blue`: blue colour bucket.
pub const BUCKET_BLUE: Color32 = Color32::from_rgb(0x34, 0x98, 0xdb);
/// `--bucket-purple`: purple colour bucket.
pub const BUCKET_PURPLE: Color32 = Color32::from_rgb(0x9b, 0x59, 0xb6);
/// `--bucket-magenta`: magenta colour bucket.
pub const BUCKET_MAGENTA: Color32 = Color32::from_rgb(0xe9, 0x1e, 0x63);
/// `--bucket-pink`: pink colour bucket. The stylesheet has no pink row.
pub const BUCKET_PINK: Color32 = Color32::from_rgb(0xff, 0x80, 0xab);
/// `--bucket-neutral`: neutral colour bucket.
pub const BUCKET_NEUTRAL: Color32 = Color32::from_rgb(0x7f, 0x8c, 0x8d);

/// Light cell of the checkerboard in the transparent swatch. No stylesheet row.
pub const CHECKER_LIGHT: Color32 = Color32::from_rgb(0xb0, 0xb0, 0xb0);
/// Dark cell of the checkerboard in the transparent swatch. No stylesheet row.
pub const CHECKER_DARK: Color32 = Color32::from_rgb(0x70, 0x70, 0x70);

/// `--space-xs`: extra small gap, in points.
pub const SPACING_XS: f32 = 4.0;
/// `--space-s`: small gap, in points.
pub const SPACING_S: f32 = 8.0;
/// `--space-m`: medium gap, in points.
pub const SPACING_M: f32 = 12.0;
/// `--space-l`: large gap, in points.
pub const SPACING_L: f32 = 16.0;
/// `--space-xl`: extra large gap, in points.
pub const SPACING_XL: f32 = 24.0;

/// `--radius-none`: no rounding, in points.
pub const RADIUS_NONE: u8 = 0;
/// `--radius-xs`: extra small rounding, in points.
pub const RADIUS_XS: u8 = 3;
/// The width of a control border, in points.
pub const STROKE_WIDTH: f32 = 1.0;
/// `--radius-s`: small rounding, in points.
pub const RADIUS_S: u8 = 4;
/// `--radius-m`: medium rounding, in points.
pub const RADIUS_M: u8 = 6;
/// `--radius-l`: large rounding, in points.
pub const RADIUS_L: u8 = 8;
/// `--radius-xl`: extra large rounding, in points.
pub const RADIUS_XL: u8 = 10;

/// `--dur-fast`: fast animation, in seconds.
pub const DURATION_FAST: f32 = 0.15;
/// `--dur-normal`: normal animation, in seconds.
pub const DURATION_NORMAL: f32 = 0.2;

/// `--grid-gap`: gap between grid cells, in points.
pub const GRID_GAP: f32 = 14.0;
/// `--thumbnail`: grid cell size, in points.
pub const THUMBNAIL_SIZE: f32 = 160.0;
/// Default window size in points. The dock split shares are computed for its
/// width, so the app and the layout must read the same value.
pub const WINDOW_SIZE: [f32; 2] = [1280.0, 800.0];
/// `--sidebar-width`: default Library dock width, in points.
pub const SIDEBAR_WIDTH: f32 = 220.0;
/// `--preview-width`: default Preview dock width, in points.
pub const PREVIEW_WIDTH: f32 = 490.0;
/// `.preview-img` min-height: the Preview image box height, in points.
pub const PREVIEW_IMAGE_HEIGHT: f32 = 260.0;

/// `--font-sans`: stylesheet sans stack.
///
/// Recorded for traceability and not applied. The interface uses the vendored
/// Roboto families instead.
pub const FONT_SANS: &str = "\"Segoe UI\", -apple-system, sans-serif";
/// `--font-mono`: stylesheet mono stack.
///
/// Recorded for traceability and not applied. Roboto Regular serves the
/// Monospace family until a monospace face is vendored.
pub const FONT_MONO: &str = "ui-monospace, \"Consolas\", \"Courier New\", monospace";
/// `--size-body`: body text size, in points.
pub const SIZE_BODY: f32 = 12.0;
/// `--size-heading`: heading text size, in points.
pub const SIZE_HEADING: f32 = 16.0;
/// `--size-small`: small text size, in points.
pub const SIZE_SMALL: f32 = 10.0;
/// `--size-log`: log line text size, in points.
pub const SIZE_LOG: f32 = 11.0;
/// `--size-caption`: caption text size, in points.
pub const SIZE_CAPTION: f32 = 11.0;
/// `--weight-regular`: regular weight. egui picks the weight by family.
pub const WEIGHT_REGULAR: f32 = 400.0;
/// `--weight-medium`: medium weight. egui picks the weight by family.
pub const WEIGHT_MEDIUM: f32 = 500.0;
/// `--weight-semibold`: semibold weight. egui picks the weight by family.
pub const WEIGHT_SEMIBOLD: f32 = 600.0;

#[cfg(test)]
mod tests;
