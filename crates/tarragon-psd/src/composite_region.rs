//! The composited pixels of one viewport and their byte encoders.

use crate::decode::Viewport;

/// The composited pixels of one viewport.
///
/// Colors are straight (not premultiplied) RGB in `0.0..=1.0`. The alpha is the
/// composite coverage. A fully transparent pixel keeps the white backdrop, so
/// its color is white.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositeRegion {
    /// The viewport this region covers.
    viewport: Viewport,
    /// Straight RGB, three `f32` per pixel, row-major.
    color: Vec<f32>,
    /// Alpha, one `f32` per pixel, row-major.
    alpha: Vec<f32>,
    /// The number of layers skipped because their channels use an unsupported
    /// compression.
    skipped_layers: usize,
}

impl CompositeRegion {
    /// Build a region from its straight color and alpha planes.
    pub(crate) const fn new(
        viewport: Viewport,
        color: Vec<f32>,
        alpha: Vec<f32>,
        skipped_layers: usize,
    ) -> Self {
        Self {
            viewport,
            color,
            alpha,
            skipped_layers,
        }
    }

    /// The number of layers skipped because their channels use an unsupported
    /// compression.
    ///
    /// A nonzero count means the pixels leave those layers out, so the render
    /// may differ from the document.
    #[must_use]
    pub const fn skipped_layers(&self) -> usize {
        self.skipped_layers
    }

    /// The viewport this region covers.
    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// The region width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.viewport.width()
    }

    /// The region height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.viewport.height()
    }

    /// The straight RGB at a region pixel.
    ///
    /// Returns `None` when the pixel is outside the region.
    #[must_use]
    pub fn color(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        let index = self.pixel_index(x, y)?;
        Some([
            self.color[index * 3],
            self.color[index * 3 + 1],
            self.color[index * 3 + 2],
        ])
    }

    /// The alpha at a region pixel, or `None` when outside the region.
    #[must_use]
    pub fn alpha(&self, x: u32, y: u32) -> Option<f32> {
        let index = self.pixel_index(x, y)?;
        self.alpha.get(index).copied()
    }

    /// The straight RGBA at a region pixel, or `None` when outside the region.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[f32; 4]> {
        let index = self.pixel_index(x, y)?;
        Some([
            self.color[index * 3],
            self.color[index * 3 + 1],
            self.color[index * 3 + 2],
            self.alpha[index],
        ])
    }

    /// Encode the region as straight RGBA8, rounding to the nearest level.
    #[must_use]
    pub fn rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.alpha.len() * 4);
        for (index, alpha) in self.alpha.iter().enumerate() {
            out.push(to_u8(self.color[index * 3]));
            out.push(to_u8(self.color[index * 3 + 1]));
            out.push(to_u8(self.color[index * 3 + 2]));
            out.push(to_u8(*alpha));
        }
        out
    }

    /// Encode the region as straight RGBA16.
    #[must_use]
    pub fn rgba16(&self) -> Vec<u16> {
        let mut out = Vec::with_capacity(self.alpha.len() * 4);
        for (index, alpha) in self.alpha.iter().enumerate() {
            out.push(to_u16(self.color[index * 3]));
            out.push(to_u16(self.color[index * 3 + 1]));
            out.push(to_u16(self.color[index * 3 + 2]));
            out.push(to_u16(*alpha));
        }
        out
    }

    /// The row-major pixel index of a region pixel.
    #[must_use]
    fn pixel_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.viewport.width() || y >= self.viewport.height() {
            return None;
        }
        let x = usize::try_from(x).ok()?;
        let y = usize::try_from(y).ok()?;
        let stride = usize::try_from(self.viewport.width()).ok()?;
        y.checked_mul(stride)?.checked_add(x)
    }
}

/// Round a normalized value to the nearest `u8` level.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[must_use]
fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Round a normalized value to the nearest `u16` level.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[must_use]
fn to_u16(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16
}

#[cfg(test)]
mod tests {
    use super::{to_u8, to_u16};

    #[test]
    fn values_round_to_the_nearest_level() {
        assert_eq!(to_u8(0.0), 0);
        assert_eq!(to_u8(1.0), 255);
        assert_eq!(to_u8(0.5), 128);
        assert_eq!(to_u8(2.0 / 255.0), 2);
        assert_eq!(to_u16(1.0), 65535);
        assert_eq!(to_u16(0.5), 32768);
    }
}
