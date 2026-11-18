//! Tests for the tile context menu.

use super::*;
use crate::grid::{Selection, tile_id};
use egui::epaint::Shape;
use egui::{Color32, Key};

/// One painted text: the text, its rect, and its colour.
pub(super) type Text = (String, Rect, Color32);

/// The text shapes of one frame.
fn texts(output: &egui::FullOutput) -> Vec<Text> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Text(shape) => {
                let color = shape.galley.job.sections[0].format.color;
                Some((
                    shape.galley.job.text.clone(),
                    // The galley rect, not the ink: a letter with a tail would
                    // move the centre of its row.
                    Rect::from_min_size(shape.pos, shape.galley.size()),
                    color,
                ))
            }
            _ => None,
        })
        .collect()
}

/// Draw one frame with `events`. The screen is tall enough that the four test
/// items need no scroll bar, so the frame does not ask for a repaint.
pub(super) fn run(ctx: &egui::Context, gallery: &mut Gallery, events: Vec<Event>) -> Vec<Text> {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 800.0))),
        events,
        ..Default::default()
    };
    let now = Instant::now();
    let output = ctx.run_ui(input, |ui| gallery.show_at(ui, now));
    let seen = texts(&output);
    output.drop_without_applying_deltas();
    seen
}

/// A themed context and a gallery with four items in the folder `/images`.
pub(super) fn setup() -> (egui::Context, Gallery) {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    for _ in 0..4 {
        run(&ctx, &mut gallery, Vec::new());
    }
    (ctx, gallery)
}

/// Move to `pos`, press and release `button`, and draw a settled frame.
pub(super) fn press_at(
    ctx: &egui::Context,
    gallery: &mut Gallery,
    pos: Pos2,
    button: PointerButton,
) -> Vec<Text> {
    let press = |pressed| Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [
        vec![Event::PointerMoved(pos)],
        vec![press(true)],
        vec![press(false)],
    ] {
        run(ctx, gallery, events);
    }
    run(ctx, gallery, Vec::new())
}

/// The centre of the tile of item `index`.
pub(super) fn tile_center(ctx: &egui::Context, index: usize) -> Pos2 {
    ctx.read_response(tile_id(index))
        .expect("the tile registered a response")
        .rect
        .center()
}

/// Right-click the tile of item `index`. Returns the pointer position and the
/// texts of the frame after.
pub(super) fn right_click(
    ctx: &egui::Context,
    gallery: &mut Gallery,
    index: usize,
) -> (Pos2, Vec<Text>) {
    let pos = tile_center(ctx, index);
    (pos, press_at(ctx, gallery, pos, PointerButton::Secondary))
}

/// Click the menu row that reads `label`. Returns the texts of the frame after.
pub(super) fn click_row(
    ctx: &egui::Context,
    gallery: &mut Gallery,
    seen: &[Text],
    label: &str,
) -> Vec<Text> {
    let (_, rect, _) = seen
        .iter()
        .find(|(text, ..)| text == label)
        .unwrap_or_else(|| panic!("no menu row reads {label:?}"));
    press_at(ctx, gallery, rect.center(), PointerButton::Primary)
}

/// Whether the menu shows in `seen`.
pub(super) fn menu_shows(seen: &[Text]) -> bool {
    seen.iter().any(|(text, ..)| text == "Add tag...")
}

/// The `/images` path of `name`.
fn path(name: &str) -> PathBuf {
    PathBuf::from("/images").join(name)
}

/// Select the items `indices` and draw so the tiles have a place.
fn select(gallery: &mut Gallery, indices: &[usize]) {
    gallery.selection = Selection::from_parts(indices.iter().copied(), indices.first().copied());
    gallery.sync_selection_revision();
}

/// Assert that the menu has no event waiting. `what` names the step.
pub(super) fn assert_no_events(gallery: &mut Gallery, what: &str) {
    let events = gallery.take_menu_events();
    assert_eq!(events, Vec::new(), "{what}");
}

/// A key press of `key`.
pub(super) fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn a_right_click_on_an_unselected_tile_selects_only_it_and_opens_the_menu() {
    let (ctx, mut gallery) = setup();
    select(&mut gallery, &[0, 1]);
    let (_, seen) = right_click(&ctx, &mut gallery, 3);
    assert_eq!(gallery.selected_paths(), [path("a 2.png")]);
    assert!(menu_shows(&seen), "the menu did not open");
}

