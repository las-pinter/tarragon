//! The paint count of a layer effect stack, which sizes the memory budget.

use crate::effects::{BevelStyle, Effect, Effects, StrokePosition};

impl Effects {
    /// The most paints the compositor holds at once for these effects.
    ///
    /// Each paint is a color plane and a coverage plane over the padded size.
    /// The exterior paints (drop shadow, outer glow, stroke, outer bevel) live
    /// together until the layer composites. The emboss paints are built after
    /// that, so the count is the larger of the two groups. Interior effects
    /// keep no paint. A disabled stack gives zero.
    #[must_use]
    pub fn held_paints(&self) -> usize {
        if !self.master {
            return 0;
        }
        let (mut exterior, mut emboss) = (0, 0);
        for effect in self.enabled() {
            match effect {
                Effect::DropShadow(_) | Effect::OuterGlow(_) => exterior += 1,
                Effect::Stroke(stroke) if stroke.position != StrokePosition::Inside => {
                    exterior += 1;
                }
                Effect::Bevel(bevel) => match bevel.style {
                    BevelStyle::Outer => exterior += 2,
                    BevelStyle::Emboss | BevelStyle::PillowEmboss => emboss += 2,
                    BevelStyle::Inner | BevelStyle::StrokeEmboss => {}
                },
                _ => {}
            }
        }
        exterior.max(emboss)
    }
}

#[cfg(test)]
mod tests {
    use crate::blend::BlendMode;
    use crate::effects::{
        Bevel, BevelStyle, Contour, Effect, EffectCommon, Effects, Overlay, Stroke, StrokePaint,
        StrokePosition,
    };

    fn common() -> EffectCommon {
        EffectCommon {
            enabled: true,
            blend: BlendMode::Normal,
            opacity: 1.0,
        }
    }

    fn bevel(style: BevelStyle) -> Effect {
        Effect::Bevel(Bevel {
            enabled: true,
            style,
            depth: 1.0,
            size: 4.0,
            soften: 0.0,
            angle: 120.0,
            altitude: 30.0,
            use_global_light: false,
            up: true,
            highlight: common(),
            highlight_color: [1.0; 3],
            shadow: common(),
            shadow_color: [0.0; 3],
            gloss: Contour::default(),
        })
    }

    fn stack(items: Vec<Effect>) -> Effects {
        Effects {
            master: true,
            items,
        }
    }

    #[test]
    fn each_outer_bevel_holds_two_paints() {
        let effects = stack(vec![
            bevel(BevelStyle::Outer),
            bevel(BevelStyle::Outer),
            bevel(BevelStyle::Outer),
        ]);
        assert_eq!(effects.items.len(), 3);
        assert_eq!(effects.held_paints(), 6);
    }

    #[test]
    fn interior_effects_hold_no_paint() {
        let overlay = Effect::ColorOverlay(Overlay {
            common: common(),
            color: [1.0; 3],
        });
        let effects = stack(vec![overlay, bevel(BevelStyle::Inner)]);
        assert_eq!(effects.held_paints(), 0);
    }

    #[test]
    fn an_inside_stroke_holds_no_paint_and_an_outside_stroke_holds_one() {
        let stroke = |position| {
            Effect::Stroke(Stroke {
                common: common(),
                size: 3.0,
                position,
                paint: StrokePaint::Color([0.0; 3]),
            })
        };
        assert_eq!(stack(vec![stroke(StrokePosition::Inside)]).held_paints(), 0);
        assert_eq!(
            stack(vec![stroke(StrokePosition::Outside)]).held_paints(),
            1
        );
    }

    #[test]
    fn emboss_paints_do_not_add_to_the_exterior_paints() {
        let effects = stack(vec![bevel(BevelStyle::Outer), bevel(BevelStyle::Emboss)]);
        assert_eq!(effects.held_paints(), 2);
        let embossed = stack(vec![
            bevel(BevelStyle::Emboss),
            bevel(BevelStyle::PillowEmboss),
        ]);
        assert_eq!(embossed.held_paints(), 4);
    }

    #[test]
    fn a_disabled_stack_holds_no_paint() {
        let mut effects = stack(vec![bevel(BevelStyle::Outer)]);
        effects.master = false;
        assert_eq!(effects.held_paints(), 0);
    }
}
