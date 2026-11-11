//! Tests for the notice line at the foot of the gallery.

use super::*;
use egui::epaint::Shape;

/// What one frame showed.
struct Seen {
    /// The painted rows and the rect of the notice, or none when it did not
    /// show.
    line: Option<(String, Rect)>,
    /// The shortest repaint delay of the frame.
    delay: Duration,
    /// The lowest painted point of everything above the notice line, or
    /// `f32::MIN` when the notice did not show.
    content_bottom: f32,
}

/// The lowest painted point, inside its clip area, of the shapes that are not
/// in the clip area of the notice line.
fn bottom_above(output: &egui::FullOutput, notice_top: f32) -> f32 {
    output
        .shapes
        .iter()
        // The panel clip starts at the frame, above the text by its margin.
        .filter(|clipped| clipped.clip_rect.min.y < notice_top - tokens::SPACING_S)
        .filter_map(|clipped| {
            let bounds = clipped.shape.visual_bounding_rect();
            bounds
                .intersects(clipped.clip_rect)
                .then(|| bounds.intersect(clipped.clip_rect).max.y)
        })
        .fold(f32::MIN, f32::max)
}

/// Draw one frame `width` points wide at `now`. `text` names the notice.
fn frame_at(
    ctx: &egui::Context,
    gallery: &mut Gallery,
    width: f32,
    now: Instant,
    text: &str,
) -> Seen {
    // The screen is tall enough that the four test items need no scroll bar.
    // A scroll bar fades, and its fade asks for a repaint for as long as the
    // frames carry no time.
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 800.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| gallery.show_at(ui, now));
    let line = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            Shape::Text(shape) if shape.galley.job.text == text => {
                let shown: String = shape.galley.rows.iter().map(|r| r.text()).collect();
                Some((shown, shape.visual_bounding_rect()))
            }
            _ => None,
        });
    let delay = output
        .viewport_output
        .values()
        .map(|viewport| viewport.repaint_delay)
        .min()
        .expect("the frame reports a repaint delay");
    let content_bottom = line
        .as_ref()
        .map_or(f32::MIN, |(_, rect)| bottom_above(&output, rect.min.y));
    output.drop_without_applying_deltas();
    Seen {
        line,
        delay,
        content_bottom,
    }
}

/// A themed context and a gallery that shows the folder `/images`. A few
/// frames have run, so fonts and layout have settled.
fn setup() -> (egui::Context, Gallery, Instant) {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    let start = Instant::now();
    for _ in 0..6 {
        frame_at(&ctx, &mut gallery, 600.0, start, "");
    }
    (ctx, gallery, start)
}

/// Whether the frame at `now` shows the notice `text`.
fn shows(ctx: &egui::Context, gallery: &mut Gallery, now: Instant, text: &str) -> bool {
    frame_at(ctx, gallery, 600.0, now, text).line.is_some()
}

#[test]
fn a_notice_shows_for_six_seconds_and_then_hides() {
    let (ctx, mut gallery, start) = setup();
    assert!(!shows(&ctx, &mut gallery, start, "x"));
    gallery.notify("x".to_owned());
    assert!(shows(&ctx, &mut gallery, start, "x"));
    assert!(shows(
        &ctx,
        &mut gallery,
        start + Duration::from_millis(5900),
        "x"
    ));
    assert!(!shows(
        &ctx,
        &mut gallery,
        start + Duration::from_millis(6100),
        "x"
    ));
    assert!(!shows(
        &ctx,
        &mut gallery,
        start + Duration::from_millis(6200),
        "x"
    ));
}

#[test]
fn a_second_notice_replaces_the_text_and_restarts_the_time() {
    let (ctx, mut gallery, start) = setup();
    gallery.notify("first".to_owned());
    assert!(shows(&ctx, &mut gallery, start, "first"));
    let later = start + Duration::from_secs(4);
    gallery.notify("second".to_owned());
    assert!(shows(&ctx, &mut gallery, later, "second"));
    assert!(!shows(&ctx, &mut gallery, later, "first"));
    // Ten seconds after the first call, six after the second would hide it.
    assert!(shows(
        &ctx,
        &mut gallery,
        later + Duration::from_millis(5900),
        "second"
    ));
    assert!(!shows(
        &ctx,
        &mut gallery,
        later + Duration::from_millis(6100),
        "second"
    ));
}

#[test]
fn the_expiry_asks_for_one_repaint_and_an_idle_frame_asks_for_none() {
    let (ctx, mut gallery, start) = setup();
    let idle = frame_at(&ctx, &mut gallery, 600.0, start, "x").delay;
    assert_eq!(idle, Duration::MAX, "an idle frame asks for no repaint");
    gallery.notify("x".to_owned());
    // The first frame places the line, and the second one is settled.
    frame_at(&ctx, &mut gallery, 600.0, start, "x");
    let now = start + Duration::from_secs(1);
    let seen = frame_at(&ctx, &mut gallery, 600.0, now, "x");
    let due = Duration::from_secs(5) + WAKE_SLACK;
    assert!(
        seen.delay > Duration::from_millis(4900) && seen.delay <= due,
        "asked for {:?}, expected about {due:?}",
        seen.delay
    );
    let expired = start + Duration::from_millis(6100);
    let seen = frame_at(&ctx, &mut gallery, 600.0, expired, "x");
    assert!(seen.line.is_none());
    assert_eq!(seen.delay, Duration::MAX, "the expired frame asks for none");
}

#[test]
fn the_grid_paints_above_the_notice_line() {
    let (ctx, mut gallery, start) = setup();
    let many = (0..200)
        .map(|n| GridItem::new(PathBuf::from("/images").join(format!("{n}.png")), n, None))
        .collect();
    gallery.set_items(many);
    gallery.notify("x".to_owned());
    frame_at(&ctx, &mut gallery, 600.0, start, "x");
    let seen = frame_at(&ctx, &mut gallery, 600.0, start, "x");
    let (_, rect) = seen.line.expect("the notice was painted");
    assert!(!gallery.painted.is_empty(), "the grid painted tiles");
    assert!(
        seen.content_bottom <= rect.min.y,
        "the grid reaches {}, the notice starts at {}",
        seen.content_bottom,
        rect.min.y
    );
}

#[test]
fn a_long_notice_is_cut_with_an_ellipsis_inside_the_width() {
    let (ctx, mut gallery, start) = setup();
    let long = "Could not move the file because the folder is read only and full";
    gallery.notify(long.to_owned());
    let seen = frame_at(&ctx, &mut gallery, 260.0, start, long);
    let (shown, rect) = seen.line.expect("the notice was painted");
    assert!(shown.ends_with('…'), "{shown:?}");
    assert!(shown.len() < long.len(), "{shown:?}");
    assert!(rect.max.x <= 260.0, "{shown:?} ends at {}", rect.max.x);
}