#[test]
fn a_right_click_on_a_selected_tile_keeps_the_selection() {
    let (ctx, mut gallery) = setup();
    select(&mut gallery, &[1, 2, 3]);
    let before = gallery.selected_paths();
    let (_, seen) = right_click(&ctx, &mut gallery, 2);
    assert_eq!(gallery.selected_paths(), before);
    assert_eq!(before.len(), 3);
    assert!(menu_shows(&seen), "the menu did not open");
}

#[test]
fn one_selected_tile_gives_the_labels_in_four_groups() {
    let (ctx, mut gallery) = setup();
    let (_, seen) = right_click(&ctx, &mut gallery, 0);
    let labels = [
        "Open in editor",
        "Show in file manager",
        "Regenerate Thumbnail",
        "Add tag...",
        "Copy path",
        "Copy",
        "Paste",
        "Duplicate",
        "Rename...",
        "Delete",
        "Delete permanently...",
    ];
    let mut last = f32::MIN;
    let mut gaps = Vec::new();
    for label in labels {
        let (_, rect, _) = seen
            .iter()
            .find(|(text, ..)| text == label)
            .unwrap_or_else(|| panic!("no menu row reads {label:?}"));
        gaps.push(rect.center().y - last);
        assert!(rect.center().y > last, "{label:?} is out of order");
        last = rect.center().y;
    }
    // A separator widens the gap before the first row of a group: Regenerate
    // Thumbnail, Copy, Delete.
    let row = gaps[3];
    for after in [2, 5, 9] {
        assert!(gaps[after] > row + 1.0, "no separator before row {after}");
    }
    for plain in [1, 3, 4, 6, 7, 8, 10] {
        assert!((gaps[plain] - row).abs() < 0.5, "extra gap at row {plain}");
    }
}

#[test]
fn three_selected_tiles_give_the_plural_labels() {
    let (ctx, mut gallery) = setup();
    select(&mut gallery, &[1, 2, 3]);
    let (_, seen) = right_click(&ctx, &mut gallery, 1);
    for label in [
        "Open 3 in editor",
        "Regenerate 3 Thumbnails",
        "Delete 3 items",
        "Delete 3 items permanently...",
    ] {
        assert!(
            seen.iter().any(|(text, ..)| text == label),
            "no menu row reads {label:?}"
        );
    }
}

#[test]
fn shortcuts_are_dim_and_right_aligned() {
    let (ctx, mut gallery) = setup();
    let (_, seen) = right_click(&ctx, &mut gallery, 0);
    let find = |label: &str| {
        seen.iter()
            .find(|(text, ..)| text == label)
            .unwrap_or_else(|| panic!("no text reads {label:?}"))
    };
    let (_, long, color) = find("Shift+Del");
    let (_, short, _) = find("Ctrl+C");
    assert_eq!(*color, tokens::TEXT_MUTED);
    assert!((long.max.x - short.max.x).abs() < 0.5, "not right-aligned");
    let (_, label, _) = find("Delete permanently...");
    assert!(label.max.x < long.min.x, "the label meets the shortcut");
}

#[test]
fn single_only_rows_are_greyed_and_send_no_event_with_three_selected() {
    let (ctx, mut gallery) = setup();
    select(&mut gallery, &[1, 2, 3]);
    let (_, mut seen) = right_click(&ctx, &mut gallery, 1);
    for label in ["Show in file manager", "Copy path", "Rename..."] {
        let (_, _, color) = seen
            .iter()
            .find(|(text, ..)| text == label)
            .expect("the row shows");
        assert_eq!(*color, tokens::TEXT_MUTED, "{label:?} is not greyed");
        seen = click_row(&ctx, &mut gallery, &seen, label);
        assert!(menu_shows(&seen), "a greyed row closed the menu");
        assert_no_events(&mut gallery, label);
    }
    // An enabled row still works in the same menu.
    click_row(&ctx, &mut gallery, &seen, "Copy");
    assert_eq!(gallery.take_menu_events().len(), 1);
}

