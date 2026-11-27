//! Tests for the status bar.

use egui::epaint::Shape;
use egui::{Context, Pos2, Rect};

use super::{Job, STATUS_HEIGHT, grouped, show};
use crate::{test_support, tokens};

/// What one frame of the bar painted.
#[derive(Default)]
struct Painted {
    /// Each text with its left edge.
    texts: Vec<(String, Rect)>,
    /// The track rects on the hover step.
    tracks: Vec<Rect>,
    /// The amber rects: a fill or a sliding block.
    amber: Vec<Rect>,
}

fn collect(shape: &Shape, painted: &mut Painted) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, painted)),
        Shape::Text(text) => painted
            .texts
            .push((text.galley.job.text.clone(), text.visual_bounding_rect())),
        Shape::Rect(rect) if rect.fill == tokens::SURFACE_HOVER => painted.tracks.push(rect.rect),
        Shape::Rect(rect) if rect.fill == tokens::AMBER_ACCENT => painted.amber.push(rect.rect),
        _ => {}
    }
}

/// Draw the bar 800 wide at `time` seconds and return what it painted.
fn frame(ctx: &Context, jobs: &[Job], selected: usize, time: f64) -> Painted {
    let mut input = test_support::input(800.0, vec![]);
    input.time = Some(time);
    let output = ctx.run_ui(input, |ui| {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, STATUS_HEIGHT));
        let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        show(&mut bar, jobs, selected);
    });
    let mut painted = Painted::default();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut painted);
    }
    output.drop_without_applying_deltas();
    painted
}

fn text_rect(painted: &Painted, want: &str) -> Rect {
    painted
        .texts
        .iter()
        .find(|(text, _)| text == want)
        .unwrap_or_else(|| panic!("no {want:?} in {:?}", painted.texts))
        .1
}

#[test]
fn numbers_group_their_thousands_with_commas() {
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1204), "1,204");
    assert_eq!(grouped(1_234_567), "1,234,567");
}

#[test]
fn each_job_has_the_text_of_the_page() {
    let cases = [
        (Job::Scan { found: 1 }, "Scanning 1 file"),
        (Job::Scan { found: 1204 }, "Scanning 1,204 files"),
        (Job::Library, "Loading library"),
        (
            Job::Thumbnails {
                done: 340,
                total: 1880,
            },
            "Rendering thumbnails 340 / 1,880",
        ),
        (
            Job::Colors {
                done: 1204,
                total: 1880,
            },
            "Indexing colors 1,204 / 1,880",
        ),
        (Job::Purge { done: 0, total: 0 }, "Purging cache"),
        (
            Job::Purge {
                done: 512,
                total: 2048,
            },
            "Purging cache 512 B / 2.0 KB",
        ),
    ];
    for (job, want) in cases {
        assert_eq!(job.text(), want, "{job:?}");
    }
}

#[test]
fn only_a_job_with_a_total_has_a_share() {
    assert_eq!(Job::Scan { found: 9 }.share(), None);
    assert_eq!(Job::Library.share(), None);
    assert_eq!(Job::Purge { done: 5, total: 0 }.share(), None);
    assert_eq!(Job::Colors { done: 1, total: 4 }.share(), Some(0.25));
    // A file written after the walk can carry `done` past `total`.
    assert_eq!(Job::Purge { done: 9, total: 4 }.share(), Some(1.0));
}

#[test]
fn a_counted_job_fills_its_track_by_its_share() {
    let ctx = test_support::context();
    let painted = frame(&ctx, &[Job::Colors { done: 1, total: 4 }], 0, 1.0);
    assert_eq!(painted.tracks.len(), 1, "{:?}", painted.tracks);
    let track = painted.tracks[0];
    assert_eq!((track.width(), track.height()), (120.0, 4.0));
    assert_eq!(painted.amber.len(), 1, "{:?}", painted.amber);
    let fill = painted.amber[0];
    assert_eq!(fill.min, track.min);
    assert!((fill.width() - 30.0).abs() < 0.01, "fill {fill:?}");
    let text = text_rect(&painted, "Indexing colors 1 / 4");
    assert!(text.left() > track.right(), "the text sits after the track");
}

#[test]
fn a_job_with_no_total_slides_its_block() {
    let ctx = test_support::context();
    let first = frame(&ctx, &[Job::Scan { found: 3 }], 0, 10.0);
    let second = frame(&ctx, &[Job::Scan { found: 3 }], 0, 10.3);
    assert_eq!((first.amber.len(), second.amber.len()), (1, 1));
    assert_ne!(first.amber[0].min.x, second.amber[0].min.x);
}

#[test]
fn two_jobs_sit_side_by_side_in_order_and_the_selection_at_the_right() {
    let ctx = test_support::context();
    let jobs = [
        Job::Thumbnails { done: 1, total: 2 },
        Job::Colors { done: 0, total: 2 },
    ];
    let painted = frame(&ctx, &jobs, 1204, 1.0);
    assert_eq!(painted.tracks.len(), 2);
    let thumbs = text_rect(&painted, "Rendering thumbnails 1 / 2");
    let colors = text_rect(&painted, "Indexing colors 0 / 2");
    assert!(
        thumbs.right() < painted.tracks[1].left(),
        "thumbnails come first"
    );
    assert!(painted.tracks[1].left() < colors.left());
    let selected = text_rect(&painted, "1,204 selected");
    assert!(selected.left() > colors.right());
    assert!(
        selected.right() > 780.0,
        "the count ends at the right: {selected:?}"
    );
}

#[test]
fn an_idle_bar_with_no_selection_paints_nothing() {
    let ctx = test_support::context();
    let painted = frame(&ctx, &[], 0, 1.0);
    assert!(painted.texts.is_empty(), "{:?}", painted.texts);
    assert!(painted.tracks.is_empty() && painted.amber.is_empty());
}
