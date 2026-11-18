//! The grid and the shell lay out in points, so a display scale changes only
//! the pixel density. Each test runs headless: no window and no GPU.

use std::collections::BTreeSet;
use std::path::PathBuf;

use egui::{
    Color32, ColorImage, Context, Id, Pos2, RawInput, Rect, TextureHandle, TextureOptions, Vec2,
    ViewportId, ViewportIdMap, ViewportInfo, epaint, vec2,
};

use super::{CELL_HEIGHT, GridItem, Selection, VirtualGrid, metrics_for};
use crate::shell::Shell;
use crate::tokens;

/// The display scales under test.
const SCALES: [f32; 3] = [1.0, 1.5, 2.0];
/// Sample tiles in each run.
const TILES: usize = 12;
/// Grid width in points.
const GRID_WIDTH: f32 = 1200.0;
/// Shell width in points.
const SHELL_WIDTH: f32 = 1280.0;
/// Grid height in points.
const SCREEN_HEIGHT: f32 = 900.0;
/// Shell height in points. It is tall so the narrow gallery pane shows every tile.
const SHELL_HEIGHT: f32 = 1700.0;
/// Frames before a measured frame, so the first-frame repaints stop.
const SETTLE: usize = 6;
/// Point difference that counts as equal.
const EPSILON: f32 = 1e-3;

/// What one frame painted, in points.
struct Seen {
    /// Rect of each tile, by index.
    cells: Vec<Rect>,
    /// Size of each thumbnail picture.
    thumbs: Vec<Vec2>,
    /// Bottom right corner of each tile name, by text.
    labels: Vec<(String, Pos2)>,
    /// Text and font size of every text shape.
    texts: BTreeSet<(String, u32)>,
    /// Tessellated clip rects, rounded to thousandths of a point.
    clips: BTreeSet<[i32; 4]>,
}

/// Raw input for a screen of `size` points at display scale `scale`.
fn raw(scale: f32, size: Vec2) -> RawInput {
    let mut viewports = ViewportIdMap::default();
    viewports.insert(
        ViewportId::ROOT,
        ViewportInfo {
            native_pixels_per_point: Some(scale),
            ..Default::default()
        },
    );
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        viewports,
        ..Default::default()
    }
}

/// A themed context with the app fonts.
fn themed() -> Context {
    let ctx = Context::default();
    crate::install(&ctx);
    crate::apply_theme(&ctx);
    ctx
}

/// Twelve tiles with a square picture each. One name is too long for a tile.
fn tiles(ctx: &Context) -> Vec<GridItem> {
    let texture: TextureHandle = ctx.load_texture(
        "scale-test",
        ColorImage::filled([8, 8], Color32::GRAY),
        TextureOptions::LINEAR,
    );
    (0..TILES)
        .map(|index| {
            GridItem::new(
                PathBuf::from("/images").join(tile_name(index)),
                1,
                Some(texture.clone()),
            )
        })
        .collect()
}

/// The file name of tile `index`. Tile 3 has a name too long for a tile.
fn tile_name(index: usize) -> String {
    if index == 3 {
        "a_very_long_reference_sketch_name_for_the_overflow_case.jpg".to_owned()
    } else {
        format!("IMG_{index:04}.jpg")
    }
}

/// Key a rect by thousandths of a point.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a rect edge within a few thousand points fits in i32 thousandths"
)]
fn key(rect: Rect) -> [i32; 4] {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y].map(|edge| (edge * 1000.0).round() as i32)
}

/// Collect the text and the picture shapes of `shape` into `seen`.
fn collect(shape: &epaint::Shape, seen: &mut Seen) {
    match shape {
        epaint::Shape::Text(text) => {
            let size = text
                .galley
                .job
                .sections
                .first()
                .map_or(0.0, |section| section.format.font_id.size);
            let name = text.galley.job.text.clone();
            seen.texts.insert((name.clone(), size.to_bits()));
            if (size - tokens::SIZE_SMALL).abs() < EPSILON
                && (0..TILES).any(|index| tile_name(index) == name)
            {
                seen.labels.push((name, text.pos + text.galley.size()));
            }
        }
        epaint::Shape::Rect(rect) if rect.brush.is_some() => {
            seen.thumbs.push(rect.rect.size());
        }
        epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                collect(shape, seen);
            }
        }
        _ => {}
    }
}

/// Settle `show` for a few frames, then record one more.
fn observe(ctx: &Context, scale: f32, size: Vec2, mut show: impl FnMut(&mut egui::Ui)) -> Seen {
    for _ in 0..SETTLE {
        ctx.run_ui(raw(scale, size), &mut show)
            .drop_without_applying_deltas();
    }
    let output = ctx.run_ui(raw(scale, size), &mut show);
    // A run that ignores the requested scale would pass every check at 1.0.
    assert!(
        (output.pixels_per_point - scale).abs() < EPSILON,
        "the frame ran at {} instead of {scale}",
        output.pixels_per_point
    );
    let mut seen = Seen {
        cells: Vec::new(),
        thumbs: Vec::new(),
        labels: Vec::new(),
        texts: BTreeSet::new(),
        clips: BTreeSet::new(),
    };
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut seen);
    }
    for primitive in ctx.tessellate(output.shapes, output.pixels_per_point) {
        seen.clips.insert(key(primitive.clip_rect));
    }
    seen.cells = (0..TILES)
        .filter_map(|index| ctx.read_response(Id::new(("tarragon-grid-tile", index))))
        .map(|response| response.rect)
        .collect();
    seen
}

