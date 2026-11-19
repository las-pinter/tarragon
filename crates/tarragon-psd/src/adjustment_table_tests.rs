//! The table form of a tone adjustment matches the direct form.

use super::Prepared;
use crate::adjustments::{Adjustment, Curve, CurvePoint, LevelsChannel};

/// Half of one 8-bit step: a table read must round to the same 8-bit value.
const TOLERANCE: f32 = 0.5 / 255.0;

/// Levels with the given gamma on every channel.
fn levels(gamma: f32) -> Adjustment {
    let channel = LevelsChannel {
        input_black: 0.1,
        input_white: 0.9,
        gamma,
        output_black: 0.05,
        output_white: 0.95,
    };
    Adjustment::Levels {
        master: LevelsChannel::identity(),
        per_channel: [channel; 3],
    }
}

/// The table cases: the steepest Levels gammas and every table variant.
fn table_cases() -> Vec<Adjustment> {
    let lifted = Curve::new(vec![
        CurvePoint {
            input: 0.0,
            output: 0.0,
        },
        CurvePoint {
            input: 0.5,
            output: 0.7,
        },
        CurvePoint {
            input: 1.0,
            output: 1.0,
        },
    ]);
    vec![
        levels(0.01),
        levels(1.0),
        levels(9.99),
        Adjustment::Curves {
            master: Curve::identity(),
            per_channel: [lifted.clone(), Curve::identity(), lifted],
        },
        Adjustment::BrightnessContrast {
            brightness: 80.0,
            contrast: 60.0,
            legacy: false,
        },
        Adjustment::BrightnessContrast {
            brightness: -40.0,
            contrast: -30.0,
            legacy: true,
        },
        Adjustment::Exposure {
            exposure: 1.5,
            offset: 0.02,
            gamma: 0.8,
        },
    ]
}

/// The inputs: an even grid plus values between the table samples.
#[expect(clippy::cast_precision_loss, reason = "the grid index is below 1000")]
fn inputs() -> impl Iterator<Item = [f32; 3]> {
    (0..=1000).map(|step| {
        let value = step as f32 / 1000.0;
        [value, (value * 0.37 + 0.31) % 1.0, 1.0 - value]
    })
}

#[test]
fn every_table_adjustment_matches_the_direct_form_within_half_a_step() {
    for adjustment in table_cases() {
        let prepared = Prepared::new(&adjustment);
        assert!(matches!(prepared, Prepared::Tables(..)), "{adjustment:?}");
        for input in inputs() {
            let mut direct = input;
            adjustment.apply(&mut direct);
            let mut table = input;
            prepared.apply(&mut table);
            for (got, want) in table.iter().zip(direct) {
                assert!(
                    (got - want).abs() <= TOLERANCE,
                    "{adjustment:?} at {input:?}: {got} vs {want}"
                );
            }
        }
    }
}

#[test]
fn a_step_or_mixing_adjustment_is_applied_directly() {
    for adjustment in [
        Adjustment::Posterize {
            levels: 4,
            lut_size: 256,
        },
        Adjustment::Threshold { level: 0.5 },
        Adjustment::Invert,
    ] {
        let prepared = Prepared::new(&adjustment);
        assert!(matches!(prepared, Prepared::Direct(_)), "{adjustment:?}");
        for input in inputs() {
            let mut direct = input;
            adjustment.apply(&mut direct);
            let mut through = input;
            prepared.apply(&mut through);
            assert_eq!(through, direct);
        }
    }
}
