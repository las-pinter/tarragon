//! Tests for the application shell.

use super::*;
use crate::layout::Pane;
use crate::theme::apply_theme;
use egui::{
    Color32, Context, Event, FontId, Id, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2,
};
use std::path::PathBuf;

/// The headless screen, wide and tall enough for both docks.
pub(super) fn input() -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
        ..Default::default()
    }
}

/// Draw one headless frame.
pub(super) fn draw(ctx: &Context, shell: &mut Shell) {
    ctx.run_ui(input(), |ui| shell.show(ui))
        .drop_without_applying_deltas();
}

/// One gallery item with no texture.
fn item(name: &str) -> GridItem {
    GridItem::new(PathBuf::from("/images").join(name), 1, None)
}

/// Items where the name order and the date order differ.
fn dated_items() -> Vec<GridItem> {
    let item =
        |name: &str, mtime: i64| GridItem::new(PathBuf::from("/images").join(name), mtime, None);
    vec![
        item("b.png", 10),
        item("a.png", 20),
        item("a 10.png", 5),
        item("a 2.png", 5),
    ]
}

/// Every text shape in the frame.
pub(super) fn painted_text(output: &egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::epaint::Shape, text: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(shape) => text.push(shape.galley.job.text.clone()),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, text);
                }
            }
            _ => {}
        }
    }
    let mut text = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut text);
    }
    text
}

/// The file names that the frame painted, in paint order.
///
/// The grid paints one tile after the next in display order, so this list is
/// the display order of the tiles.
fn painted_tile_names(output: &egui::FullOutput) -> Vec<String> {
    let names = ["a.png", "a 2.png", "a 10.png", "b.png"];
    painted_text(output)
        .into_iter()
        .filter(|text| names.contains(&text.as_str()))
        .collect()
}

/// The fill and the stroke colour of the rect that the frame painted at `rect`.
fn painted_rect_fill(output: &egui::FullOutput, rect: Rect) -> Option<(Color32, Color32)> {
    output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Rect(shape)
                if shape.rect.min.distance(rect.min) < 0.5
                    && shape.rect.max.distance(rect.max) < 0.5 =>
            {
                Some((shape.fill, shape.stroke.color))
            }
            _ => None,
        })
}

/// The painted fill and stroke of one sort pill.
fn pill_colours(ctx: &Context, output: &egui::FullOutput, mode: SortMode) -> (Color32, Color32) {
    let rect = ctx
        .read_response(Id::new(("tarragon-sort-pill", mode.as_str())))
        .expect("the pill registered a response")
        .rect;
    painted_rect_fill(output, rect).expect("the pill painted a rect")
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

/// The rect of the leaf that holds `pane`, from the last frame.
pub(super) fn leaf_rect(shell: &Shell, pane: Pane) -> Rect {
    let (node, _) = shell
        .dock
        .main_surface()
        .find_tab(&pane)
        .expect("the pane is in the dock");
    shell.dock.main_surface()[node]
        .rect()
        .expect("the leaf was drawn")
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
fn reset_layout_restores_the_defaults() {
    let mut shell = Shell::new();
    shell.toggle_log();
    for pane in [Pane::Library, Pane::Gallery, Pane::Preview] {
        shell.toggle(pane);
    }
    assert!(!shell.library_visible() && !shell.gallery_visible() && !shell.preview_visible());

    shell.reset_layout();

    assert!(shell.library_visible(), "library returns");
    assert!(shell.gallery_visible(), "gallery returns");
    assert!(shell.preview_visible(), "preview returns");
    assert!(!shell.log_visible(), "log hides again");
}

#[test]
fn the_shell_draws_the_gallery_items() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.gallery.set_folder(Some(PathBuf::from("/images")));
    shell.set_items(vec![item("a.png"), item("b.png"), item("c.png")]);
    draw(&ctx, &mut shell);
    draw(&ctx, &mut shell);
    assert!(
        ctx.read_response(Id::new(("tarragon-grid-tile", 0)))
            .is_some(),
        "the gallery grid is mounted"
    );
}

#[test]
fn with_sort_mode_restores_the_date_label_and_order() {
    let ctx = Context::default();
    apply_theme(&ctx);
    // The database stores the name `date`; the shell takes the parsed mode.
    let restored: SortMode = "date".parse().expect("the stored name parses");
    let mut shell = Shell::new().with_sort_mode(restored);
    shell.gallery.set_folder(Some(PathBuf::from("/images")));
    shell.set_items(dated_items());

    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    let text = painted_text(&output);
    let date_pill = pill_colours(&ctx, &output, SortMode::Date);
    let name_pill = pill_colours(&ctx, &output, SortMode::Natural);
    let names = painted_tile_names(&output);
    output.drop_without_applying_deltas();

    assert!(text.iter().any(|line| line == "Date"), "the Date pill drew");
    assert_eq!(
        date_pill,
        (tokens::BG_TERTIARY, tokens::AMBER_ACCENT),
        "the Date pill is the active pill"
    );
    assert_eq!(
        name_pill,
        (tokens::BG_SECONDARY, Color32::TRANSPARENT),
        "the Name pill is inactive"
    );
    assert_eq!(
        names,
        ["a.png", "b.png", "a 2.png", "a 10.png"],
        "the grid paints the date order"
    );
    assert_eq!(shell.take_sort_changed(), None, "a restore is no change");
}

#[test]
fn a_click_on_the_date_pill_switches_the_order_in_the_same_frame() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.gallery.set_folder(Some(PathBuf::from("/images")));
    shell.set_items(dated_items());
    draw(&ctx, &mut shell);
    draw(&ctx, &mut shell);

    let pos = ctx
        .read_response(Id::new(("tarragon-sort-pill", "date")))
        .expect("the Date pill registered")
        .rect
        .center();
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };

    let moved = RawInput {
        events: vec![Event::PointerMoved(pos)],
        ..input()
    };
    ctx.run_ui(moved, |ui| shell.show(ui))
        .drop_without_applying_deltas();
    let down = RawInput {
        events: vec![press(true)],
        ..input()
    };
    ctx.run_ui(down, |ui| shell.show(ui))
        .drop_without_applying_deltas();
    let up = RawInput {
        events: vec![press(false)],
        ..input()
    };
    let output = ctx.run_ui(up, |ui| shell.show(ui));
    let names = painted_tile_names(&output);
    output.drop_without_applying_deltas();

    assert_eq!(
        names,
        ["a.png", "b.png", "a 2.png", "a 10.png"],
        "the same frame paints the date order"
    );
    assert_eq!(shell.take_sort_changed(), Some(SortMode::Date));

    // The pills paint the new active state on the next frame.
    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    let date_pill = pill_colours(&ctx, &output, SortMode::Date);
    let name_pill = pill_colours(&ctx, &output, SortMode::Natural);
    output.drop_without_applying_deltas();
    assert_eq!(
        date_pill,
        (tokens::BG_TERTIARY, tokens::AMBER_ACCENT),
        "the Date pill is now active"
    );
    assert_eq!(
        name_pill,
        (tokens::BG_SECONDARY, Color32::TRANSPARENT),
        "the Name pill is now inactive"
    );
    assert_eq!(shell.take_sort_changed(), None, "the flag reports once");
}

