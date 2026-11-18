//! The `PtFl` pattern fill block: which pattern and how it is placed.

use crate::descriptor::parse_versioned;

/// A parsed `PtFl` pattern fill: which pattern and how it is placed.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternFill {
    /// The id of the pattern, without the trailing NUL.
    pub id: String,
    /// The name of the pattern, without the trailing NUL.
    pub name: String,
    /// The scale as a fraction, 1.0 for 100 percent.
    pub scale: f32,
    /// The angle in degrees, counter-clockwise.
    pub angle: f32,
    /// True when the pattern starts at the layer, false when it starts at the
    /// document origin.
    pub linked: bool,
    /// The phase in pixels, horizontal and vertical.
    pub phase: (f32, f32),
}

/// Parse a `PtFl` block: a version and a descriptor.
///
/// Returns `None` when the descriptor holds neither a pattern id nor a name.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the descriptor holds small percent, angle and pixel values"
)]
#[must_use]
pub fn parse_pattern_fill(data: &[u8]) -> Option<PatternFill> {
    let (descriptor, _) = parse_versioned(data)?;
    let pattern = descriptor.descriptor(b"Ptrn")?;
    let text = |key: &[u8]| {
        pattern
            .text(key)
            .map(|value| value.trim_end_matches('\0').to_owned())
            .unwrap_or_default()
    };
    let (id, name) = (text(b"Idnt"), text(b"Nm  "));
    if id.is_empty() && name.is_empty() {
        return None;
    }
    let finite = |value: f64, fallback: f64| if value.is_finite() { value } else { fallback };
    let scale = finite(descriptor.number(b"Scl ").unwrap_or(100.0), 100.0) / 100.0;
    let angle = finite(descriptor.number(b"Angl").unwrap_or(0.0), 0.0);
    // Writers differ on the key of the link flag. A missing flag means linked.
    let linked = descriptor
        .boolean(b"Algn")
        .or_else(|| descriptor.boolean(b"Lnkd"))
        .unwrap_or(true);
    let phase = descriptor.descriptor(b"phase").map_or((0.0, 0.0), |point| {
        (
            finite(point.number(b"Hrzn").unwrap_or(0.0), 0.0),
            finite(point.number(b"Vrtc").unwrap_or(0.0), 0.0),
        )
    });
    Some(PatternFill {
        id,
        name,
        scale: scale as f32,
        angle: angle as f32,
        linked,
        phase: (phase.0 as f32, phase.1 as f32),
    })
}
