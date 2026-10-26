//! Compose the layer pixels and the interior effects of one layer.
//!
//! Fill opacity scales only the pixels of the layer itself. The interior
//! effects keep their own strength inside the shape, so they stay visible at a
//! fill of zero. Layer opacity does not appear here: it applies once, when the
//! finished stack mixes back over the backdrop.

use crate::blend::{BlendMode, fill_is_special, union};
use crate::composite::Compositor;

/// The planes of one layer after its interior effects painted, over one
/// viewport.
pub(crate) struct LayerStage {
    /// The layer colour with the interior effects painted in.
    pub(crate) color: Vec<f32>,
    /// The layer colour before the effects, when fill opacity is below one.
    pub(crate) own: Option<Vec<f32>>,
    /// The layer alpha after fill opacity.
    pub(crate) layer_alpha: Vec<f32>,
    /// The layer alpha before fill opacity, without layer opacity.
    pub(crate) over: Vec<f32>,
    /// The alpha of the interior effects together.
    pub(crate) effect_alpha: Vec<f32>,
    /// The layer blend mode.
    pub(crate) blend: BlendMode,
    /// The fill opacity.
    pub(crate) fill: f32,
}

impl Compositor {
    /// Composite the layer and its interior effects, and return the alpha of
    /// the stack they add.
    ///
    /// At full fill the painted colour goes through the layer blend as one
    /// source. Below full fill, the layer pixels and the effects separate:
    ///
    /// - In a mode of the special eight, the layer pixels go through the fill
    ///   blend, then the effect colour goes through the plain blend with the
    ///   effect alpha, so the effects keep full strength.
    /// - In any other mode, [`fold_interior`] joins them into one source.
    pub(crate) fn apply_layer_stage(&mut self, stage: LayerStage) -> Vec<f32> {
        let LayerStage {
            mut color,
            own,
            mut layer_alpha,
            mut over,
            effect_alpha,
            blend,
            fill,
        } = stage;
        let Some(own) = own else {
            self.apply_source(&color, &layer_alpha, &layer_alpha, blend);
            return layer_alpha;
        };
        if fill_is_special(blend) {
            self.apply_source_fill(&own, &layer_alpha, &layer_alpha, &over, blend, fill);
            if effect_alpha.iter().any(|alpha| *alpha > 0.0) {
                let effect_color = effect_color(&color, &own, &effect_alpha);
                self.apply_source(&effect_color, &effect_alpha, &effect_alpha, blend);
            }
            for (alpha, effect) in layer_alpha.iter_mut().zip(&effect_alpha) {
                *alpha = union(*alpha, *effect);
            }
            return layer_alpha;
        }
        fold_interior(&mut color, &own, &mut layer_alpha, &mut over, &effect_alpha);
        self.apply_source(&color, &layer_alpha, &layer_alpha, blend);
        layer_alpha
    }
}

/// The colour of the effects alone.
///
/// The painted colour is the layer colour mixed toward the effect colour by the
/// effect alpha, so the effect colour is what remains. A pixel without an
/// effect gets zero.
fn effect_color(painted: &[f32], own: &[f32], effect_alpha: &[f32]) -> Vec<f32> {
    let mut effect = vec![0.0f32; painted.len()];
    for (index, alpha) in effect_alpha.iter().enumerate() {
        if *alpha <= 0.0 {
            continue;
        }
        for channel in 0..3 {
            let at = index * 3 + channel;
            effect[at] = ((painted[at] - (1.0 - alpha) * own[at]) / alpha).clamp(0.0, 1.0);
        }
    }
    effect
}

/// Fold the interior effect alpha into the fill-scaled layer.
///
/// The layer pixels have alpha `layer_alpha` (shape times fill) and colour
/// `own`. The effects, with alpha `effect_alpha`, sit over them. `color` holds
/// the painted colour, which is `own` mixed toward the effect colour by the
/// effect alpha. The result is the colour and alpha of the stacked layer. The
/// `over` plane becomes the alpha of that stack at full fill. With several
/// effects or a non-normal effect blend mode, the effect blends over the layer
/// colour, not over the real backdrop.
fn fold_interior(
    color: &mut [f32],
    own: &[f32],
    layer_alpha: &mut [f32],
    over: &mut [f32],
    effect_alpha: &[f32],
) {
    for index in 0..layer_alpha.len() {
        let effect = effect_alpha[index];
        if effect <= 0.0 {
            continue;
        }
        let layer = layer_alpha[index];
        let alpha = union(effect, layer);
        for channel in 0..3 {
            let at = index * 3 + channel;
            let effect_part = color[at] - (1.0 - effect) * own[at];
            let numerator = effect_part + (1.0 - effect) * layer * own[at];
            color[at] = (numerator / alpha).clamp(0.0, 1.0);
        }
        layer_alpha[index] = alpha;
        over[index] = union(effect, over[index]);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::{BlendMode, Compositor, LayerStage};

    /// One-pixel compositor over a half-transparent backdrop.
    fn half_backdrop(color: [f32; 3]) -> Compositor {
        Compositor::new(
            1,
            1,
            color.to_vec(),
            vec![0.5],
            false,
            &Rc::new(Cell::new(0)),
        )
    }

    #[test]
    fn the_fill_source_matches_the_written_out_sum_on_a_half_backdrop() {
        // Linear dodge at fill f, opacity o, backdrop alpha ab.
        let (ab, o, f) = (0.5f32, 0.6f32, 0.5f32);
        let cb = [0.8f32, 0.6, 0.4];
        let cs = [0.4f32, 0.7, 0.9];
        let mut compositor = half_backdrop(cb);
        let (over, alone) = ([o], [o * f]);
        compositor.apply_source_fill(&cs, &alone, &alone, &over, BlendMode::LinearDodge, f);
        let ao = ab + (1.0 - ab) * o * f;
        for channel in 0..3 {
            let blend = (cb[channel] + cs[channel] * f).min(1.0);
            let sum =
                ab * (1.0 - o) * cb[channel] + ab * o * blend + (1.0 - ab) * o * f * cs[channel];
            let want = sum / ao;
            let got = compositor.color[channel];
            assert!(
                (got - want).abs() < 1e-5,
                "channel {channel}: {got} vs {want}"
            );
        }
        assert!((compositor.alpha[0] - ao).abs() < 1e-5);
    }

    #[test]
    fn a_special_layer_at_fill_zero_still_blends_its_effect() {
        // Opaque backdrop, linear dodge, fill zero: the layer pixels add
        // nothing and a full-strength effect adds its colour.
        let mut compositor = Compositor::new(
            1,
            1,
            vec![0.5, 0.5, 0.5],
            vec![1.0],
            false,
            &Rc::new(Cell::new(0)),
        );
        let stage = LayerStage {
            color: vec![1.0, 0.0, 0.0],
            own: Some(vec![0.2, 0.2, 0.2]),
            layer_alpha: vec![0.0],
            over: vec![1.0],
            effect_alpha: vec![1.0],
            blend: BlendMode::LinearDodge,
            fill: 0.0,
        };
        let alpha = compositor.apply_layer_stage(stage);
        assert_eq!(alpha, vec![1.0]);
        let want = [1.0f32, 0.5, 0.5];
        for (channel, expected) in want.iter().enumerate() {
            let got = compositor.color[channel];
            assert!((got - expected).abs() < 1e-5, "channel {channel}: {got}");
        }
    }
}
