//! Tests for the gallery strip and the search timer.

use std::path::PathBuf;

use super::strip::sort_label;
use super::*;
use egui::{Event, Id, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};

/// Four items in one folder. Two files share one mtime, so the date tie shows.
fn items() -> Vec<GridItem> {
    let item =
        |name: &str, mtime: i64| GridItem::new(PathBuf::from("/images").join(name), mtime, None);
    vec![
        item("b.png", 10),
        item("a.png", 20),
        item("a 10.png", 5),
        item("a 2.png", 5),
    ]
}

/// Two folders. The first folder holds the [`items`] list.
fn items_in_two_folders() -> Vec<GridItem> {
    let item = |folder: &str, name: &str, mtime: i64| {
        GridItem::new(
            PathBuf::from("/library").join(folder).join(name),
            mtime,
            None,
        )
    };
    vec![
        item("sketches", "b.png", 10),
        item("sketches", "a.png", 20),
        item("renders", "c.png", 30),
        item("renders", "d.png", 40),
    ]
}

/// The `sketches` folder of [`items_in_two_folders`].
fn sketches() -> PathBuf {
    PathBuf::from("/library/sketches")
}

/// Type `text` and let the quiet window pass.
fn search(gallery: &mut Gallery, text: &str) {
    let start = Instant::now();
    gallery.search = text.to_owned();
    gallery.debouncer.arm(start);
    gallery.apply_pending_search(start + SEARCH_DEBOUNCE);
}

/// The headless screen that the frame helpers draw into.
fn screen() -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))
}

/// Draw one headless frame of the gallery with the given events.
fn frame(ctx: &egui::Context, gallery: &mut Gallery, events: Vec<Event>) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    ctx.run_ui(input, |ui| gallery.show(ui))
}

/// Click the widget with `id` in a headless context.
fn click(ctx: &egui::Context, gallery: &mut Gallery, id: Id) {
    let pos = ctx
        .read_response(id)
        .expect("the widget registered a response")
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
        frame(ctx, gallery, events).drop_without_applying_deltas();
    }
}

#[test]
fn a_debouncer_without_arm_is_never_ready() {
    let mut debouncer = Debouncer::default();
    assert!(!debouncer.is_ready(Instant::now()));
    assert_eq!(debouncer.remaining(Instant::now()), None);
}

#[test]
fn the_search_window_is_exactly_300_ms() {
    assert_eq!(SEARCH_DEBOUNCE, Duration::from_millis(300));
}

#[test]
fn the_debouncer_waits_for_the_quiet_window() {
    let start = Instant::now();
    let mut debouncer = Debouncer::default();
    debouncer.arm(start);
    assert!(!debouncer.is_ready(start));
    assert!(!debouncer.is_ready(start + Duration::from_millis(299)));
    assert!(debouncer.is_ready(start + SEARCH_DEBOUNCE));
    assert!(!debouncer.is_ready(start + SEARCH_DEBOUNCE), "ready clears");
}

#[test]
fn the_debouncer_reports_the_time_left() {
    let start = Instant::now();
    let mut debouncer = Debouncer::default();
    debouncer.arm(start);
    assert_eq!(debouncer.remaining(start), Some(SEARCH_DEBOUNCE));
    assert_eq!(
        debouncer.remaining(start + Duration::from_millis(200)),
        Some(Duration::from_millis(100))
    );
    assert_eq!(
        debouncer.remaining(start + SEARCH_DEBOUNCE),
        Some(Duration::ZERO),
        "a deadline that passed leaves no time, but the wake is still owed"
    );
}

#[test]
fn typing_rearms_the_quiet_window() {
    let start = Instant::now();
    let mut debouncer = Debouncer::default();
    debouncer.arm(start);
    debouncer.arm(start + Duration::from_millis(200));
    assert!(!debouncer.is_ready(start + Duration::from_millis(450)));
    assert!(debouncer.is_ready(start + Duration::from_millis(500)));
}

#[test]
fn set_items_sorts_in_the_current_mode() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    assert_eq!(gallery.order, [3, 2, 1, 0]);

    let mut dated = Gallery::new(SortMode::Date);
    dated.set_folder(Some(PathBuf::from("/images")));
    dated.set_items(items());
    assert_eq!(dated.order, [1, 0, 3, 2]);
}

#[test]
fn set_items_drops_stale_selection_indices() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items());
    gallery.selection.select_only(3);
    let only = GridItem::new(PathBuf::from("/images/a.png"), 1, None);
    gallery.set_items(vec![only]);
    assert!(gallery.selection.is_empty(), "index 3 is now stale");
}

#[test]
fn set_sort_mode_resorts_and_clears_the_changed_flag() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery.sort_changed = true;

    gallery.set_sort_mode(SortMode::Date);
    assert_eq!(gallery.sort_mode(), SortMode::Date);
    assert_eq!(gallery.order, [1, 0, 3, 2], "the order follows the mode");
    assert_eq!(gallery.take_sort_changed(), None, "the flag cleared");
}

