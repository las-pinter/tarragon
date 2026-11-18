//! Tests for the Preview events and the selection hand-off through a full Shell.

use super::{draw, input, leaf_rect};
use crate::grid::GridItem;
use crate::layout::Pane;
use crate::preview::{Meta, PreviewEvent};
use crate::shell::Shell;
use crate::swatch::{self, Swatch};
use crate::theme::apply_theme;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use std::path::PathBuf;

/// A shell with the theme applied, a previewed file, and two drawn frames.
fn setup() -> (Context, Shell) {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.preview_mut().set_meta(Some(Meta {
        name: "a.png".into(),
        dimensions: None,
        size: None,
        format: "PNG".into(),
    }));
    draw(&ctx, &mut shell);
    draw(&ctx, &mut shell);
    (ctx, shell)
}

/// Run one frame with `events`; return every painted text, every plain
/// rectangle, and where they are.
fn frame(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> (Vec<(String, Pos2)>, Vec<Rect>) {
    fn collect(shape: &egui::Shape, texts: &mut Vec<(String, Pos2)>, rects: &mut Vec<Rect>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, texts, rects)),
            egui::Shape::Text(t) => texts.push((t.galley.job.text.clone(), t.pos)),
            egui::Shape::Rect(r) => rects.push(r.rect),
            _ => {}
        }
    }
    let raw = RawInput { events, ..input() };
    let output = ctx.run_ui(raw, |ui| shell.show(ui));
    let (mut texts, mut rects) = (Vec::new(), Vec::new());
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut texts, &mut rects);
    }
    output.drop_without_applying_deltas();
    (texts, rects)
}

/// Click at `at`. The pointer moves in its own frame first, because egui
/// hit-tests against the last layout.
fn click(ctx: &Context, shell: &mut Shell, at: Pos2) {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(ctx, shell, vec![Event::PointerMoved(at)]);
    frame(ctx, shell, vec![button(true)]);
    frame(ctx, shell, vec![button(false)]);
}

/// Where the text `label` is painted.
fn text_pos(ctx: &Context, shell: &mut Shell, label: &str) -> Pos2 {
    frame(ctx, shell, vec![])
        .0
        .into_iter()
        .find_map(|(text, pos)| (text == label).then_some(pos))
        .unwrap_or_else(|| panic!("{label} is not drawn"))
}

fn is_drawn(ctx: &Context, shell: &mut Shell, label: &str) -> bool {
    frame(ctx, shell, vec![]).0.iter().any(|(t, _)| t == label)
}

#[test]
fn a_colour_click_in_the_preview_comes_out_of_the_shell_once() {
    let (ctx, mut shell) = setup();
    let leaf = leaf_rect(&shell, Pane::Preview);
    let mut squares: Vec<Rect> = frame(&ctx, &mut shell, vec![])
        .1
        .into_iter()
        .filter(|r| r.size() == vec2(swatch::SIZE, swatch::SIZE) && leaf.contains_rect(*r))
        .collect();
    squares.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
    assert_eq!(squares.len(), 11, "the colour row is painted");
    assert_eq!(shell.take_preview_events(), Vec::<PreviewEvent>::new());

    click(&ctx, &mut shell, squares[6].center());
    assert_eq!(
        shell.take_preview_events(),
        [PreviewEvent::ColorClicked(Swatch::ALL[6])]
    );
    assert_eq!(
        shell.take_preview_events(),
        Vec::<PreviewEvent>::new(),
        "draining empties the queue"
    );
}

#[test]
fn a_selection_change_closes_the_add_menu_and_a_quiet_frame_keeps_it() {
    let (ctx, mut shell) = setup();
    shell.preview_mut().set_known_tags(vec!["hands".into()]);
    shell.set_folder(Some(PathBuf::from("/images")));
    shell.set_items(vec![GridItem::new(PathBuf::from("/images/a.png"), 1, None)]);
    draw(&ctx, &mut shell);

    // Select the tile with the pointer, then open the menu.
    let tile = text_pos(&ctx, &mut shell, "a.png") + vec2(3.0, 3.0);
    click(&ctx, &mut shell, tile);
    assert_eq!(
        shell.selected_paths(),
        [PathBuf::from("/images/a.png")],
        "the click selected the tile"
    );
    let add = text_pos(&ctx, &mut shell, "+ add") + vec2(3.0, 3.0);
    click(&ctx, &mut shell, add);
    assert!(is_drawn(&ctx, &mut shell, "hands"), "the menu is open");

    for _ in 0..3 {
        draw(&ctx, &mut shell);
    }
    assert!(
        is_drawn(&ctx, &mut shell, "hands"),
        "frames with the same selection keep the menu open"
    );

    // New items clear the selection without a pointer press in the Preview.
    let before = shell.selection_revision();
    shell.set_items(Vec::new());
    assert_ne!(shell.selection_revision(), before);
    draw(&ctx, &mut shell);
    assert!(
        !is_drawn(&ctx, &mut shell, "hands"),
        "the selection change closed the menu"
    );
}

#[test]
fn a_gallery_click_that_changes_the_selection_leaves_the_menu_closed_next_frame() {
    let (ctx, mut shell) = setup();
    shell.preview_mut().set_known_tags(vec!["hands".into()]);
    shell.set_folder(Some(PathBuf::from("/images")));
    shell.set_items(vec![
        GridItem::new(PathBuf::from("/images/a.png"), 1, None),
        GridItem::new(PathBuf::from("/images/b.png"), 2, None),
    ]);
    draw(&ctx, &mut shell);

    let first = text_pos(&ctx, &mut shell, "a.png") + vec2(3.0, 3.0);
    click(&ctx, &mut shell, first);
    let add = text_pos(&ctx, &mut shell, "+ add") + vec2(3.0, 3.0);
    click(&ctx, &mut shell, add);
    assert!(is_drawn(&ctx, &mut shell, "hands"), "the menu is open");

    // The press also counts as a press outside the menu, which closes it on
    // its own. This pins the end state a user sees; the revision rule itself is
    // pinned by the test above, where no pointer is involved.
    let second = text_pos(&ctx, &mut shell, "b.png") + vec2(3.0, 3.0);
    click(&ctx, &mut shell, second);
    assert_eq!(shell.selected_paths(), [PathBuf::from("/images/b.png")]);
    assert!(
        !is_drawn(&ctx, &mut shell, "hands"),
        "the menu is closed by the next frame"
    );
}
