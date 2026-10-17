//! Layer content: pixel layers, fill layers, and adjustment layers.
//!
//! A layer record carries extra tagged blocks. A fill layer has `SoCo` (solid
//! color) or `GdFl` (gradient). An adjustment layer has one adjustment block
//! such as `levl` or `curv`. The parser turns those blocks into a
//! [`LayerContent`]; the compositor draws the fill or applies the adjustment.
//!
//! The binary layouts follow the Adobe PSD specification.

use crate::adjustments::Adjustment;
use crate::descriptor::{Descriptor, Value, parse_versioned};

/// The gradient style of a `GdFl` fill layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientStyle {
    /// A straight linear ramp.
    Linear,
    /// A radial ramp from the center.
    Radial,
    /// An angular sweep around the center.
    Angle,
    /// A reflected ramp out from the center.
    Reflected,
    /// A diamond-shaped ramp.
    Diamond,
}

/// The color stops, midpoints and opacity stops of a parsed gradient.
type GradientStops = (Vec<(f32, [f32; 4])>, Vec<f32>, Vec<(f32, f32)>);

/// The gradient of a `GdFl` fill layer.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientFill {
    /// The color stops, sorted by location, with straight RGBA in `0.0..=1.0`.
    pub stops: Vec<(f32, [f32; 4])>,
    /// The midpoint of each segment, one per adjacent stop pair.
    pub midpoints: Vec<f32>,
    /// The opacity stops, sorted by location, in `0.0..=1.0`.
    pub opacity_stops: Vec<(f32, f32)>,
    /// The angle in degrees.
    pub angle: f32,
    /// The scale as a fraction of the frame.
    pub scale: f32,
    /// The gradient style.
    pub style: GradientStyle,
    /// True when the ramp is reversed.
    pub reverse: bool,
    /// The center offset as a fraction of the frame, horizontal and vertical.
    pub offset: (f32, f32),
    /// True when the fill is dithered.
    pub dither: bool,
    /// True when the gradient is aligned with the layer.
    pub align: bool,
}

/// The parsed content of one layer.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LayerContent {
    /// A pixel layer. The color comes from the layer channels.
    #[default]
    Pixels,
    /// A solid-color fill layer.
    SolidColor([f32; 4]),
    /// A gradient fill layer.
    Gradient(GradientFill),
    /// An adjustment layer.
    Adjustment(Adjustment),
    /// A recognized block the compositor does not apply yet.
    Unsupported,
}

impl GradientFill {
    /// Sample the ramp at `t` in `0.0..=1.0`, with the segment midpoints applied.
    #[must_use]
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let t = t.clamp(0.0, 1.0);
        let mut color = sample_stops(&self.stops, &self.midpoints, t);
        if !self.opacity_stops.is_empty() {
            color[3] *= sample_opacity(&self.opacity_stops, t);
        }
        color
    }

    /// The ramp parameter for one canvas point.
    ///
    /// `frame` is the gradient frame: left, top, width, height. A transparent
    /// fill uses the whole canvas frame, so tiles agree on the ramp.
    #[must_use]
    pub fn parameter_at(&self, frame: (f32, f32, f32, f32), x: f32, y: f32) -> f32 {
        let (x0, y0, width, height) = frame;
        let width = width.max(1.0);
        let height = height.max(1.0);
        let center_x = x0 + width / 2.0 + self.offset.0 * width;
        let center_y = y0 + height / 2.0 + self.offset.1 * height;
        let (dx, dy) = (x - center_x, y - center_y);
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let along = dx * cos - dy * sin;
        let across = dx * sin + dy * cos;
        let (chord, length) = gradient_units(self.angle, width, height);
        let scale = self.scale.max(1e-3);
        let chord = chord * scale;
        let length = length * scale;
        // Linear and Reflected sample a pixel corner, half a step before its center.
        let corner = 0.5 * (cos - sin);
        let mut t = match self.style {
            GradientStyle::Linear => (along - corner) / chord + 0.5,
            GradientStyle::Reflected => ((along - corner) / (chord / 2.0)).abs(),
            GradientStyle::Radial => (dx * dx + dy * dy).sqrt() / (length / 2.0),
            GradientStyle::Diamond => (along.abs() + across.abs()) / (length / 2.0),
            GradientStyle::Angle => ((self.angle.to_radians() - (-dy).atan2(dx))
                / std::f32::consts::TAU)
                .rem_euclid(1.0),
        };
        t = t.clamp(0.0, 1.0);
        if self.reverse { 1.0 - t } else { t }
    }
}