/// The grid alone at `scale`.
fn grid_at(scale: f32) -> Seen {
    let ctx = themed();
    let items = tiles(&ctx);
    let order: Vec<usize> = (0..items.len()).collect();
    let selection = Selection::default();
    let grid = VirtualGrid {
        metrics: metrics_for(GRID_WIDTH),
        item_count: items.len(),
    };
    observe(&ctx, scale, vec2(GRID_WIDTH, SCREEN_HEIGHT), |ui| {
        grid.show_items_at(ui, 0.0, &items, &order, &selection);
    })
}

/// The whole shell with the sample tiles at `scale`.
fn shell_at(scale: f32) -> Seen {
    let ctx = themed();
    let mut shell = Shell::new();
    shell.set_folder(Some(PathBuf::from("/images")));
    shell.set_items(tiles(&ctx));
    observe(&ctx, scale, vec2(SHELL_WIDTH, SHELL_HEIGHT), |ui| {
        shell.show(ui);
    })
}

/// Number of tiles in the first row.
fn columns(seen: &Seen) -> usize {
    let top = seen.cells.first().map_or(0.0, |cell| cell.min.y);
    seen.cells
        .iter()
        .filter(|cell| (cell.min.y - top).abs() < EPSILON)
        .count()
}

/// Whether each rect of `from` has a rect in `to` within `tolerance` points on
/// every edge.
fn near_all(from: &BTreeSet<[i32; 4]>, to: &BTreeSet<[i32; 4]>, tolerance: f32) -> bool {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the tolerance in thousandths fits in i32"
    )]
    let limit = (tolerance * 1000.0).ceil() as i32;
    from.iter().all(|a| {
        to.iter()
            .any(|b| a.iter().zip(b).all(|(x, y)| (x - y).abs() <= limit))
    })
}

/// Assert that two sizes match within [`EPSILON`].
fn assert_size(actual: Vec2, expected: Vec2, what: &str) {
    assert!(
        (actual - expected).abs().max_elem() < EPSILON,
        "{what}: {actual:?} is not {expected:?}"
    );
}

#[test]
fn a_thumbnail_is_one_token_square_in_points_at_every_scale() {
    let square = Vec2::splat(tokens::THUMBNAIL_SIZE);
    for scale in SCALES {
        for seen in [grid_at(scale), shell_at(scale)] {
            assert_eq!(seen.thumbs.len(), TILES, "scale {scale}");
            for thumb in &seen.thumbs {
                assert_size(*thumb, square, &format!("thumbnail at scale {scale}"));
            }
        }
    }
}

#[test]
fn the_cell_and_the_column_count_do_not_change_with_the_scale() {
    let base = grid_at(1.0);
    let columns_at_base = columns(&base);
    assert_eq!(base.cells.len(), TILES);
    assert!(columns_at_base > 1, "the 1200 pt grid holds one column");
    for scale in SCALES {
        let seen = grid_at(scale);
        assert_eq!(columns(&seen), columns_at_base, "columns at scale {scale}");
        for (cell, first) in seen.cells.iter().zip(&base.cells) {
            assert_size(cell.size(), first.size(), "cell");
            assert_size(cell.min.to_vec2(), first.min.to_vec2(), "cell origin");
        }
        assert!((seen.cells[0].height() - CELL_HEIGHT).abs() < EPSILON);
    }
}

#[test]
fn the_clip_rects_and_the_font_sizes_do_not_change_with_the_scale() {
    for (run, shell) in [(grid_at as fn(f32) -> Seen, false), (shell_at, true)] {
        let base = run(1.0);
        for scale in SCALES {
            let seen = run(scale);
            // The dock can move an edge past one pixel; this sample does not,
            // so a new pane size may fail.
            let tolerance = if shell { 1.0 / scale } else { EPSILON };
            assert!(
                near_all(&seen.clips, &base.clips, tolerance)
                    && near_all(&base.clips, &seen.clips, tolerance),
                "clip rects at scale {scale}: {:?} against {:?}",
                seen.clips,
                base.clips
            );
            assert_eq!(seen.texts, base.texts, "text sizes at scale {scale}");
        }
    }
}

#[test]
fn a_tile_name_fits_inside_its_cell_at_every_scale() {
    for scale in SCALES {
        for seen in [grid_at(scale), shell_at(scale)] {
            assert_eq!(seen.labels.len(), TILES, "scale {scale}");
            for (name, corner) in &seen.labels {
                let index = (0..TILES).find(|index| tile_name(*index) == *name);
                let cell = &seen.cells[index.expect("a tile name")];
                assert!(
                    corner.y <= cell.max.y && corner.x <= cell.max.x,
                    "the name {name} ends at {corner:?}, past the cell {cell:?}, at scale {scale}"
                );
            }
        }
    }
}