#[test]
fn an_empty_gallery_shows_nothing() {
    let gallery = Gallery::new(SortMode::Natural);
    assert_eq!(gallery.visible_len(), 0);
    assert_eq!(gallery.sort_mode(), SortMode::Natural);
}

#[test]
fn the_visible_filter_waits_for_the_window() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    let start = Instant::now();
    gallery.search = "png".to_owned();
    gallery.debouncer.arm(start);

    gallery.apply_pending_search(start);
    assert_eq!(gallery.visible_len(), 4, "the filter waits");

    gallery.apply_pending_search(start + SEARCH_DEBOUNCE);
    assert_eq!(gallery.visible_len(), 4, "every name holds png");
}

#[test]
fn the_debounce_narrows_the_visible_set_after_the_window() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    let start = Instant::now();
    gallery.search = "a 2".to_owned();
    gallery.debouncer.arm(start);

    gallery.apply_pending_search(start + Duration::from_millis(299));
    assert_eq!(gallery.visible_len(), 4, "before the deadline");

    gallery.apply_pending_search(start + SEARCH_DEBOUNCE);
    assert_eq!(gallery.visible_len(), 1, "only a 2.png matches");
}

#[test]
fn set_items_refreshes_the_cached_name() {
    let mut gallery = Gallery::new(SortMode::Natural);
    let mut item = GridItem::new(PathBuf::from("/images/a.png"), 1, None);
    item.path = PathBuf::from("/images/B.PNG");
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(vec![item]);
    search(&mut gallery, "b.png");
    assert_eq!(gallery.visible_len(), 1, "the cache followed the new path");
}

#[test]
fn search_matches_the_file_name_ignoring_case() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    search(&mut gallery, "A.PNG");
    assert_eq!(gallery.visible_len(), 1, "only a.png matches");
}

#[test]
fn search_uses_the_text_without_trim() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    search(&mut gallery, " a");
    assert_eq!(gallery.visible_len(), 0, "no name holds a leading space");
    search(&mut gallery, "a ");
    assert_eq!(gallery.visible_len(), 2, "the two numbered names match");
}

#[test]
fn a_whitespace_only_search_is_one_active_filter() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    search(&mut gallery, "   ");
    assert_eq!(gallery.visible_len(), 0);
}

#[test]
fn the_folder_scope_shows_only_the_folder_items() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));
    assert_eq!(gallery.visible_len(), 2, "the folder holds two items");

    gallery.set_scope(Scope::All);
    assert_eq!(gallery.visible_len(), 4, "the global scope holds four");
}

#[test]
fn the_all_scope_yields_more_items_than_the_folder_scope() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));

    gallery.set_scope(Scope::All);
    let all = gallery.visible_len();
    gallery.set_scope(Scope::Folder);
    let folder = gallery.visible_len();
    assert!(
        all > folder,
        "All shows {all} items but Folder shows {folder}"
    );
}

#[test]
fn the_folder_scope_without_a_folder_shows_nothing() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    assert_eq!(gallery.visible_len(), 0, "no folder is set");
    assert_eq!(gallery.info_text(), "No folder - 0 files");

    gallery.set_scope(Scope::All);
    assert_eq!(gallery.visible_len(), 4, "the global scope stays full");
}

#[test]
fn the_folder_scope_ignores_a_folder_that_holds_no_item() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(PathBuf::from("/library/other")));
    assert_eq!(gallery.visible_len(), 0);
}

#[test]
fn set_folder_normalizes_the_path_once() {
    // A real folder is needed, because canonicalization reads the file system.
    let base = std::env::temp_dir()
        .canonicalize()
        .expect("the temp dir exists")
        .join(format!("tarragon-ui-folder-{}", std::process::id()));
    let child = base.join("child");
    std::fs::create_dir_all(&child).expect("create the test folder");

    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(vec![GridItem::new(child.join("a.png"), 1, None)]);
    gallery.set_folder(Some(child.join("..").join("child")));
    let visible = gallery.visible_len();
    std::fs::remove_dir_all(&base).ok();

    assert_eq!(visible, 1, "the unnormalized path still finds the folder");
}

#[test]
fn the_info_bar_names_the_folder_and_the_count() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items());
    gallery.set_folder(Some(PathBuf::from("/images")));
    assert_eq!(gallery.info_text(), "images - 4 files");

    gallery.set_scope(Scope::All);
    assert_eq!(gallery.info_text(), "All Images - 4 files");
}

#[test]
fn the_info_bar_adds_the_selection_count() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items());
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.selection.select_only(1);
    gallery.selection.toggle(2);
    assert_eq!(gallery.info_text(), "images - 4 files - 2 selected");
}

