//! Renderer for Photoshop `.psd` and `.psb` documents.
//!
//! The renderer prefers the merged image, the flattened picture the authoring
//! program saved with the file. It reads far less data than the layer
//! compositor, and it matches what the artist saw when saving. The merged
//! image is used when all of these hold: the section parsed, it holds channels,
//! each channel covers the canvas, and the pixels decode. Any other case falls
//! back to the layer compositor. A merged image that is blank but the right
//! size passes: the renderer cannot tell a blank picture from a real one.
//!
//! A canvas over the full-tier edge reads the merged image through a step: the
//! output keeps every Nth pixel, N the smallest integer that fits both edges.
//! The reader skips the rows it does not keep, so memory stays near the output
//! size. The cache store scales the result to its tiers afterwards.
//!
//! The compositor path tiles the canvas, see [`crate::psd_tiles`]. A canvas
//! over the full-tier edge with no usable merged image returns
//! [`RenderError::TooLarge`]: a stepped composite would blur the effects and
//! the masks, so the renderer refuses instead of showing a wrong picture.

use std::path::Path;

use image::{DynamicImage, RgbaImage};
use tarragon_core::error::RenderError;
use tarragon_psd::{FULL_TIER_MAX_EDGE, ParseOptions, PsdDocument, PsdError, PsdFile, Viewport};

use crate::psd_tiles::composite_tiled;
use crate::renderer::RenderedImage;

/// The target for the transient memory of one merged read band, in bytes.
///
/// The band holds the decoded samples, the float color and alpha planes, and
/// the RGBA8 copy. The finished image comes on top of this.
const MERGED_BAND_BYTES: u64 = 64 * 1024 * 1024;
/// The bytes of the float planes and the RGBA8 copy for each band pixel: three
/// color and one alpha `f32`, then four bytes.
const BAND_PIXEL_FIXED_BYTES: u64 = 16 + 4;

/// Render a `.psd` or `.psb` file to a `DynamicImage`.
///
/// The renderer uses the merged image when it is usable and the layer
/// compositor otherwise. The image has at most 8192 pixels per edge. The cache
/// store owns the tier scaling. This entry uses the default 120 s deadline and
/// no cancel flag; [`render_psd_with`] takes explicit options.
///
/// # Errors
///
/// Returns [`RenderError::Io`] when the file cannot be read. Returns
/// [`RenderError::Decode`] when the file is malformed, uses an unsupported
/// color mode, or its layers cannot be composited. Returns
/// [`RenderError::TooLarge`] when the canvas is over 8192 pixels and the file
/// has no usable merged image. Returns [`RenderError::Timeout`] when the
/// deadline passes. This entry never returns [`RenderError::Cancelled`],
/// because it sets no cancel flag.
pub fn render_psd_image(path: &Path) -> Result<RenderedImage, RenderError> {
    render_psd_with(path, &ParseOptions::default())
}

/// Render a `.psd` or `.psb` file with explicit deadline and cancel options.
///
/// This is [`render_psd_image`] with the options chosen by the caller. The
/// registry entry cannot be cancelled; a caller that must cancel a render
/// calls this function.
///
/// # Errors
///
/// Returns the errors of [`render_psd_image`]. Returns
/// [`RenderError::Cancelled`] when the cancel flag of `options` is set. A
/// timeout or a cancel ends the render and never falls back to the layers.
pub fn render_psd_with(path: &Path, options: &ParseOptions) -> Result<RenderedImage, RenderError> {
    let fail = |error: PsdError| error.into_render_error(path);
    // A bad merged image must not fail the open when the layers can be drawn.
    let document = PsdDocument::open_lenient(path, options).map_err(fail)?;
    render_document(&document, options, MERGED_BAND_BYTES).map_err(fail)
}

/// Render an opened document: the merged image first, then the layers.
fn render_document(
    document: &PsdDocument,
    options: &ParseOptions,
    band_bytes: u64,
) -> Result<RenderedImage, PsdError> {
    match merged_image(document, options, band_bytes) {
        Ok(Some(image)) => return Ok(DynamicImage::ImageRgba8(image)),
        // A timeout or a cancel ends the render. Any other merged failure
        // sends the file to the compositor.
        Err(error) if error.is_transient() => return Err(error),
        Ok(None) | Err(_) => {}
    }
    let header = document.file().header;
    let edge = header.width.max(header.height);
    if edge > FULL_TIER_MAX_EDGE {
        return Err(PsdError::CapExceeded {
            edge,
            cap: FULL_TIER_MAX_EDGE,
        });
    }
    let tile = document.suggested_tile_edge()?;
    let image = composite_tiled(document, tile, options)?;
    Ok(DynamicImage::ImageRgba8(image))
}

