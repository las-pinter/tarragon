//! The ported tokens against the stylesheet they come from.

use std::collections::BTreeSet;

use super::*;

/// The stylesheet that defines the tokens.
const THEME_CSS: &str = include_str!("../../../../design/mockups/theme.css");

/// One ported token value.
enum Token {
    Color(Color32),
    Points(f32),
    Seconds(f32),
    Number(f32),
    Text(&'static str),
}

/// The `name: value` pairs of the `:root` block.
fn stylesheet_tokens() -> Vec<(String, String)> {
    let start = THEME_CSS
        .find(":root {")
        .expect("the stylesheet has a :root block");
    let block = &THEME_CSS[start..];
    let block = &block[..block.find('}').expect("the :root block closes")];
    block
        .lines()
        .filter_map(|line| line.trim().strip_prefix("--"))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| {
            (
                format!("--{name}"),
                value.trim().trim_end_matches(';').to_owned(),
            )
        })
        .collect()
}

/// Read a number that may carry a unit suffix.
fn number(value: &str, unit: &str) -> f32 {
    value
        .strip_suffix(unit)
        .unwrap_or(value)
        .parse()
        .unwrap_or_else(|_| panic!("{value} is not a number"))
}

/// Read `#rrggbb` or `rgba(r, g, b, a)` into a colour.
fn color(value: &str) -> Color32 {
    if let Some(hex) = value.strip_prefix('#') {
        let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).expect("a hex byte");
        assert_eq!(hex.len(), 6, "{value} is not #rrggbb");
        return Color32::from_rgb(byte(0), byte(2), byte(4));
    }
    let inner = value
        .strip_prefix("rgba(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("{value} is not a colour"));
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let channel = |at: usize| parts[at].parse::<u8>().expect("a channel");
    let alpha: f32 = parts[3].parse().expect("an alpha");
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let alpha = (alpha * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(channel(0), channel(1), channel(2), alpha)
}

/// Every ported token, named by its stylesheet variable.
fn ported_tokens() -> Vec<(&'static str, Token)> {
    vec![
        ("--bg-primary", Token::Color(BG_PRIMARY)),
        ("--bg-secondary", Token::Color(BG_SECONDARY)),
        ("--bg-tertiary", Token::Color(BG_TERTIARY)),
        ("--surface-highlight", Token::Color(SURFACE_HIGHLIGHT)),
        ("--surface-hover", Token::Color(SURFACE_HOVER)),
        ("--coral-strong", Token::Color(CORAL_STRONG)),
        ("--coral-muted", Token::Color(CORAL_MUTED)),
        ("--coral-dark", Token::Color(CORAL_DARK)),
        ("--coral-bright", Token::Color(CORAL_BRIGHT)),
        ("--amber-accent", Token::Color(AMBER_ACCENT)),
        ("--amber-light", Token::Color(AMBER_LIGHT)),
        ("--amber-dark", Token::Color(AMBER_DARK)),
        ("--text-primary", Token::Color(TEXT_PRIMARY)),
        ("--text-secondary", Token::Color(TEXT_SECONDARY)),
        ("--text-tertiary", Token::Color(TEXT_TERTIARY)),
        ("--text-muted", Token::Color(TEXT_MUTED)),
        ("--border-subtle", Token::Color(BORDER_SUBTLE)),
        ("--border-card", Token::Color(BORDER_CARD)),
        ("--border-interactive", Token::Color(BORDER_INTERACTIVE)),
        ("--border-inactive", Token::Color(BORDER_INACTIVE)),
        ("--border-disabled", Token::Color(BORDER_DISABLED)),
        ("--bg-disabled", Token::Color(BG_DISABLED)),
        ("--bg-log-panel", Token::Color(BG_LOG_PANEL)),
        ("--highlight-disabled", Token::Color(HIGHLIGHT_DISABLED)),
        ("--separator", Token::Color(SEPARATOR)),
        ("--badge-bg-vinous", Token::Color(BADGE_BG_VINOUS)),
        ("--badge-fg-vinous", Token::Color(BADGE_FG_VINOUS)),
        ("--badge-bg-sage", Token::Color(BADGE_BG_SAGE)),
        ("--badge-fg-sage", Token::Color(BADGE_FG_SAGE)),
        ("--badge-bg-navy", Token::Color(BADGE_BG_NAVY)),
        ("--badge-fg-navy", Token::Color(BADGE_FG_NAVY)),
        ("--badge-bg-umber", Token::Color(BADGE_BG_UMBER)),
        ("--badge-fg-umber", Token::Color(BADGE_FG_UMBER)),
        ("--badge-bg-plum", Token::Color(BADGE_BG_PLUM)),
        ("--badge-fg-plum", Token::Color(BADGE_FG_PLUM)),
        ("--badge-bg-teal", Token::Color(BADGE_BG_TEAL)),
        ("--badge-fg-teal", Token::Color(BADGE_FG_TEAL)),
        ("--badge-bg-olive", Token::Color(BADGE_BG_OLIVE)),
        ("--badge-fg-olive", Token::Color(BADGE_FG_OLIVE)),
        ("--badge-bg-azure", Token::Color(BADGE_BG_AZURE)),
        ("--badge-fg-azure", Token::Color(BADGE_FG_AZURE)),
        ("--badge-bg-neutral", Token::Color(BADGE_BG_NEUTRAL)),
        ("--badge-fg-neutral", Token::Color(BADGE_FG_NEUTRAL)),
        ("--bucket-red", Token::Color(BUCKET_RED)),
        ("--bucket-orange", Token::Color(BUCKET_ORANGE)),
        ("--bucket-yellow", Token::Color(BUCKET_YELLOW)),
        ("--bucket-green", Token::Color(BUCKET_GREEN)),
        ("--bucket-teal", Token::Color(BUCKET_TEAL)),
        ("--bucket-cyan", Token::Color(BUCKET_CYAN)),
        ("--bucket-blue", Token::Color(BUCKET_BLUE)),
        ("--bucket-purple", Token::Color(BUCKET_PURPLE)),
        ("--bucket-magenta", Token::Color(BUCKET_MAGENTA)),
        ("--bucket-neutral", Token::Color(BUCKET_NEUTRAL)),
        ("--space-xs", Token::Points(SPACING_XS)),
        ("--space-s", Token::Points(SPACING_S)),
        ("--space-m", Token::Points(SPACING_M)),
        ("--space-l", Token::Points(SPACING_L)),
        ("--space-xl", Token::Points(SPACING_XL)),
        ("--radius-none", Token::Points(f32::from(RADIUS_NONE))),
        ("--radius-xs", Token::Points(f32::from(RADIUS_XS))),
        ("--radius-s", Token::Points(f32::from(RADIUS_S))),
        ("--radius-m", Token::Points(f32::from(RADIUS_M))),
        ("--radius-l", Token::Points(f32::from(RADIUS_L))),
        ("--radius-xl", Token::Points(f32::from(RADIUS_XL))),
        ("--dur-fast", Token::Seconds(DURATION_FAST)),
        ("--dur-normal", Token::Seconds(DURATION_NORMAL)),
        ("--grid-gap", Token::Points(GRID_GAP)),
        ("--thumbnail", Token::Points(THUMBNAIL_SIZE)),
        ("--sidebar-width", Token::Points(SIDEBAR_WIDTH)),
        ("--preview-width", Token::Points(PREVIEW_WIDTH)),
        ("--font-sans", Token::Text(FONT_SANS)),
        ("--font-mono", Token::Text(FONT_MONO)),
        ("--size-body", Token::Points(SIZE_BODY)),
        ("--size-heading", Token::Points(SIZE_HEADING)),
        ("--size-small", Token::Points(SIZE_SMALL)),
        ("--size-log", Token::Points(SIZE_LOG)),
        ("--size-caption", Token::Points(SIZE_CAPTION)),
        ("--weight-regular", Token::Number(WEIGHT_REGULAR)),
        ("--weight-medium", Token::Number(WEIGHT_MEDIUM)),
        ("--weight-semibold", Token::Number(WEIGHT_SEMIBOLD)),
    ]
}

#[test]
fn every_stylesheet_token_is_ported_with_its_value() {
    let sheet = stylesheet_tokens();
    let ported = ported_tokens();
    assert_eq!(sheet.len(), 80, "the stylesheet holds 80 tokens");
    assert_eq!(
        ported.len(),
        sheet.len(),
        "one constant per stylesheet token"
    );

    let sheet_names: BTreeSet<&str> = sheet.iter().map(|(name, _)| name.as_str()).collect();
    let ported_names: BTreeSet<&str> = ported.iter().map(|(name, _)| *name).collect();
    assert_eq!(sheet_names.len(), sheet.len(), "a stylesheet token repeats");
    assert_eq!(ported_names.len(), ported.len(), "a ported token repeats");
    assert_eq!(ported_names, sheet_names, "the token name sets differ");

    for (name, token) in &ported {
        let (_, value) = sheet
            .iter()
            .find(|(sheet_name, _)| sheet_name == name)
            .unwrap_or_else(|| panic!("{name} is not in the stylesheet"));
        let value = value.as_str();
        match token {
            Token::Color(ported) => assert_eq!(*ported, color(value), "{name}"),
            Token::Points(ported) => {
                assert!((*ported - number(value, "px")).abs() < 1e-4, "{name}");
            }
            Token::Seconds(ported) => {
                assert!(
                    (*ported - number(value, "ms") / 1000.0).abs() < 1e-4,
                    "{name}"
                );
            }
            Token::Number(ported) => assert!((*ported - number(value, "")).abs() < 1e-4, "{name}"),
            Token::Text(ported) => assert_eq!(*ported, value, "{name}"),
        }
    }
}

#[test]
fn the_pink_bucket_token_is_ff80ab() {
    assert_eq!(BUCKET_PINK, Color32::from_rgb(0xff, 0x80, 0xab));
}