#[test]
fn each_enabled_row_sends_its_event_for_one_selected_tile() {
    let (ctx, mut gallery) = setup();
    let one = path("b.png");
    let paths = || vec![one.clone()];
    // The menu opens at the tile centre each time.
    let pos = tile_center(&ctx, 0);
    let cases = [
        ("Open in editor", GridMenuEvent::Open(paths())),
        (
            "Show in file manager",
            GridMenuEvent::ShowInFileManager(one.clone()),
        ),
        (
            "Regenerate Thumbnail",
            GridMenuEvent::RegenerateThumbnails(paths()),
        ),
        (
            "Add tag...",
            GridMenuEvent::AddTag {
                pos,
                paths: paths(),
            },
        ),
        ("Copy path", GridMenuEvent::CopyPath(one.clone())),
        ("Copy", GridMenuEvent::Copy(paths())),
        ("Paste", GridMenuEvent::Paste),
        ("Duplicate", GridMenuEvent::Duplicate(paths())),
        ("Rename...", GridMenuEvent::Rename(one.clone())),
        ("Delete", GridMenuEvent::Delete(paths())),
        (
            "Delete permanently...",
            GridMenuEvent::DeletePermanently(paths()),
        ),
    ];
    for (label, expected) in cases {
        let (_, seen) = right_click(&ctx, &mut gallery, 0);
        let after = click_row(&ctx, &mut gallery, &seen, label);
        assert!(!menu_shows(&after), "{label:?} left the menu open");
        assert_eq!(gallery.take_menu_events(), [expected], "{label:?}");
    }
}

#[test]
fn a_choice_with_three_selected_carries_the_paths_in_grid_order() {
    let (ctx, mut gallery) = setup();
    // Item order is b, a, a 10, a 2. The grid shows a 2, a 10, a.
    select(&mut gallery, &[1, 2, 3]);
    let (_, seen) = right_click(&ctx, &mut gallery, 1);
    click_row(&ctx, &mut gallery, &seen, "Delete 3 items");
    assert_eq!(
        gallery.take_menu_events(),
        [GridMenuEvent::Delete(vec![
            path("a 2.png"),
            path("a 10.png"),
            path("a.png")
        ])]
    );
    assert_no_events(&mut gallery, "the take did not clear");
}

#[test]
fn paste_is_greyed_when_every_image_shows() {
    let (ctx, mut gallery) = setup();
    gallery.set_scope(Scope::All);
    // One frame, so the tiles are placed for the new scope.
    run(&ctx, &mut gallery, Vec::new());
    let (_, seen) = right_click(&ctx, &mut gallery, 0);
    let (_, _, color) = seen
        .iter()
        .find(|(text, ..)| text == "Paste")
        .expect("the row shows");
    assert_eq!(*color, tokens::TEXT_MUTED);
    let after = click_row(&ctx, &mut gallery, &seen, "Paste");
    assert!(menu_shows(&after));
    assert_no_events(&mut gallery, "Paste");
}

#[test]
fn a_selection_change_under_the_open_menu_closes_it_with_no_event() {
    let (ctx, mut gallery) = setup();
    select(&mut gallery, &[1, 2, 3]);
    let (_, seen) = right_click(&ctx, &mut gallery, 1);
    assert!(menu_shows(&seen));
    // Only b and a stay, so one of the three selected files is left.
    gallery.set_items(items().into_iter().take(2).collect());
    assert_eq!(gallery.selected_paths(), [path("a.png")]);
    let after = run(&ctx, &mut gallery, Vec::new());
    assert!(!menu_shows(&after), "the menu stayed open");
    assert_no_events(&mut gallery, "the selection change");
    let later = run(&ctx, &mut gallery, Vec::new());
    assert!(!menu_shows(&later), "the menu came back");
}

#[test]
fn escape_closes_the_menu_with_no_event() {
    let (ctx, mut gallery) = setup();
    let (_, seen) = right_click(&ctx, &mut gallery, 0);
    assert!(menu_shows(&seen));
    run(&ctx, &mut gallery, vec![key(Key::Escape)]);
    let after = run(&ctx, &mut gallery, Vec::new());
    assert!(!menu_shows(&after), "Escape left the menu open");
    assert_no_events(&mut gallery, "Escape");
}

#[test]
fn a_press_outside_closes_the_menu_with_no_event() {
    let (ctx, mut gallery) = setup();
    let (_, seen) = right_click(&ctx, &mut gallery, 0);
    assert!(menu_shows(&seen));
    let after = press_at(
        &ctx,
        &mut gallery,
        Pos2::new(880.0, 780.0),
        PointerButton::Primary,
    );
    assert!(!menu_shows(&after), "the press left the menu open");
    assert_no_events(&mut gallery, "a press outside");
}
