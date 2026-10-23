//! Tests for headless frames.

use super::*;

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
