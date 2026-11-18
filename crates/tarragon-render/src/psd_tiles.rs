//! Tile loop for the PSD layer compositor.
//!
//! The compositor never holds the whole canvas as float planes. This module
//! asks it for square tiles and copies each tile into one RGBA image.
//!
//! The tile edge comes from the document, which sizes it so the tile plus the
//! effect padding stays inside the memory budget and the full-tier edge. The
//! compositor pads each tile by the reach of its effects, so a shadow or a
//! glow that crosses a tile edge is cut by neither tile. The padding holds only
//! while the tile edge plus the padding fits the full-tier edge, which is why
//! the edge must come from the document.

use image::RgbaImage;
use tarragon_psd::{ParseOptions, PsdDocument, PsdError, Viewport};

/// Composite the whole canvas in square tiles of `edge` and join them.
///
/// The canvas must fit the full-tier edge: the image is held whole.
///
/// # Errors
///
/// Returns the compositor error for a tile: the tile is over the memory
/// budget, the color mode is unsupported, the bytes are truncated, the deadline
/// passed, or the work was cancelled.
pub(crate) fn composite_tiled(
    document: &PsdDocument,
    edge: u32,
    options: &ParseOptions,
) -> Result<RgbaImage, PsdError> {
    let header = document.file().header;
    let (width, height) = (header.width, header.height);
    let edge = edge.max(1);
    let mut image = RgbaImage::new(width, height);
    let row_pixels = width as usize;
    let mut top = 0;
    while top < height {
        let bottom = top.saturating_add(edge).min(height);
        let mut left = 0;
        while left < width {
            let right = left.saturating_add(edge).min(width);
            let tile =
                document.composite_region(Viewport::new(left, top, right, bottom), options)?;
            let rgba = tile.rgba8();
            let tile_row = tile.width() as usize * 4;
            let buffer: &mut [u8] = &mut image;
            for (y, source) in rgba.chunks_exact(tile_row).enumerate() {
                let at = ((top as usize + y) * row_pixels + left as usize) * 4;
                if let Some(target) = buffer.get_mut(at..at + tile_row) {
                    target.copy_from_slice(source);
                }
            }
            left = right;
        }
        top = bottom;
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use tarragon_psd::{ParseOptions, PsdDocument};

    use crate::psd_builder::{
        DocumentSpec, LayerSpec, build_psd, lfx2, shadow_effect, square_document_with_fill_opacity,
    };
    use crate::psd_tiles::composite_tiled;

    /// A gray backdrop with a red square at 24..40 and a drop shadow that
    /// reaches across several tile edges.
    fn shadow_document() -> PsdDocument {
        let shadow = shadow_effect(false, [0.0; 3], 120.0, 8.0, 6.0, 0.0, 75.0, true);
        let bytes =
            square_document_with_fill_opacity(*b"lfx2", lfx2(&[shadow]), [200, 60, 60], None);
        PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("parses")
    }

    #[test]
    fn tiles_give_the_same_pixels_as_one_tile() {
        let document = shadow_document();
        let options = ParseOptions::none();
        let whole = composite_tiled(&document, 64, &options).expect("one tile");
        // Edges that cut through the square and through its shadow.
        for edge in [7, 16, 20, 33, 63] {
            let tiled = composite_tiled(&document, edge, &options).expect("tiles");
            assert_eq!(tiled, whole, "tile edge {edge}");
        }
    }

    #[test]
    fn the_shadow_is_present_in_the_joined_image() {
        let document = shadow_document();
        let image = composite_tiled(&document, 16, &ParseOptions::none()).expect("tiles");
        assert_eq!(image.dimensions(), (64, 64));
        assert_eq!(image.get_pixel(5, 5).0, [200, 200, 200, 255]);
        assert_eq!(image.get_pixel(30, 30).0[..3], [200, 60, 60]);
        assert!(image.get_pixel(42, 45).0[0] < 200, "the shadow darkens");
    }

    #[test]
    fn an_edge_over_the_canvas_and_a_zero_edge_both_work() {
        let mut spec = DocumentSpec::new(10, 6, 8, 3);
        spec.layers
            .push(LayerSpec::opaque((0, 0, 6, 10), [9, 8, 7]));
        let document =
            PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
        let options = ParseOptions::none();
        let big = composite_tiled(&document, 5000, &options).expect("big edge");
        let zero = composite_tiled(&document, 0, &options).expect("zero edge");
        assert_eq!(big.dimensions(), (10, 6));
        assert_eq!(big, zero);
        assert_eq!(big.get_pixel(9, 5).0, [9, 8, 7, 255]);
    }
}
