//! End-to-end test of the stored window state and the gallery sort pills.
//!
//! The app crate owns the runtime wiring; this test proves the round trip that
//! the wiring depends on.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Context, Event, Id, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use tarragon_core::sorting::SortMode;
use tarragon_db::Database;
use tarragon_db::settings::{Settings, WindowState};
use tarragon_ui::Shell;
use tarragon_ui::grid::GridItem;
use tarragon_ui::tokens;

/// A temporary database directory that removes itself on drop.
struct TempDb {
    dir: PathBuf,
    db: Database,
}

impl TempDb {
    /// Create and initialize a fresh database in a unique temp directory.
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tarragon-ui-window-state-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        let db = Database::open(dir.join("tarragon.db")).expect("open the temp database");
        db.initialize().expect("create the schema");
        Self { dir, db }
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The headless screen, wide and tall enough for both docks.
fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
        events,
        ..Default::default()
    }
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

/// The file names that the frame painted, in paint order.
///
/// The grid paints one tile after the next in display order.
fn painted_tile_names(output: &egui::FullOutput) -> Vec<String> {
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
    let names = ["a.png", "a 2.png", "a 10.png", "b.png"];
    text.into_iter()
        .filter(|text| names.contains(&text.as_str()))
        .collect()
}

/// The fill and the stroke colour of the rect that the frame painted at `rect`.
fn painted_rect_fill(
    output: &egui::FullOutput,
    rect: Rect,
) -> Option<(egui::Color32, egui::Color32)> {
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
fn pill_colours(
    ctx: &Context,
    output: &egui::FullOutput,
    mode: SortMode,
) -> (egui::Color32, egui::Color32) {
    let rect = ctx
        .read_response(Id::new(("tarragon-sort-pill", mode.as_str())))
        .expect("the pill registered a response")
        .rect;
    painted_rect_fill(output, rect).expect("the pill painted a rect")
}

/// Draw one frame of the shell.
fn draw(ctx: &Context, shell: &mut Shell, events: Vec<Event>) -> egui::FullOutput {
    ctx.run_ui(input(events), |ui| shell.show(ui))
}

#[test]
fn the_stored_sort_mode_reaches_the_gallery_and_back() {
    let temp = TempDb::new();
    let settings = Settings::new(&temp.db);

    // Store the date order, as the app does when it saves the window state.
    let mut state = WindowState::default();
    state.set_sort_mode(SortMode::Date);
    settings
        .set_window_state_value(&state)
        .expect("write the window state");

    // Build the shell from the stored state.
    let stored = settings
        .window_state_value()
        .expect("read the window state")
        .expect("a stored window state");
    let ctx = Context::default();
    tarragon_ui::apply_theme(&ctx);
    let mut shell = Shell::new().with_sort_mode(stored.sort_mode());
    shell.set_folder(Some(PathBuf::from("/images")));
    shell.set_items(dated_items());

    let output = draw(&ctx, &mut shell, Vec::new());
    let date_pill = pill_colours(&ctx, &output, SortMode::Date);
    let name_pill = pill_colours(&ctx, &output, SortMode::Natural);
    let names = painted_tile_names(&output);
    output.drop_without_applying_deltas();

    assert_eq!(
        date_pill,
        (tokens::BG_TERTIARY, tokens::AMBER_ACCENT),
        "the restored Date pill is active"
    );
    assert_eq!(
        name_pill,
        (tokens::BG_SECONDARY, egui::Color32::TRANSPARENT),
        "the Name pill is inactive"
    );
    assert_eq!(
        names,
        ["a.png", "b.png", "a 2.png", "a 10.png"],
        "the gallery paints the stored date order"
    );

    // Click the Name pill and take the changed mode.
    let pos = ctx
        .read_response(Id::new(("tarragon-sort-pill", "natural")))
        .expect("the Name pill registered")
        .rect
        .center();
    let press = |pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [
        vec![Event::PointerMoved(pos)],
        vec![press(true)],
        vec![press(false)],
    ] {
        draw(&ctx, &mut shell, events).drop_without_applying_deltas();
    }
    let changed = shell
        .take_sort_changed()
        .expect("the click changed the mode");

    // Store the changed mode and read it back.
    state.set_sort_mode(changed);
    settings
        .set_window_state_value(&state)
        .expect("write the changed window state");
    let stored = settings
        .window_state_value()
        .expect("read the changed window state")
        .expect("a stored window state");

    assert_eq!(stored.sort_mode, Some("natural".to_owned()));
    assert_eq!(stored.sort_mode(), SortMode::Natural);
}
