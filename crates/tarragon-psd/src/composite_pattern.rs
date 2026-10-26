//! Pattern fill: tile placement, sampling and the fill layer path.
//!
//! A pattern tiles the plane from an anchor point, scaled and rotated about it.
//! A linked pattern starts at the anchor (the layer corner for a fill layer).
//! An unlinked pattern starts at the document origin. The phase moves the
//! start. A sample is bilinear with wrap-around on premultiplied color, with
//! pixel centers at half-pixel positions. At 100 percent and no rotation on an
//! integer start, every sample lands on a pixel center and the tile repeats
//! exactly.

use crate::composite::Compositor;
use crate::decode::Viewport;
use crate::document::PsdDocument;
use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::pattern_fill::PatternFill;
use crate::patterns::{PatternTile, find_pattern};

/// Where a pattern sits: a document point maps to tile coordinates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    /// The tile origin in document pixels.
    origin: (f64, f64),
    /// The cosine and sine of the angle.
    cos_sin: (f64, f64),
    /// The inverse of the scale.
    inverse_scale: f64,
}

impl Placement {
    /// A placement from an anchor point.
    ///
    /// `anchor` is the start of a linked pattern. An unlinked pattern starts at
    /// the document origin. `phase` is in pixels, `scale` is a fraction and
    /// `angle` is in degrees, counter-clockwise on screen.
    pub(crate) fn anchored(
        anchor: (f64, f64),
        linked: bool,
        phase: (f32, f32),
        scale: f32,
        angle: f32,
    ) -> Self {
        let base = if linked { anchor } else { (0.0, 0.0) };
        let origin = (base.0 + f64::from(phase.0), base.1 + f64::from(phase.1));
        let scale = f64::from(scale);
        let inverse_scale = if scale.is_finite() && scale > 1e-3 {
            1.0 / scale
        } else {
            1.0
        };
        let (sin, cos) = f64::from(angle).to_radians().sin_cos();
        Self {
            origin,
            cos_sin: (cos, sin),
            inverse_scale,
        }
    }

    /// The tile coordinates of the document point `(x, y)`.
    pub(crate) fn map(&self, x: f64, y: f64) -> (f64, f64) {
        let (dx, dy) = (x - self.origin.0, y - self.origin.1);
        // Undo a counter-clockwise rotation. The y axis points down.
        let (cos, sin) = self.cos_sin;
        (
            (dx * cos - dy * sin) * self.inverse_scale,
            (dx * sin + dy * cos) * self.inverse_scale,
        )
    }
}

impl PatternTile {
    /// The premultiplied pixel at a wrapped integer position.
    fn wrapped(&self, x: i64, y: i64) -> [f32; 4] {
        let width = i64::from(self.width);
        let height = i64::from(self.height);
        // The wrapped values are below the tile edges, so they fit a usize.
        let column = usize::try_from(x.rem_euclid(width)).unwrap_or(0);
        let row = usize::try_from(y.rem_euclid(height)).unwrap_or(0);
        let pixel = self.pixels[row * self.width as usize + column];
        pixel.map(|value| f32::from(value) / 65_535.0)
    }

    /// Sample at tile coordinates `(u, v)`, where pixel centers are at `i + 0.5`.
    ///
    /// The result is bilinear with wrap-around. It is premultiplied RGBA.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "tile coordinates are far below the i64 range"
    )]
    pub(crate) fn sample(&self, u: f64, v: f64) -> [f32; 4] {
        let (u, v) = (u - 0.5, v - 0.5);
        let (floor_u, floor_v) = (u.floor(), v.floor());
        let (fraction_x, fraction_y) = ((u - floor_u) as f32, (v - floor_v) as f32);
        let (x0, y0) = (floor_u as i64, floor_v as i64);
        let mut sum = [0.0f32; 4];
        for (step_y, weight_y) in [(0, 1.0 - fraction_y), (1, fraction_y)] {
            if weight_y == 0.0 {
                continue;
            }
            for (step_x, weight_x) in [(0, 1.0 - fraction_x), (1, fraction_x)] {
                if weight_x == 0.0 {
                    continue;
                }
                let pixel = self.wrapped(x0 + step_x, y0 + step_y);
                let weight = weight_x * weight_y;
                for (slot, value) in sum.iter_mut().zip(pixel) {
                    *slot += value * weight;
                }
            }
        }
        sum
    }
}

impl Compositor {
    /// Render a pattern fill over the viewport.
    ///
    /// `layer` is the layer record index. A linked fill starts at the top-left
    /// of the layer bounds. The returned color is straight RGB and the shape is
    /// the fill alpha. A missing pattern gives an empty fill.
    #[allow(
        clippy::cast_precision_loss,
        reason = "canvas and viewport coordinates are small enough for f64"
    )]
    pub(crate) fn render_pattern(
        fill: &PatternFill,
        layer: Option<usize>,
        viewport: Viewport,
        document: &PsdDocument,
        options: &ParseOptions,
    ) -> Result<(Vec<f32>, Vec<f32>), PsdError> {
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let mut color = vec![0.0f32; width * height * 3];
        let mut shape = vec![0.0f32; width * height];
        let psd = document.file();
        let Some(tile) = find_pattern(&psd.patterns, &fill.id, &fill.name)
            .and_then(crate::patterns::PatternEntry::tile)
        else {
            return Ok((color, shape));
        };
        let anchor = layer
            .and_then(|index| psd.layers.get(index))
            .map_or((0.0, 0.0), |record| {
                (f64::from(record.bounds.left), f64::from(record.bounds.top))
            });
        let placement =
            Placement::anchored(anchor, fill.linked, fill.phase, fill.scale, fill.angle);
        for y in 0..height {
            options.check()?;
            for x in 0..width {
                let canvas_x = f64::from(viewport.left) + x as f64 + 0.5;
                let canvas_y = f64::from(viewport.top) + y as f64 + 0.5;
                let (u, v) = placement.map(canvas_x, canvas_y);
                let sample = tile.sample(u, v);
                let index = y * width + x;
                let alpha = sample[3];
                if alpha > 0.0 {
                    color[index * 3] = sample[0] / alpha;
                    color[index * 3 + 1] = sample[1] / alpha;
                    color[index * 3 + 2] = sample[2] / alpha;
                }
                shape[index] = alpha.min(1.0);
            }
        }
        Ok((color, shape))
    }
}
