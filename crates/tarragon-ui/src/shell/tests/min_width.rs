//! The shell holds the Gallery at its minimum width.

use super::leaf_rect;
use crate::layout::Pane;
use crate::shell::Shell;
use crate::theme::apply_theme;
use crate::tokens::GALLERY_MIN_WIDTH;
use egui::{Context, Pos2, RawInput, Rect, vec2};
use egui_dock::{Node, NodeIndex};
use std::time::Duration;

/// One frame in a screen `width` wide. Returns the delay to the next repaint.
fn frame(ctx: &Context, shell: &mut Shell, width: f32) -> Duration {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| shell.show(ui));
    let delay = output
        .viewport_output
        .values()
        .map(|viewport| viewport.repaint_delay)
        .min()
        .expect("the root viewport");
    output.drop_without_applying_deltas();
    delay
}

fn themed() -> Context {
    let ctx = Context::default();
    apply_theme(&ctx);
    ctx
}

/// Frames until the first-frame repaints of a fresh shell stop.
const SETTLE: usize = 6;

/// A shell that has drawn `SETTLE` frames at `width` and wants no more.
fn settled(ctx: &Context, width: f32) -> Shell {
    let mut shell = Shell::new();
    for _ in 0..SETTLE {
        frame(ctx, &mut shell, width);
    }
    assert_ne!(frame(ctx, &mut shell, width), Duration::ZERO);
    shell
}

fn narrow_gallery(shell: &mut Shell, fraction: f32) {
    match &mut shell.dock.main_surface_mut()[NodeIndex::root().right()] {
        Node::Horizontal(split) => split.fraction = fraction,
        _ => panic!("not a split"),
    }
}

#[test]
fn a_narrow_window_gives_the_gallery_its_minimum_within_three_frames() {
    let ctx = themed();
    let mut shell = settled(&ctx, 1280.0);
    narrow_gallery(&mut shell, 0.3);
    frame(&ctx, &mut shell, 700.0);
    assert!(leaf_rect(&shell, Pane::Gallery).width() < GALLERY_MIN_WIDTH);
    // The dock also snaps the Library to its own divider limit on the first
    // frame at the new width, which costs one more frame.
    frame(&ctx, &mut shell, 700.0);
    frame(&ctx, &mut shell, 700.0);
    assert!(leaf_rect(&shell, Pane::Gallery).width() >= GALLERY_MIN_WIDTH - 0.5);
}

#[test]
fn a_clamp_asks_for_one_repaint_and_then_settles() {
    let ctx = themed();
    let mut shell = settled(&ctx, 1280.0);
    narrow_gallery(&mut shell, 0.2);
    // The frame that moves the fraction asks for the next one.
    assert_eq!(frame(&ctx, &mut shell, 1280.0), Duration::ZERO);
    // The widened layout may repaint once more, then the loop must end.
    frame(&ctx, &mut shell, 1280.0);
    assert_ne!(frame(&ctx, &mut shell, 1280.0), Duration::ZERO);
}

#[test]
fn a_normal_window_asks_for_no_extra_repaint() {
    let ctx = themed();
    let mut shell = settled(&ctx, 1280.0);
    assert_ne!(frame(&ctx, &mut shell, 1280.0), Duration::ZERO);
    assert!(leaf_rect(&shell, Pane::Gallery).width() >= GALLERY_MIN_WIDTH);
}

#[test]
fn a_saved_layout_with_a_narrow_gallery_is_widened_by_the_next_frame() {
    let mut saved = Shell::new();
    narrow_gallery(&mut saved, 0.2);
    let text = saved.layout_json();

    let ctx = themed();
    let mut shell = Shell::new();
    assert!(shell.restore_layout(&text));
    frame(&ctx, &mut shell, 1280.0);
    assert!(leaf_rect(&shell, Pane::Gallery).width() < GALLERY_MIN_WIDTH);
    frame(&ctx, &mut shell, 1280.0);
    assert!(leaf_rect(&shell, Pane::Gallery).width() >= GALLERY_MIN_WIDTH - 0.5);
}