/// The gradient chord and ellipse-norm length for a frame at `angle`.
#[must_use]
fn gradient_units(angle: f32, width: f32, height: f32) -> (f32, f32) {
    let (sin, cos) = angle.to_radians().sin_cos();
    let length = ((cos * width).powi(2) + (sin * height).powi(2))
        .sqrt()
        .max(1.0);
    let chord = (width / cos.abs().max(1e-6))
        .min(height / sin.abs().max(1e-6))
        .max(1.0);
    (chord, length)
}

/// Sample color stops with their segment midpoints.
#[allow(
    clippy::many_single_char_names,
    reason = "t is the standard gradient parameter"
)]
#[must_use]
fn sample_stops(stops: &[(f32, [f32; 4])], midpoints: &[f32], t: f32) -> [f32; 4] {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return [0.0, 0.0, 0.0, 1.0];
    };
    if t <= first.0 {
        return first.1;
    }
    for (index, pair) in stops.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.0 {
            let u = if b.0 > a.0 {
                (t - a.0) / (b.0 - a.0)
            } else {
                0.0
            };
            let k = midpoint_remap(u, midpoints.get(index).copied().unwrap_or(0.5));
            return std::array::from_fn(|c| a.1[c] + (b.1[c] - a.1[c]) * k);
        }
    }
    last.1
}

/// Sample opacity stops linearly.
#[must_use]
fn sample_opacity(stops: &[(f32, f32)], t: f32) -> f32 {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return 1.0;
    };
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        if t <= pair[1].0 {
            let u = if pair[1].0 > pair[0].0 {
                (t - pair[0].0) / (pair[1].0 - pair[0].0)
            } else {
                0.0
            };
            return pair[0].1 + (pair[1].1 - pair[0].1) * u;
        }
    }
    last.1
}

/// Move a segment position `u` so that the color is halfway at `midpoint`.
#[must_use]
fn midpoint_remap(u: f32, midpoint: f32) -> f32 {
    let midpoint = midpoint.clamp(0.05, 0.95);
    if (midpoint - 0.5).abs() < 1e-6 {
        u
    } else if u <= midpoint {
        0.5 * u / midpoint
    } else {
        0.5 + 0.5 * (u - midpoint) / (1.0 - midpoint)
    }
}

/// Parse a solid-color `SoCo` block.
#[must_use]
pub fn parse_solid_color(data: &[u8]) -> Option<[f32; 4]> {
    let (descriptor, _) = parse_versioned(data)?;
    let color = descriptor
        .descriptor(b"Clr ")
        .and_then(color_from_descriptor)
        .or_else(|| color_from_descriptor(&descriptor))?;
    Some([color[0], color[1], color[2], 1.0])
}

/// Parse a gradient `GdFl` block.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the descriptor holds small percentage values"
)]
#[must_use]
pub fn parse_gradient_fill(data: &[u8]) -> Option<GradientFill> {
    let (descriptor, _) = parse_versioned(data)?;
    let gradient = descriptor.descriptor(b"Grad");
    let (stops, midpoints, opacity_stops) = gradient.map(parse_gradient_stops).unwrap_or_default();
    let stops = if stops.is_empty() {
        vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])]
    } else {
        stops
    };
    let offset = descriptor.descriptor(b"Ofst").map_or((0.0, 0.0), |o| {
        (
            o.number(b"Hrzn").unwrap_or(0.0) as f32 / 100.0,
            o.number(b"Vrtc").unwrap_or(0.0) as f32 / 100.0,
        )
    });
    Some(GradientFill {
        stops,
        midpoints,
        opacity_stops,
        angle: descriptor.number(b"Angl").unwrap_or(90.0) as f32,
        scale: descriptor.number(b"Scl ").map_or(1.0, |v| v as f32 / 100.0),
        style: match descriptor.enumeration(b"Type") {
            Some(b"Rdl ") => GradientStyle::Radial,
            Some(b"Angl") => GradientStyle::Angle,
            Some(b"Rflc") => GradientStyle::Reflected,
            Some(b"Dmnd") => GradientStyle::Diamond,
            _ => GradientStyle::Linear,
        },
        reverse: descriptor.boolean(b"Rvrs").unwrap_or(false),
        offset,
        dither: descriptor.boolean(b"Dthr").unwrap_or(false),
        align: !matches!(descriptor.get(b"Algn"), Some(Value::Boolean(false))),
    })
}

