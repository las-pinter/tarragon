//! Compositor paths for fills, adjustments, and the merged-preview fallback.
//!
//! Each path writes into the shared compositor state of [`crate::composite`].

use crate::adjustments::Adjustment;
use crate::composite::Compositor;
use crate::composite_region::CompositeRegion;
use crate::content::GradientFill;
use crate::decode::Viewport;
use crate::document::PsdDocument;
use crate::error::PsdError;
use crate::layer_source::{channel_samples, cmyk_color, gray_color, read_layer, rgb_color};
use crate::layer_tree::LayerNode;
use crate::limits::ParseOptions;
use crate::model::ColorMode;

impl PsdDocument {
    /// Composite the merged preview when the file has no layer records.
    ///
    /// A flattened document falls back to the merged image. The merged data is
    /// a preview over a white background, so the RGB is
    /// un-composited with the alpha channel. The alpha exists only when the
    /// layer count was negative. Only an RGB or gray preview takes that
    /// straight-color recovery, so a flattened CMYK document with merged alpha
    /// keeps its stored values.
    pub(crate) fn composite_merged_background(
        &self,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<CompositeRegion, PsdError> {
        let region = self.read_merged_region(viewport, options)?;
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let count = width
            .checked_mul(height)
            .and_then(|area| area.checked_mul(3))
            .ok_or(PsdError::LengthTooLarge {
                length: u64::from(viewport.width()) * u64::from(viewport.height()),
            })?;
        let area = count / 3;
        let max = if region.depth >= 16 { 65535.0 } else { 255.0 };
        let color = match region.color_mode {
            ColorMode::Rgb => rgb_color(&region, area, max),
            ColorMode::Cmyk => cmyk_color(&region, area, max),
            ColorMode::Grayscale => gray_color(&region, area, max),
            mode => {
                return Err(PsdError::UnsupportedColorMode {
                    mode,
                    depth: region.depth,
                });
            }
        };
        let alpha_channel = channel_samples(&region, -1);
        let has_alpha = alpha_channel.is_some();
        let alpha = match alpha_channel {
            Some(samples) => {
                let mut alpha: Vec<f32> = samples
                    .iter()
                    .take(area)
                    .map(|sample| f32::from(*sample) / max)
                    .collect();
                // A channel with no data reads as transparent.
                alpha.resize(area, 0.0);
                alpha
            }
            None => vec![1.0f32; area],
        };
        let mut color = color;
        // The compositor recovers the straight color only when a merged alpha
        // channel exists and the preview sits on white.
        if matches!(region.color_mode, ColorMode::Rgb | ColorMode::Grayscale) && has_alpha {
            for index in 0..area {
                let a = alpha[index];
                if a > 0.0 {
                    for channel in 0..3 {
                        let straight = (color[index * 3 + channel] + a - 1.0) / a;
                        color[index * 3 + channel] = straight.clamp(0.0, 1.0);
                    }
                }
            }
        }
        Ok(CompositeRegion::new(viewport, color, alpha, 0))
    }
}

impl Compositor {
    /// Apply one adjustment layer to the running backdrop.
    ///
    /// The adjustment transforms the backdrop, then blends the result back by
    /// the layer opacity times its mask. Alpha is unchanged.
    pub(crate) fn apply_adjustment(
        &mut self,
        node: &LayerNode,
        adjustment: &Adjustment,
        document: &PsdDocument,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<(), PsdError> {
        let Some(index) = node.index else {
            return Ok(());
        };
        let mask = read_layer(document, index, viewport, options)?
            .mask
            .unwrap_or_else(|| vec![1.0f32; self.count]);
        let coverage = node.opacity * node.fill_opacity;
        for (index, &mask_value) in mask.iter().enumerate() {
            let k = coverage * mask_value;
            if k <= 0.0 {
                continue;
            }
            let backdrop = [
                self.color[index * 3],
                self.color[index * 3 + 1],
                self.color[index * 3 + 2],
            ];
            let mut adjusted = backdrop;
            adjustment.apply(&mut adjusted);
            let blended = crate::blend::blend_rgb(node.blend, backdrop, adjusted);
            for channel in 0..3 {
                self.color[index * 3 + channel] =
                    backdrop[channel] + (blended[channel] - backdrop[channel]) * k;
            }
        }
        Ok(())
    }

    /// Render a gradient fill over the viewport.
    ///
    /// The frame is the whole canvas, so tiles agree on the ramp. The returned
    /// color is straight RGB and the shape is the fill alpha.
    #[allow(
        clippy::cast_precision_loss,
        reason = "canvas and viewport coordinates are small enough for f32"
    )]
    pub(crate) fn render_gradient(
        fill: &GradientFill,
        viewport: Viewport,
        document: &PsdDocument,
        options: &ParseOptions,
    ) -> Result<(Vec<f32>, Vec<f32>), PsdError> {
        let width = usize::try_from(viewport.width()).unwrap_or(0);
        let height = usize::try_from(viewport.height()).unwrap_or(0);
        let mut color = vec![0.0f32; width * height * 3];
        let mut shape = vec![0.0f32; width * height];
        let header = document.file().header;
        let frame = (0.0, 0.0, header.width as f32, header.height as f32);
        for y in 0..height {
            options.check()?;
            for x in 0..width {
                let canvas_x = viewport.left as f32 + x as f32 + 0.5;
                let canvas_y = viewport.top as f32 + y as f32 + 0.5;
                let sample = fill.sample(fill.parameter_at(frame, canvas_x, canvas_y));
                let index = y * width + x;
                color[index * 3] = sample[0];
                color[index * 3 + 1] = sample[1];
                color[index * 3 + 2] = sample[2];
                shape[index] = sample[3];
            }
        }
        Ok((color, shape))
    }
}
