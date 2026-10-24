//! Per-layer source reads for the compositor.
//!
//! Each read decodes one layer over a viewport, then builds its color, its
//! coverage, and its optional mask. A layer without a transparency channel is
//! fully covered inside its bounds.

use crate::decode::{ImageRegion, Viewport};
use crate::document::PsdDocument;
use crate::error::PsdError;
use crate::layer::{LayerBounds, LayerRecord};
use crate::limits::ParseOptions;
use crate::model::ColorMode;

/// The decoded color, coverage, and mask of one layer over a viewport.
pub(crate) struct LayerRead {
    /// Straight RGB, three `f32` per pixel.
    pub(crate) color: Vec<f32>,
    /// Coverage in `0.0..=1.0`, one `f32` per pixel.
    pub(crate) shape: Vec<f32>,
    /// The layer mask in `0.0..=1.0`, when present and enabled.
    pub(crate) mask: Option<Vec<f32>>,
}

/// Read one layer's color, coverage, and mask over a viewport.
///
/// A layer without a transparency channel is fully covered inside its bounds.
/// A CMYK layer converts to RGB with `R = (1 - C) * (1 - K)`.
pub(crate) fn read_layer(
    document: &PsdDocument,
    index: usize,
    viewport: Viewport,
    options: &ParseOptions,
) -> Result<LayerRead, PsdError> {
    let layer = document
        .file()
        .layers
        .get(index)
        .ok_or(PsdError::LayerOutOfRange {
            index,
            count: document.file().layers.len(),
        })?;
    let region = document.read_layer_region(index, viewport, options)?;
    let width = usize::try_from(viewport.width()).unwrap_or(0);
    let height = usize::try_from(viewport.height()).unwrap_or(0);
    let count = width * height;
    let scale = if region.depth >= 16 { 65535.0 } else { 255.0 };
    let color = match region.color_mode {
        ColorMode::Rgb => rgb_color(&region, count, scale),
        ColorMode::Cmyk => cmyk_color(&region, count, scale),
        ColorMode::Grayscale => gray_color(&region, count, scale),
        mode => {
            return Err(PsdError::UnsupportedColorMode {
                mode,
                depth: region.depth,
            });
        }
    };
    let shape = shape_from(&region, layer.bounds, viewport, count, scale);
    let mask = mask_from(&region, layer, viewport, count, scale);
    Ok(LayerRead { color, shape, mask })
}

/// The samples of one channel, when the channel is present.
#[must_use]
pub(crate) fn channel_samples(region: &ImageRegion, id: i16) -> Option<&[u16]> {
    region
        .channels
        .iter()
        .find(|channel| channel.id == id)
        .map(|channel| channel.samples.as_slice())
}

/// Build straight RGB from the color channels.
#[must_use]
pub(crate) fn rgb_color(region: &ImageRegion, count: usize, scale: f32) -> Vec<f32> {
    let mut color = vec![0.0f32; count * 3];
    for (channel, id) in [0i16, 1, 2].into_iter().enumerate() {
        if let Some(samples) = channel_samples(region, id) {
            for (index, sample) in samples.iter().enumerate().take(count) {
                color[index * 3 + channel] = f32::from(*sample) / scale;
            }
        }
    }
    color
}

/// Build straight RGB from the gray channel: the three components are equal.
#[must_use]
pub(crate) fn gray_color(region: &ImageRegion, count: usize, scale: f32) -> Vec<f32> {
    let mut color = vec![0.0f32; count * 3];
    if let Some(samples) = channel_samples(region, 0) {
        for (index, sample) in samples.iter().enumerate().take(count) {
            let value = f32::from(*sample) / scale;
            color[index * 3..index * 3 + 3].fill(value);
        }
    }
    color
}

/// The sample at `index`, or zero for a channel with no data.
fn sample_at(samples: &[u16], index: usize) -> u16 {
    samples.get(index).copied().unwrap_or(0)
}

/// Build straight RGB from the CMYK channels.
///
/// PSD stores CMYK inverted: a stored sample of 255 means zero ink.
#[must_use]
pub(crate) fn cmyk_color(region: &ImageRegion, count: usize, scale: f32) -> Vec<f32> {
    let cyan = channel_samples(region, 0);
    let magenta = channel_samples(region, 1);
    let yellow = channel_samples(region, 2);
    let black = channel_samples(region, 3);
    let mut color = vec![0.0f32; count * 3];
    for index in 0..count {
        let c = cyan.map_or(0.0, |s| (scale - f32::from(sample_at(s, index))) / scale);
        let m = magenta.map_or(0.0, |s| (scale - f32::from(sample_at(s, index))) / scale);
        let y = yellow.map_or(0.0, |s| (scale - f32::from(sample_at(s, index))) / scale);
        let k = black.map_or(0.0, |s| (scale - f32::from(sample_at(s, index))) / scale);
        color[index * 3] = (1.0 - c) * (1.0 - k);
        color[index * 3 + 1] = (1.0 - m) * (1.0 - k);
        color[index * 3 + 2] = (1.0 - y) * (1.0 - k);
    }
    color
}