#[test]
fn a_library_row_click_queues_one_navigate_and_set_folder_follows() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.sidebar_mut().set_roots(vec![PathBuf::from("/r")]);
    assert_eq!(
        shell.take_sidebar_events(),
        [SidebarEvent::ListChildren(PathBuf::from("/r"))]
    );
    draw(&ctx, &mut shell);
    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    let pos = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.job.text == "r" => Some(text.pos),
            _ => None,
        })
        .expect("the root row is painted")
        + vec2(2.0, 5.0);
    output.drop_without_applying_deltas();
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for event in [Event::PointerMoved(pos), press(true), press(false)] {
        let raw = RawInput {
            events: vec![event],
            ..input()
        };
        ctx.run_ui(raw, |ui| shell.show(ui))
            .drop_without_applying_deltas();
    }
    assert_eq!(
        shell.take_sidebar_events(),
        [SidebarEvent::Navigate(PathBuf::from("/r"))]
    );
    shell.set_folder(Some(PathBuf::from("/other")));
    assert_eq!(
        shell.sidebar_mut().open_folder(),
        Some(std::path::Path::new("/other"))
    );
}

#[test]
fn the_library_paddings_start_at_the_panel_edge() {
    let ctx = Context::default();
    apply_theme(&ctx);
    let mut shell = Shell::new();
    shell.sidebar_mut().set_roots(vec![PathBuf::from("/r")]);
    shell.set_folder(Some(PathBuf::from("/r")));
    draw(&ctx, &mut shell);
    let output = ctx.run_ui(input(), |ui| shell.show(ui));
    // The tab bar, its tab, and the selected row share one fill; sort top down.
    let mut rects: Vec<Rect> = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Rect(shape)
                if shape.fill == tokens::BG_TERTIARY && shape.rect.min.x < 20.0 =>
            {
                Some(shape.rect)
            }
            _ => None,
        })
        .collect();
    rects.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    let button_x = output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.job.text == "Add Current Folder" => {
                Some(text.pos.x)
            }
            _ => None,
        })
        .expect("the button is painted");
    output.drop_without_applying_deltas();

    // The first rect is the tab bar, the panel title strip.
    let title = rects[0];
    let row = *rects
        .iter()
        .find(|rect| rect.min.y >= title.max.y)
        .expect("the selected row is painted");
    assert!(title.min.x.abs() < 0.5, "title starts at {}", title.min.x);
    // The dock body ends at the Library leaf edge; the divider lies outside it.
    let edge = leaf_rect(&shell, Pane::Library).max.x;
    assert!((title.max.x - edge).abs() < 0.5, "title is {title:?}");
    assert!((row.min.x - 6.0).abs() < 0.5, "row starts at {}", row.min.x);
    assert!((row.max.x - (edge - 6.0)).abs() < 0.5);
    // The button text sits 8 pt of inset plus 8 pt of button padding in.
    assert!((button_x - 16.0).abs() < 0.5, "button text at {button_x}");
}

#[test]
fn restore_layout_round_trips_a_changed_layout() {
    let mut shell = Shell::new();
    set_visible(&mut shell.dock, Pane::Library, false);
    set_visible(&mut shell.dock, Pane::Log, true);
    let text = shell.layout_json();

    let mut fresh = Shell::new();
    assert!(fresh.restore_layout(&text));
    assert!(!fresh.library_visible());
    assert!(fresh.log_visible());
    assert_eq!(fresh.gallery_visible(), shell.gallery_visible());
    assert_eq!(fresh.preview_visible(), shell.preview_visible());
    assert_eq!(fresh.layout_json(), text);
}

#[test]
fn restore_layout_keeps_the_dock_on_garbage() {
    let mut shell = Shell::new();
    let before = shell.layout_json();
    assert!(!shell.restore_layout("not json"));
    assert_eq!(shell.layout_json(), before);
}