/// True when the merged image is present, holds the color channels of the
/// color mode, and covers the whole canvas.
///
/// The parser builds every merged channel with the canvas rectangle, so a
/// healthy file always passes the size test. The test stays as a guard in case
/// that changes.
pub(crate) fn merged_is_usable(file: &PsdFile) -> bool {
    let header = file.header;
    let Ok(width) = i32::try_from(header.width) else {
        return false;
    };
    let Ok(height) = i32::try_from(header.height) else {
        return false;
    };
    let Some(color_channels) = header.color_mode.color_channels() else {
        return false;
    };
    let has_colors = (0..color_channels)
        .all(|id| i16::try_from(id).is_ok_and(|id| file.merged.channel(id).is_some()));
    file.merged_error.is_none()
        && has_colors
        && file.merged.channels.iter().all(|channel| {
            let bounds = channel.bounds;
            (bounds.left, bounds.top, bounds.right, bounds.bottom) == (0, 0, width, height)
        })
}

/// The smallest step that fits both canvas edges in the full-tier edge.
pub(crate) fn merged_step(width: u32, height: u32) -> u32 {
    width.max(height).div_ceil(FULL_TIER_MAX_EDGE).max(1)
}

/// Read the merged image, stepped when the canvas is over the cap.
///
/// The read runs in horizontal bands. Each band starts on a row that is a
/// multiple of the step, so the kept pixels line up across bands.
///
/// Returns `None` when the file has no usable merged image.
fn merged_image(
    document: &PsdDocument,
    options: &ParseOptions,
    band_bytes: u64,
) -> Result<Option<RgbaImage>, PsdError> {
    let file = document.file();
    if !merged_is_usable(file) {
        return Ok(None);
    }
    let header = file.header;
    let step = merged_step(header.width, header.height);
    let out_width = header.width.div_ceil(step);
    let out_height = header.height.div_ceil(step);
    let row_bytes = out_width as usize * 4;
    let mut pixels = vec![0u8; row_bytes * out_height as usize];
    // The decoder keeps a `u16` per sample whatever the file depth, so the
    // channel count sets the sample bytes.
    let decoded = file
        .merged
        .channels
        .iter()
        .filter(|channel| {
            channel.id == -1
                || usize::try_from(channel.id)
                    .is_ok_and(|id| Some(id) < header.color_mode.color_channels())
        })
        .count() as u64;
    let pixel_bytes = BAND_PIXEL_FIXED_BYTES + 2 * decoded;
    let band_rows = u32::try_from(band_bytes / (pixel_bytes * u64::from(out_width)))
        .unwrap_or(u32::MAX)
        .clamp(1, out_height);
    let mut out_top = 0u32;
    while out_top < out_height {
        let out_bottom = (out_top + band_rows).min(out_height);
        let band = Viewport::new(
            0,
            out_top * step,
            header.width,
            (out_bottom * step).min(header.height),
        );
        let region = document.read_merged_pixels(band, step, options)?;
        let start = out_top as usize * row_bytes;
        let rgba = region.rgba8();
        pixels
            .get_mut(start..start + rgba.len())
            .ok_or_else(|| too_large(out_width, out_height))?
            .copy_from_slice(&rgba);
        out_top = out_bottom;
    }
    Ok(RgbaImage::from_raw(out_width, out_height, pixels))
}