/// Parse the `Clrs`, midpoints and `Trns` of a `Grad` descriptor.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the descriptor holds small percentage and location values"
)]
fn parse_gradient_stops(gradient: &Descriptor) -> GradientStops {
    let mut stops: Vec<(f32, [f32; 4])> = Vec::new();
    let mut midpoints: Vec<f32> = Vec::new();
    if let Some(items) = gradient.list(b"Clrs") {
        for item in items {
            let Value::Descriptor(stop) = item else {
                continue;
            };
            let location = stop.number(b"Lctn").unwrap_or(0.0) as f32 / 4096.0;
            let Some(color) = stop.descriptor(b"Clr ").and_then(color_from_descriptor) else {
                continue;
            };
            let alpha = stop
                .descriptor(b"Clr ")
                .and_then(|c| c.number(b"alpha"))
                .map_or(1.0, |v| (v / 255.0) as f32);
            let midpoint = stop.number(b"Mdpn").unwrap_or(50.0) as f32 / 100.0;
            stops.push((location, [color[0], color[1], color[2], alpha]));
            midpoints.push(midpoint);
        }
    }
    let mut order: Vec<usize> = (0..stops.len()).collect();
    order.sort_by(|a, b| stops[*a].0.total_cmp(&stops[*b].0));
    let sorted: Vec<(f32, [f32; 4])> = order.iter().map(|i| stops[*i]).collect();
    // Each midpoint is stored on the stop that ends its segment.
    let mids: Vec<f32> = order
        .iter()
        .skip(1)
        .map(|i| midpoints.get(*i).copied().unwrap_or(0.5))
        .collect();
    let mut opacity: Vec<(f32, f32)> = Vec::new();
    if let Some(items) = gradient.list(b"Trns") {
        for item in items {
            let Value::Descriptor(stop) = item else {
                continue;
            };
            let location = stop.number(b"Lctn").unwrap_or(0.0) as f32 / 4096.0;
            let alpha = stop.number(b"Opct").map_or(1.0, |v| (v / 100.0) as f32);
            opacity.push((location, alpha));
        }
    }
    opacity.sort_by(|a, b| a.0.total_cmp(&b.0));
    (sorted, mids, opacity)
}

