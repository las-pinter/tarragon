//! Tests for the driven scroll of the grid.

use super::*;

/// 400 items in one folder, so the grid is far taller than the screen.
fn many_items() -> Vec<GridItem> {
    (0..400)
        .map(|i| GridItem::new(PathBuf::from(format!("/images/{i:04}.png")), i, None))
        .collect()
}

fn tall_gallery(ctx: &egui::Context) -> Gallery {
    crate::theme::apply_theme(ctx);
    let mut gallery = Gallery::new(SortMode::Natural);
    gallery.set_folder(Some(PathBuf::from("/images")));
    gallery.set_items(many_items());
    gallery
}

#[test]
fn the_offset_goes_down_and_back_on_a_triangle_path() {
    let at = |ms| scroll_offset(Duration::from_millis(ms), 2000.0, 800.0);
    let near = |got: f32, want: f32| assert!((got - want).abs() < 0.01, "{got} vs {want}");
    // Travel is 1200 pt; at 60000 pt/s the end comes at 20 ms.
    near(at(0), 0.0);
    near(at(10), 600.0);
    near(at(20), 1200.0);
    near(at(30), 600.0);
    near(at(40), 0.0);
    near(at(50), 600.0);
    // Content that fits the view does not move.
    near(scroll_offset(Duration::from_secs(3), 500.0, 800.0), 0.0);
}

#[test]
fn the_setter_moves_the_grid_and_nothing_else_does() {
    let ctx = egui::Context::default();
    let mut gallery = tall_gallery(&ctx);
    for _ in 0..3 {
        frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    }
    let (content, view) = gallery.scroll_extent();
    assert!(
        content > view + 300.0,
        "the grid must be taller than the view"
    );
    assert!(gallery.scroll_y().abs() < 0.01, "a quiet grid moved");

    gallery.set_scroll_offset(300.0);
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert!((gallery.scroll_y() - 300.0).abs() < 0.01);

    // The offset applies once; a later frame keeps it.
    frame(&ctx, &mut gallery, Vec::new()).drop_without_applying_deltas();
    assert!((gallery.scroll_y() - 300.0).abs() < 0.01);
}
