//! The values as the user is editing them, and the parse of the typed text.

use std::str::FromStr;

use super::PrefsValues;

/// The index of each number in `Draft::numbers`.
pub(super) const WORKERS: usize = 0;
pub(super) const MULTI: usize = 1;
pub(super) const CANVAS: usize = 2;
pub(super) const PALETTE: usize = 3;
pub(super) const SHARE: usize = 4;
pub(super) const NEUTRAL: usize = 5;
pub(super) const CAP: usize = 6;

/// The values as the user is editing them. A number keeps its typed text, so
/// a half-typed value is not lost. The range of a number is not checked here:
/// the settings registry clamps it when the value is saved.
#[derive(Debug)]
pub(super) struct Draft {
    pub(super) numbers: [String; 7],
    /// The values that are not typed numbers.
    pub(super) rest: PrefsValues,
}

fn number<T: FromStr>(text: &str) -> Option<T> {
    text.trim().parse().ok()
}

/// A float that is a real number: `inf` and `NaN` parse, but are no setting.
fn float(text: &str) -> Option<f32> {
    number::<f32>(text).filter(|v| v.is_finite())
}

impl Draft {
    pub(super) fn new(v: PrefsValues) -> Self {
        let numbers = [
            v.max_psd_workers.to_string(),
            v.max_multi_preview.to_string(),
            v.large_canvas_mp.to_string(),
            v.palette_size.to_string(),
            v.min_share.to_string(),
            v.neutral_s.to_string(),
            v.cache_cap_gb.to_string(),
        ];
        Self { numbers, rest: v }
    }

    /// Set the cache folder from the picker.
    pub(super) fn set_cache_dir(&mut self, dir: String) {
        self.rest.cache_dir = Some(dir);
    }

    /// The values, or `None` while a number does not parse.
    pub(super) fn parse(&self) -> Option<PrefsValues> {
        let n = &self.numbers;
        Some(PrefsValues {
            max_psd_workers: number(&n[WORKERS])?,
            max_multi_preview: number(&n[MULTI])?,
            large_canvas_mp: float(&n[CANVAS])?,
            palette_size: number(&n[PALETTE])?,
            min_share: float(&n[SHARE])?,
            neutral_s: float(&n[NEUTRAL])?,
            cache_cap_gb: number(&n[CAP])?,
            ..self.rest.clone()
        })
    }

    /// Whether the text of number `index` parses.
    pub(super) fn valid(&self, index: usize) -> bool {
        let text = &self.numbers[index];
        match index {
            CANVAS | SHARE | NEUTRAL => float(text).is_some(),
            _ => number::<u32>(text).is_some(),
        }
    }
}