/// Convert a color descriptor to straight RGB.
///
/// `RGBC`, `CMYC` and `Grsc` classes are supported. The Lab class is out of
/// scope and returns `None`, so the fill is marked unsupported.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the descriptor holds 0..=255 and 0..=100 channel values"
)]
#[must_use]
fn color_from_descriptor(descriptor: &Descriptor) -> Option<[f32; 3]> {
    match descriptor.class_id.as_slice() {
        // Newer Photoshop writes 0..1 floats in an `RGBC` descriptor.
        b"RGBC" if descriptor.get(b"redFloat").is_some() => {
            let channel = |key: &[u8]| descriptor.number(key).map(|v| v as f32);
            Some([
                channel(b"redFloat")?,
                channel(b"greenFloat")?,
                channel(b"blueFloat")?,
            ])
        }
        b"RGBC" => {
            let channel = |key: &[u8]| descriptor.number(key).map(|v| (v / 255.0) as f32);
            Some([channel(b"Rd  ")?, channel(b"Grn ")?, channel(b"Bl  ")?])
        }
        b"CMYC" => {
            let channel = |key: &[u8]| descriptor.number(key).map(|v| (v / 100.0) as f32);
            let c = channel(b"Cyn ")?;
            let m = channel(b"Mgnt")?;
            let y = channel(b"Ylw ")?;
            let k = channel(b"Blck")?;
            Some([
                (1.0 - c) * (1.0 - k),
                (1.0 - m) * (1.0 - k),
                (1.0 - y) * (1.0 - k),
            ])
        }
        b"Grsc" => {
            let ink = descriptor.number(b"Gry ")? as f32 / 100.0;
            let gray = 1.0 - ink;
            Some([gray, gray, gray])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GradientFill, GradientStyle, color_from_descriptor, parse_gradient_stops, sample_stops,
    };
    use crate::descriptor::{Descriptor, Value};

    fn color_descriptor(r: f64, g: f64, b: f64) -> Descriptor {
        Descriptor {
            class_id: b"RGBC".to_vec(),
            items: vec![
                (b"Rd  ".to_vec(), Value::Double(r)),
                (b"Grn ".to_vec(), Value::Double(g)),
                (b"Bl  ".to_vec(), Value::Double(b)),
            ],
        }
    }

    /// A `Clrt` stop with a location, midpoint and grey colour.
    fn gradient_stop(location: i32, midpoint: i32, grey: f64) -> Value {
        Value::Descriptor(Descriptor {
            class_id: b"Clrt".to_vec(),
            items: vec![
                (b"Lctn".to_vec(), Value::Integer(location)),
                (b"Mdpn".to_vec(), Value::Integer(midpoint)),
                (
                    b"Clr ".to_vec(),
                    Value::Descriptor(color_descriptor(grey, grey, grey)),
                ),
            ],
        })
    }

    #[test]
    fn an_rgbc_color_descriptor_converts_to_unit_rgb() {
        let color = color_from_descriptor(&color_descriptor(255.0, 128.0, 0.0)).expect("rgb");
        assert!((color[0] - 1.0).abs() < 1e-6);
        assert!((color[1] - 128.0 / 255.0).abs() < 1e-6);
        assert!((color[2]).abs() < 1e-6);
    }

    #[test]
    fn a_float_rgbc_color_descriptor_converts_to_unit_rgb() {
        let descriptor = Descriptor {
            class_id: b"RGBC".to_vec(),
            items: vec![
                (b"redFloat".to_vec(), Value::Double(1.0)),
                (b"greenFloat".to_vec(), Value::Double(0.5)),
                (b"blueFloat".to_vec(), Value::Double(0.25)),
            ],
        };
        let color = color_from_descriptor(&descriptor).expect("rgb");
        assert!((color[0] - 1.0).abs() < 1e-6);
        assert!((color[1] - 0.5).abs() < 1e-6);
        assert!((color[2] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn a_lab_color_descriptor_is_unsupported() {
        let descriptor = Descriptor {
            class_id: b"LbCl".to_vec(),
            items: vec![(b"Lmnc".to_vec(), Value::Double(50.0))],
        };
        assert!(color_from_descriptor(&descriptor).is_none());
    }

    #[test]
    fn a_gray_color_descriptor_inverts_the_ink() {
        let descriptor = Descriptor {
            class_id: b"Grsc".to_vec(),
            items: vec![(b"Gry ".to_vec(), Value::Double(25.0))],
        };
        let color = color_from_descriptor(&descriptor).expect("gray");
        assert!((color[0] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn a_two_stop_ramp_interpolates_at_the_midpoint() {
        let stops = vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])];
        let mid = sample_stops(&stops, &[], 0.5);
        assert!((mid[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_midpoint_moves_the_halfway_color() {
        let stops = vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])];
        // Midpoint 0.25 reaches white earlier.
        assert!((sample_stops(&stops, &[0.25], 0.25)[0] - 0.5).abs() < 1e-6);
        // Past the midpoint the ramp is stretched: t = 0.5 lands at 2/3 white.
        assert!((sample_stops(&stops, &[0.25], 0.5)[0] - 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn a_gradient_midpoint_on_a_stop_maps_to_its_segment() {
        // Each midpoint is stored on the stop that ends its segment: black at
        // 0 with the default 50, white at 1 with `Mdpn` 25.
        let gradient = Descriptor {
            class_id: b"Grdn".to_vec(),
            items: vec![(
                b"Clrs".to_vec(),
                Value::List(vec![
                    gradient_stop(0, 50, 0.0),
                    gradient_stop(4096, 25, 255.0),
                ]),
            )],
        };
        let (stops, midpoints, _) = parse_gradient_stops(&gradient);
        assert!((midpoints[0] - 0.25).abs() < 1e-6, "{midpoints:?}");
        // The half-way colour lands at t = 0.25, not at t = 0.5.
        assert!((sample_stops(&stops, &midpoints, 0.25)[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_linear_gradient_runs_left_to_right() {
        let fill = GradientFill {
            stops: vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])],
            midpoints: Vec::new(),
            opacity_stops: Vec::new(),
            angle: 0.0,
            scale: 1.0,
            style: GradientStyle::Linear,
            reverse: false,
            offset: (0.0, 0.0),
            dither: false,
            align: true,
        };
        let frame = (0.0, 0.0, 10.0, 1.0);
        let left = fill.parameter_at(frame, 0.5, 0.5);
        let right = fill.parameter_at(frame, 9.5, 0.5);
        assert!(left < right, "{left} vs {right}");
        assert!((fill.sample(0.5)[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_reversed_gradient_swaps_its_ends() {
        let mut fill = GradientFill {
            stops: vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])],
            midpoints: Vec::new(),
            opacity_stops: Vec::new(),
            angle: 0.0,
            scale: 1.0,
            style: GradientStyle::Linear,
            reverse: false,
            offset: (0.0, 0.0),
            dither: false,
            align: true,
        };
        let frame = (0.0, 0.0, 10.0, 1.0);
        let forward = fill.parameter_at(frame, 0.5, 0.5);
        fill.reverse = true;
        let backward = fill.parameter_at(frame, 0.5, 0.5);
        assert!((forward + backward - 1.0).abs() < 1e-6);
    }

    #[test]
    fn opacity_stops_scale_the_alpha() {
        let fill = GradientFill {
            stops: vec![(0.0, [1.0, 1.0, 1.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])],
            midpoints: Vec::new(),
            opacity_stops: vec![(0.0, 1.0), (1.0, 0.0)],
            angle: 0.0,
            scale: 1.0,
            style: GradientStyle::Linear,
            reverse: false,
            offset: (0.0, 0.0),
            dither: false,
            align: true,
        };
        assert!((fill.sample(0.5)[3] - 0.5).abs() < 1e-6);
    }
}