/// Build the coverage. A missing transparency channel means full coverage
/// inside the layer rectangle.
#[must_use]
fn shape_from(
    region: &ImageRegion,
    bounds: LayerBounds,
    viewport: Viewport,
    count: usize,
    scale: f32,
) -> Vec<f32> {
    let mut shape = vec![0.0f32; count];
    if let Some(samples) = channel_samples(region, -1) {
        for (index, sample) in samples.iter().enumerate().take(count) {
            shape[index] = f32::from(*sample) / scale;
        }
        return shape;
    }
    let width = usize::try_from(viewport.width()).unwrap_or(0);
    for y in 0..usize::try_from(viewport.height()).unwrap_or(0) {
        for x in 0..width {
            let canvas_x = viewport.left + u32::try_from(x).unwrap_or(0);
            let canvas_y = viewport.top + u32::try_from(y).unwrap_or(0);
            if inside(bounds, canvas_x, canvas_y) {
                shape[y * width + x] = 1.0;
            }
        }
    }
    shape
}

/// Build the layer mask. Outside the mask rectangle the default color fills.
fn mask_from(
    region: &ImageRegion,
    layer: &LayerRecord,
    viewport: Viewport,
    count: usize,
    scale: f32,
) -> Option<Vec<f32>> {
    let mask = layer.mask?;
    // Bit 1 of the mask flags disables the mask.
    if mask.flags & 0x02 != 0 {
        return None;
    }
    let background = f32::from(mask.default_color) / 255.0;
    let mut shape = vec![background; count];
    let Some(samples) = channel_samples(region, -2) else {
        return Some(shape);
    };
    let width = usize::try_from(viewport.width()).unwrap_or(0);
    for y in 0..usize::try_from(viewport.height()).unwrap_or(0) {
        for x in 0..width {
            let canvas_x = viewport.left + u32::try_from(x).unwrap_or(0);
            let canvas_y = viewport.top + u32::try_from(y).unwrap_or(0);
            if inside(mask.bounds, canvas_x, canvas_y) {
                let index = y * width + x;
                shape[index] = f32::from(sample_at(samples, index)) / scale;
            }
        }
    }
    Some(shape)
}

/// Build a constant RGB plane from a straight RGBA color.
#[must_use]
pub(crate) fn solid_color(rgba: [f32; 4], count: usize) -> Vec<f32> {
    let mut color = vec![0.0f32; count * 3];
    for index in 0..count {
        color[index * 3] = rgba[0];
        color[index * 3 + 1] = rgba[1];
        color[index * 3 + 2] = rgba[2];
    }
    color
}

/// True when a canvas pixel is inside a layer rectangle.
#[must_use]
fn inside(bounds: LayerBounds, x: u32, y: u32) -> bool {
    let x = i64::from(x);
    let y = i64::from(y);
    x >= i64::from(bounds.left)
        && x < i64::from(bounds.right)
        && y >= i64::from(bounds.top)
        && y < i64::from(bounds.bottom)
}

#[cfg(test)]
mod tests {
    use super::cmyk_color;
    use crate::decode::{ChannelRegion, ImageRegion, Viewport};
    use crate::model::ColorMode;

    /// Build a one-pixel CMYK region from the stored samples.
    fn one_pixel(stored: [u16; 4], depth: u16) -> ImageRegion {
        let channels = [0i16, 1, 2, 3]
            .into_iter()
            .zip(stored)
            .map(|(id, sample)| ChannelRegion {
                id,
                samples: vec![sample],
            })
            .collect();
        ImageRegion {
            viewport: Viewport::full(1, 1),
            depth,
            color_mode: ColorMode::Cmyk,
            channels,
        }
    }

    #[test]
    fn a_stored_255_is_zero_ink() {
        // A stored byte of 255 is zero ink, so the pixel is white.
        let region = one_pixel([255, 255, 255, 255], 8);
        assert_eq!(cmyk_color(&region, 1, 255.0), vec![1.0, 1.0, 1.0]);
    }

    #[test]
    fn a_stored_0_is_full_ink() {
        let region = one_pixel([0, 0, 0, 0], 8);
        assert_eq!(cmyk_color(&region, 1, 255.0), vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn the_black_channel_inverts_like_the_others() {
        // Zero ink in C, M, and Y, full ink in K: the pixel is black.
        let region = one_pixel([255, 255, 255, 0], 8);
        assert_eq!(cmyk_color(&region, 1, 255.0), vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn each_color_channel_inverts_on_its_own() {
        // Full ink in one color channel turns one light component off.
        let cases = [
            ([0, 255, 255, 255], [0.0, 1.0, 1.0]),
            ([255, 0, 255, 255], [1.0, 0.0, 1.0]),
            ([255, 255, 0, 255], [1.0, 1.0, 0.0]),
            ([255, 0, 0, 255], [1.0, 0.0, 0.0]),
        ];
        for (stored, want) in cases {
            let region = one_pixel(stored, 8);
            assert_eq!(cmyk_color(&region, 1, 255.0), want, "stored {stored:?}");
        }
    }

    #[test]
    fn the_16bit_scale_inverts_like_the_8bit_scale() {
        // The same ink at 16 bits: magenta only, so the pixel is magenta.
        let region = one_pixel([65535, 0, 65535, 65535], 16);
        assert_eq!(cmyk_color(&region, 1, 65535.0), vec![1.0, 0.0, 1.0]);
    }
}