/// The error for an image buffer that does not match its size.
fn too_large(width: u32, height: u32) -> PsdError {
    PsdError::LengthTooLarge {
        length: u64::from(width) * u64::from(height),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use tarragon_psd::{Deadline, ParseOptions, PsdDocument, PsdError};

    use crate::psd::{merged_image, merged_is_usable, merged_step, render_document};
    use crate::psd_builder::{DocumentSpec, LayerSpec, build_psd, rle_merged_section};

    /// A gray document whose merged image differs at every pixel.
    fn gray_document(width: usize, height: usize) -> PsdDocument {
        let plane: Vec<u8> = (0..width * height)
            .map(|index| u8::try_from((index * 5 + index / width * 11) % 253).expect("u8"))
            .collect();
        let mut spec = DocumentSpec::new(
            u32::try_from(width).expect("width"),
            u32::try_from(height).expect("height"),
            8,
            1,
        );
        spec.merged_section = Some(rle_merged_section(width, &[plane], false));
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses")
    }

    #[test]
    fn the_step_is_the_smallest_that_fits_both_edges() {
        assert_eq!(merged_step(100, 100), 1);
        assert_eq!(merged_step(8192, 8192), 1);
        assert_eq!(merged_step(8193, 10), 2);
        assert_eq!(merged_step(10, 16_384), 2);
        assert_eq!(merged_step(16_385, 10), 3);
        assert_eq!(merged_step(300_000, 1), 37);
    }

    #[test]
    fn a_merged_image_is_split_into_bands_without_changing_a_pixel() {
        let document = gray_document(41, 29);
        let options = ParseOptions::none();
        let whole = merged_image(&document, &options, u64::MAX)
            .expect("reads")
            .expect("usable");
        for band_bytes in [1, 902, 2000, 902 * 7] {
            let banded = merged_image(&document, &options, band_bytes)
                .expect("reads")
                .expect("usable");
            assert_eq!(banded, whole, "band of {band_bytes} bytes");
        }
    }

    #[test]
    fn a_merged_image_with_a_wrong_size_is_not_usable() {
        let document = gray_document(8, 6);
        let mut file = document.file().clone();
        assert!(merged_is_usable(&file));
        file.merged.channels[0].bounds.right = 7;
        assert!(!merged_is_usable(&file), "narrower than the canvas");
        let mut file = document.file().clone();
        file.merged.channels[0].bounds.bottom = 7;
        assert!(!merged_is_usable(&file), "taller than the canvas");
        let mut file = document.file().clone();
        file.merged.channels.clear();
        assert!(!merged_is_usable(&file), "no channels");
        let mut file = document.file().clone();
        file.merged_error = Some("bad".to_owned());
        assert!(!merged_is_usable(&file), "a recorded merged error");
    }

    #[test]
    fn an_unusable_merged_image_reads_as_none() {
        let mut spec = DocumentSpec::new(4, 4, 8, 3);
        spec.layers.push(LayerSpec::opaque((0, 0, 4, 4), [1, 2, 3]));
        spec.merged_section = Some(Vec::new());
        let dir =
            std::env::temp_dir().join(format!("tarragon-psd-unusable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("folder");
        let path = dir.join("a.psd");
        std::fs::write(&path, build_psd(&spec)).expect("write");
        let options = ParseOptions::none();
        let document = PsdDocument::open_lenient(&path, &options).expect("opens");
        assert!(
            merged_image(&document, &options, u64::MAX)
                .expect("no error")
                .is_none()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An RGB document with a blue merged image and a red layer.
    fn blue_merged_red_layer() -> PsdDocument {
        let mut spec = DocumentSpec::new(8, 8, 8, 3);
        spec.layers
            .push(LayerSpec::opaque((0, 0, 8, 8), [200, 0, 0]));
        spec.merged = [0u16, 0, 200].iter().map(|v| vec![*v; 64]).collect();
        PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses")
    }

    #[test]
    fn a_timeout_or_a_cancel_in_the_merged_read_ends_the_render() {
        let document = blue_merged_red_layer();
        let expired = ParseOptions::with_deadline(Deadline::after(Duration::ZERO));
        let cancelled = ParseOptions::none().with_cancel(Arc::new(AtomicBool::new(true)));
        // The merged read itself reports the transient error.
        for (options, timeout) in [(&expired, true), (&cancelled, false)] {
            let read = merged_image(&document, options, u64::MAX);
            assert!(matches!(read, Err(ref error) if error.is_transient()));
            let rendered = render_document(&document, options, u64::MAX);
            match rendered {
                Err(PsdError::Timeout) if timeout => {}
                Err(PsdError::Cancelled) if !timeout => {}
                other => panic!("expected a transient error, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_merged_image_without_all_color_channels_is_not_usable() {
        let mut spec = DocumentSpec::new(8, 8, 8, 3);
        spec.merged_channels = Some(2);
        let document =
            PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
        assert!(!merged_is_usable(document.file()));
        let mut cmyk = DocumentSpec::new(8, 8, 8, 4);
        cmyk.merged_channels = Some(3);
        let document =
            PsdDocument::from_bytes(build_psd(&cmyk), &ParseOptions::none()).expect("parses");
        assert!(!merged_is_usable(document.file()));
    }
}