#[test]
fn the_scope_name_falls_back_when_no_folder_exists() {
    let gallery = Gallery::new(SortMode::Natural);
    assert_eq!(gallery.scope_name(), "No folder");
}

#[test]
fn the_sort_labels_name_the_two_modes() {
    assert_eq!(sort_label(SortMode::Natural), "Name");
    assert_eq!(sort_label(SortMode::Date), "Date");
}

#[test]
fn the_sort_change_reports_once() {
    let mut gallery = Gallery::new(SortMode::Natural);
    assert_eq!(gallery.take_sort_changed(), None);
    gallery.sort_mode = SortMode::Date;
    gallery.sort_changed = true;
    assert_eq!(gallery.take_sort_changed(), Some(SortMode::Date));
    assert_eq!(gallery.take_sort_changed(), None, "the flag clears");
}

#[test]
fn a_tab_switch_prunes_the_selection_to_the_visible_items() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));
    gallery.set_scope(Scope::All);
    // Indices 2 and 3 sit in the other folder, so the folder scope hides them.
    gallery.selection.select_only(1);
    gallery.selection.toggle(2);

    gallery.set_scope(Scope::Folder);

    assert_eq!(
        gallery.selection.iter().collect::<Vec<_>>(),
        [1],
        "the hidden item left the selection"
    );
}

#[test]
fn a_gallery_frame_draws_the_strip_and_the_grid() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    let output = frame(&ctx, &mut gallery, Vec::new());
    let drawn = output.shapes.len();
    output.drop_without_applying_deltas();
    assert!(drawn > 0, "the gallery drew nothing");
}

#[test]
fn a_quiet_frame_keeps_the_cached_visible_order() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    let before = gallery.visible_order().as_ptr();

    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();

    assert_eq!(
        gallery.visible_order().as_ptr(),
        before,
        "a quiet frame rebuilt the cached order"
    );
}

#[test]
fn a_sort_change_rebuilds_the_cached_visible_order() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    assert_eq!(gallery.visible_order(), [3, 2, 1, 0]);

    gallery.set_sort_mode(SortMode::Date);

    assert_eq!(
        gallery.visible_order(),
        [1, 0, 3, 2],
        "the cache followed the sort mode"
    );
}

#[test]
fn a_search_that_narrows_the_view_prunes_the_selection_at_once() {
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    gallery.selection.select_only(0);
    gallery.selection.toggle(3);

    search(&mut gallery, "a 2");

    assert_eq!(gallery.selection.iter().collect::<Vec<_>>(), [3]);
}

#[test]
fn a_tab_click_switches_the_scope_and_the_visible_count() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_items(items_in_two_folders());
    gallery.set_folder(Some(sketches()));
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert_eq!(gallery.scope(), Scope::Folder);
    assert_eq!(gallery.visible_len(), 2);

    click(
        &ctx,
        &mut gallery,
        Id::new(("tarragon-scope-tab", "All Images")),
    );

    assert_eq!(gallery.scope(), Scope::All, "the tab switched the scope");
    assert_eq!(gallery.visible_len(), 4, "the All scope shows every item");
}

#[test]
fn the_search_timer_wakes_itself_without_further_input() {
    let ctx = egui::Context::default();
    crate::theme::apply_theme(&ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(items());
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();

    // Focus the search field and type.
    click(&ctx, &mut gallery, Id::new("tarragon-gallery-search"));
    frame(&ctx, &mut gallery, vec![Event::Text("a 2".to_owned())]).drop_without_applying_deltas();
    assert_eq!(gallery.search, "a 2", "the text reached the field");
    assert_eq!(gallery.visible_len(), 4, "the filter still waits");

    // A wake frame arrives early and must ask for the time that is left. The
    // first quiet frame clears the immediate repaint that the typing asked for.
    let early = frame(&ctx, &mut gallery, Vec::new());
    early.drop_without_applying_deltas();
    assert_eq!(gallery.visible_len(), 4, "the early frame must not filter");

    let quiet = frame(&ctx, &mut gallery, Vec::new());
    let delay = quiet
        .viewport_output
        .values()
        .map(|viewport| viewport.repaint_delay)
        .min()
        .expect("the frame reports a repaint delay");
    quiet.drop_without_applying_deltas();
    assert_eq!(gallery.visible_len(), 4, "the quiet frame must not filter");
    assert!(
        delay > SEARCH_DEBOUNCE / 2 && delay <= SEARCH_DEBOUNCE + WAKE_SLACK,
        "the delayed wake is not requested again, delay is {delay:?}"
    );

    std::thread::sleep(SEARCH_DEBOUNCE + Duration::from_millis(20));

    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert_eq!(
        gallery.visible_len(),
        1,
        "the timer woke the filter with no input"
    );
}
