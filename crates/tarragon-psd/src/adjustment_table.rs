//! Per-channel lookup tables for the tone adjustments.
//!
//! Levels, Curves, Brightness/Contrast and Exposure map each channel from its
//! own value alone. Their curves call `powf` several times per channel, so a
//! layer samples the curve once into a table and each pixel reads the table.

use crate::adjustments::Adjustment;

/// The samples per channel table.
const TABLE_SIZE: usize = 4096;
/// The widest gap between two neighbouring samples that a linear read may
/// span: half of one 8-bit step. On a steeper part, such as the kink of a
/// Levels white point at a low gamma, the pixel is computed exactly.
const MAX_GAP: f32 = 0.5 / 255.0;

/// An adjustment ready for a pixel loop.
pub(crate) enum Prepared<'a> {
    /// One table per RGB channel, and the adjustment for the steep parts.
    Tables([Box<[f32]>; 3], &'a Adjustment),
    /// An adjustment that mixes channels or steps, applied as it is.
    Direct(&'a Adjustment),
}

impl<'a> Prepared<'a> {
    /// Prepare `adjustment` for many pixels.
    pub(crate) fn new(adjustment: &'a Adjustment) -> Self {
        match adjustment {
            Adjustment::Levels { .. }
            | Adjustment::Curves { .. }
            | Adjustment::BrightnessContrast { .. }
            | Adjustment::Exposure { .. } => Self::Tables(tables(adjustment), adjustment),
            _ => Self::Direct(adjustment),
        }
    }

    /// Apply the adjustment to one RGB pixel.
    pub(crate) fn apply(&self, rgb: &mut [f32; 3]) {
        match self {
            Self::Tables(tables, adjustment) => {
                for (channel, table) in tables.iter().enumerate() {
                    rgb[channel] = read(table, rgb[channel]).unwrap_or_else(|| {
                        let mut exact = [rgb[channel]; 3];
                        adjustment.apply(&mut exact);
                        exact[channel]
                    });
                }
            }
            Self::Direct(adjustment) => adjustment.apply(rgb),
        }
    }
}

/// Sample each channel of a per-channel `adjustment` over `0.0..=1.0`.
#[expect(clippy::cast_precision_loss, reason = "the table index is below 4096")]
fn tables(adjustment: &Adjustment) -> [Box<[f32]>; 3] {
    let mut tables: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::with_capacity(TABLE_SIZE));
    for index in 0..TABLE_SIZE {
        let value = index as f32 / (TABLE_SIZE - 1) as f32;
        let mut rgb = [value; 3];
        adjustment.apply(&mut rgb);
        for (table, out) in tables.iter_mut().zip(rgb) {
            table.push(out);
        }
    }
    tables.map(Vec::into_boxed_slice)
}

/// Read `table` at `value`, clamped to `0.0..=1.0`, between the two nearest
/// samples, or `None` when they are more than [`MAX_GAP`] apart.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the scaled value is in 0..4096, so the floor fits the index"
)]
fn read(table: &[f32], value: f32) -> Option<f32> {
    let scaled = value.clamp(0.0, 1.0) * (TABLE_SIZE - 1) as f32;
    let index = (scaled.floor() as usize).min(TABLE_SIZE - 2);
    let fraction = scaled - index as f32;
    let (low, high) = (table[index], table[index + 1]);
    ((high - low).abs() <= MAX_GAP).then_some(low + (high - low) * fraction)
}

#[cfg(test)]
#[path = "adjustment_table_tests.rs"]
mod tests;
