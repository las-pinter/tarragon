//! Tile helpers: composite a canvas in square tiles and stitch the pixels.

use tarragon_psd::{ParseOptions, PsdDocument, Viewport};

/// Composite `width` by `height` in square tiles of `edge` and stitch the RGBA8
/// bytes row by row.
///
/// # Panics
///
/// Panics when a tile is refused.
#[must_use]
pub fn stitched_rgba8(document: &PsdDocument, width: u32, height: u32, edge: u32) -> Vec<u8> {
    let mut out = vec![0u8; width as usize * height as usize * 4];
    for top in (0..height).step_by(edge as usize) {
        for left in (0..width).step_by(edge as usize) {
            let right = (left + edge).min(width);
            let bottom = (top + edge).min(height);
            let tile = document
                .composite_region(
                    Viewport::new(left, top, right, bottom),
                    &ParseOptions::none(),
                )
                .expect("the tile fits the budget");
            let rgba = tile.rgba8();
            let row = tile.width() as usize * 4;
            for y in 0..tile.height() as usize {
                let at = ((top as usize + y) * width as usize + left as usize) * 4;
                out[at..at + row].copy_from_slice(&rgba[y * row..(y + 1) * row]);
            }
        }
    }
    out
}

/// Cut the rectangle `view` out of stitched RGBA8 bytes of a canvas `width`
/// pixels wide.
#[must_use]
pub fn crop_rgba8(stitched: &[u8], width: u32, view: Viewport) -> Vec<u8> {
    let mut out = Vec::new();
    for y in view.top..view.bottom {
        let start = (y as usize * width as usize + view.left as usize) * 4;
        out.extend_from_slice(&stitched[start..start + view.width() as usize * 4]);
    }
    out
}

/// The peak resident memory of this process in bytes, or `None` where the
/// platform does not report it or the process is shared.
///
/// The peak covers the whole process, so it is only meaningful when the test
/// runs alone in a fresh process. `cargo nextest` sets `NEXTEST` for that case;
/// elsewhere other tests in the same process raise the peak, and this gives
/// `None`. The figure is a smoke check against runaway memory, not a bound on
/// one estimate.
#[must_use]
pub fn peak_resident_bytes() -> Option<u64> {
    std::env::var_os("NEXTEST")?;
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// Assert that a tile of the suggested edge fits the budget when it grows by
/// `padding` pixels on each side, bounded by the canvas.
///
/// # Panics
///
/// Panics when the working size is over `max_tile_pixels`.
pub fn assert_suggested_edge_fits(document: &PsdDocument, canvas: u32, padding: u32) {
    let edge = document.suggested_tile_edge().expect("the edge");
    let working = u64::from((edge + 2 * padding).min(canvas));
    let allowed = document.max_tile_pixels().expect("the pixels");
    assert!(
        working * working <= allowed,
        "working {working} squared is over {allowed}"
    );
}
