use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use eframe::egui::{Color32, Context, FontFamily, FontId, RawInput};
use tarragon_ui::FallbackFont;
use tracing::Level;
use tracing_subscriber::layer::SubscriberExt;

use super::{Found, NO_CJK, apply, discover, load, report, spawn};
use crate::logging::ring::{LogRing, RingLayer};

const ROBOTO: &[u8] = include_bytes!("../../../tarragon-ui/assets/fonts/Roboto-Regular.ttf");
const ROBOTO_BOLD: &[u8] = include_bytes!("../../../tarragon-ui/assets/fonts/Roboto-Bold.ttf");
const CJK_TEXT: &str = "日本語 한국어 中文";

fn face(name: &str, data: &[u8]) -> FallbackFont {
    FallbackFont {
        name: name.to_owned(),
        data: data.to_vec(),
        index: 0,
    }
}

fn frame(ctx: &Context) {
    ctx.run_ui(RawInput::default(), |_| {})
        .drop_without_applying_deltas();
}

/// Run `body` with a subscriber that keeps the lines, and return the lines.
fn logged(body: impl FnOnce()) -> Vec<(Level, String)> {
    let ring = LogRing::new();
    let layer = RingLayer::new(ring.clone(), time::UtcOffset::UTC);
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), body);
    ring.snapshot()
        .into_iter()
        .map(|entry| (entry.level, entry.text))
        .collect()
}

/// True when a warn line ends with `text`. The ring adds a time prefix.
fn warned(lines: &[(Level, String)], text: &str) -> bool {
    lines
        .iter()
        .any(|(level, line)| *level == Level::WARN && line.ends_with(text))
}

/// The laid-out width of `text` in `family`. It changes when another face
/// draws the text. Unlike `has_glyphs`, it does not depend on which face egui
/// picks for the replacement glyph.
fn width(ctx: &Context, family: FontFamily, text: &str) -> f32 {
    ctx.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), FontId::new(14.0, family), Color32::WHITE)
            .rect
            .width()
    })
}

/// The four families. The ui crate does not export the two names, so they
/// are spelled out here.
fn families() -> [FontFamily; 4] {
    [
        FontFamily::Proportional,
        FontFamily::Monospace,
        FontFamily::Name("Roboto Medium".into()),
        FontFamily::Name("Roboto Bold".into()),
    ]
}

#[test]
fn no_cjk_face_gives_the_warning_text_and_a_warn_line() {
    let empty = Found {
        fonts: Vec::new(),
        cjk: false,
    };
    let mut result = None;

    let lines = logged(|| result = report(&empty));

    assert_eq!(result.as_deref(), Some(NO_CJK));
    assert!(warned(&lines, NO_CJK), "{lines:?}");
}

#[test]
fn a_cjk_face_gives_no_warning_and_an_info_line_with_the_names() {
    let found = Found {
        fonts: vec![face("fallback-0-Han", ROBOTO)],
        cjk: true,
    };
    let mut result = Some(String::new());

    let lines = logged(|| result = report(&found));

    assert_eq!(result, None);
    assert!(
        lines.iter().all(|(level, _)| *level == Level::INFO),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|(_, text)| text.contains("fallback-0-Han")),
        "{lines:?}"
    );
}

#[test]
fn a_junk_face_is_named_in_a_warn_line_and_the_good_face_still_applies() {
    let ctx = Context::default();
    tarragon_ui::apply_theme(&ctx);
    let found = Found {
        fonts: vec![face("junk-face", &[0; 16]), face("good-face", ROBOTO)],
        cjk: true,
    };

    let lines = logged(|| apply(&ctx, found));
    frame(&ctx);

    let expected = "skipped system fonts that egui cannot parse: junk-face";
    assert!(warned(&lines, expected), "{lines:?}");
    let defs = ctx.fonts(|fonts| fonts.definitions().clone());
    assert!(defs.font_data.contains_key("good-face"));
    assert!(!defs.font_data.contains_key("junk-face"));
}

#[test]
fn applying_asks_for_a_repaint() {
    let ctx = Context::default();
    let asked = Arc::new(AtomicBool::new(false));
    let flag = asked.clone();
    ctx.set_request_repaint_callback(move |_| flag.store(true, Ordering::SeqCst));

    apply(
        &ctx,
        Found {
            fonts: Vec::new(),
            cjk: true,
        },
    );

    assert!(asked.load(Ordering::SeqCst));
}

#[test]
fn roboto_keeps_first_place_after_the_fallbacks_are_added() {
    let ctx = Context::default();
    tarragon_ui::apply_theme(&ctx);
    frame(&ctx);

    apply(
        &ctx,
        Found {
            fonts: vec![face("fb-bold", ROBOTO_BOLD)],
            cjk: true,
        },
    );
    frame(&ctx);

    let defs = ctx.fonts(|fonts| fonts.definitions().clone());
    assert_eq!(
        defs.families[&FontFamily::Proportional]
            .first()
            .map(String::as_str),
        Some("roboto-regular")
    );
}

#[test]
fn spawn_loads_the_fonts_on_its_own_thread_and_asks_for_a_repaint() {
    let ctx = Context::default();
    let (tx, rx) = std::sync::mpsc::channel();
    ctx.set_request_repaint_callback(move |_| {
        // The receiver may be gone after the first signal.
        tx.send(std::thread::current().name().map(str::to_owned))
            .ok();
    });

    spawn(&ctx);

    let name = rx.recv_timeout(Duration::from_secs(20)).expect("a repaint");
    assert_eq!(name.as_deref(), Some("sysfonts"));
}

#[test]
fn a_panic_while_finding_fonts_is_logged_and_does_not_unwind() {
    let ctx = Context::default();

    let lines = logged(|| load(&ctx, || panic!("no fonts today")));

    assert!(
        warned(&lines, "the system font loader panicked, using Roboto only"),
        "{lines:?}"
    );
}

#[test]
fn the_font_search_runs_inside_the_render_guard_so_a_panic_writes_no_crash_file() {
    let ctx = Context::default();
    let mut inside = false;

    load(&ctx, || {
        inside = tarragon_core::render_scope::inside_render_guard();
        Found {
            fonts: Vec::new(),
            cjk: true,
        }
    });

    assert!(inside);
    assert!(!tarragon_core::render_scope::inside_render_guard());
}

#[test]
fn a_system_cjk_font_changes_the_cjk_text_in_all_four_families() {
    let found = discover();
    if !found.cjk {
        eprintln!("skipped: no system CJK font on this box");
        return;
    }
    // The CJK faces come first in the list.
    let first = found.fonts[0].name.clone();
    let ctx = Context::default();
    tarragon_ui::apply_theme(&ctx);
    frame(&ctx);
    let before = families().map(|family| width(&ctx, family, CJK_TEXT));

    let rejected = tarragon_ui::add_fallbacks(&ctx, found.fonts);
    frame(&ctx);

    // A box with a broken font file must not fail the test. The first CJK face
    // is the one this test knows about, so the test skips when egui rejects it.
    if rejected.contains(&first) {
        eprintln!("skipped: egui cannot parse the first system CJK font");
        return;
    }
    for (family, before) in families().into_iter().zip(before) {
        let after = width(&ctx, family.clone(), CJK_TEXT);
        assert!(
            (after - before).abs() > 1.0,
            "{family:?} drew the CJK text the same: {before} then {after}"
        );
    }
}
