//! A 3D color lookup table: a `.cube` text parser and trilinear sampling.
//!
//! The `.cube` text holds a `LUT_3D_SIZE`, an optional `DOMAIN_MIN` and
//! `DOMAIN_MAX`, and then `size^3` lines of three floats. Red changes fastest,
//! then green, then blue. A bad file gives `None`; nothing here panics.

/// The smallest accepted table edge.
const MIN_SIZE: usize = 2;

/// A guard against overflow; `MAX_ENTRIES` is the real limit (an edge of 65).
const MAX_SIZE: usize = 256;

/// The largest accepted number of table entries (a 65-point cube).
const MAX_ENTRIES: usize = 65 * 65 * 65;

/// A cube of RGB output colors.
#[derive(Debug, Clone, PartialEq)]
pub struct Lut3d {
    /// The number of samples on each edge.
    size: usize,
    /// The input range that maps to the table edges.
    domain_min: [f32; 3],
    /// The input range that maps to the table edges.
    domain_max: [f32; 3],
    /// `size^3` RGB triplets, red fastest, then green, then blue.
    data: Vec<[f32; 3]>,
}

impl Lut3d {
    /// Parse the text of a `.cube` file.
    ///
    /// Returns `None` for a missing or out-of-range size, a bad number, a
    /// short table, or a domain that has no width.
    #[must_use]
    pub fn parse_cube(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?;
        let mut size = None;
        let mut domain_min = [0.0f32; 3];
        let mut domain_max = [1.0f32; 3];
        let mut data: Vec<[f32; 3]> = Vec::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let first = words.next()?;
            match first {
                "TITLE" => {}
                "LUT_3D_INPUT_RANGE" => {
                    let [low, high] = two_floats(words)?;
                    domain_min = [low; 3];
                    domain_max = [high; 3];
                }
                // A 1D table is not a color cube.
                "LUT_1D_SIZE" | "LUT_1D_INPUT_RANGE" => return None,
                "LUT_3D_SIZE" => {
                    let value: usize = words.next()?.parse().ok()?;
                    if !(MIN_SIZE..=MAX_SIZE).contains(&value) {
                        return None;
                    }
                    let entries = value.checked_mul(value)?.checked_mul(value)?;
                    if entries > MAX_ENTRIES {
                        return None;
                    }
                    size = Some(value);
                }
                "DOMAIN_MIN" => domain_min = three_floats(words)?,
                "DOMAIN_MAX" => domain_max = three_floats(words)?,
                _ => {
                    let size = size?;
                    let entries = size * size * size;
                    if data.len() >= entries {
                        return None;
                    }
                    let mut all = std::iter::once(first).chain(words);
                    let triplet = three_floats(&mut all)?;
                    if all.next().is_some() {
                        return None;
                    }
                    data.push(triplet);
                }
            }
        }
        let size = size?;
        if data.len() != size * size * size {
            return None;
        }
        if (0..3).any(|k| domain_max[k] <= domain_min[k]) {
            return None;
        }
        Some(Self {
            size,
            domain_min,
            domain_max,
            data,
        })
    }

    /// The number of samples on each edge.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Sample the table at `rgb` with trilinear interpolation.
    ///
    /// The input is mapped through the domain and clamped to the table edges.
    /// The output is clamped to `0.0..=1.0`.
    #[must_use]
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        #[allow(clippy::cast_precision_loss, reason = "the edge is at most 65")]
        let top = (n - 1) as f32;
        let position: [f32; 3] = std::array::from_fn(|k| {
            let unit = (rgb[k] - self.domain_min[k]) / (self.domain_max[k] - self.domain_min[k]);
            let unit = if unit.is_finite() { unit } else { 0.0 };
            unit.clamp(0.0, 1.0) * top
        });
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the position is in 0.0..=64.0"
        )]
        let low: [usize; 3] = position.map(|p| (p.floor() as usize).min(n - 2));
        #[allow(clippy::cast_precision_loss, reason = "the index is at most 64")]
        let fraction: [f32; 3] = std::array::from_fn(|k| position[k] - low[k] as f32);
        let at = |dr: usize, dg: usize, db: usize| -> [f32; 3] {
            let index = ((low[2] + db) * n + low[1] + dg) * n + low[0] + dr;
            self.data.get(index).copied().unwrap_or([0.0; 3])
        };
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
            std::array::from_fn(|k| (b[k] - a[k]).mul_add(t, a[k]))
        };
        let c00 = lerp(at(0, 0, 0), at(1, 0, 0), fraction[0]);
        let c10 = lerp(at(0, 1, 0), at(1, 1, 0), fraction[0]);
        let c01 = lerp(at(0, 0, 1), at(1, 0, 1), fraction[0]);
        let c11 = lerp(at(0, 1, 1), at(1, 1, 1), fraction[0]);
        let out = lerp(
            lerp(c00, c10, fraction[1]),
            lerp(c01, c11, fraction[1]),
            fraction[2],
        );
        out.map(|v| v.clamp(0.0, 1.0))
    }
}

/// Read exactly two finite floats from `words`.
fn two_floats<'a>(mut words: impl Iterator<Item = &'a str>) -> Option<[f32; 2]> {
    let mut out = [0.0f32; 2];
    for slot in &mut out {
        let value: f32 = words.next()?.parse().ok()?;
        if !value.is_finite() {
            return None;
        }
        *slot = value;
    }
    Some(out)
}

/// Read exactly three finite floats from `words`.
fn three_floats<'a>(mut words: impl Iterator<Item = &'a str>) -> Option<[f32; 3]> {
    let mut out = [0.0f32; 3];
    for slot in &mut out {
        let value: f32 = words.next()?.parse().ok()?;
        if !value.is_finite() {
            return None;
        }
        *slot = value;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::Lut3d;

    /// A cube text with `size` and the given entries.
    fn cube(size: usize, header: &str, entries: &[[f32; 3]]) -> Vec<u8> {
        let mut text = format!("TITLE \"t\"\n# note\nLUT_3D_SIZE {size}\n{header}\n");
        for e in entries {
            let _ = writeln!(text, "{} {} {}", e[0], e[1], e[2]);
        }
        text.into_bytes()
    }

    /// The 2x2x2 identity entries, red fastest.
    fn identity2() -> Vec<[f32; 3]> {
        let mut v = Vec::new();
        for b in [0.0f32, 1.0] {
            for g in [0.0f32, 1.0] {
                for r in [0.0f32, 1.0] {
                    v.push([r, g, b]);
                }
            }
        }
        v
    }

    #[test]
    fn a_two_cube_identity_parses() {
        let lut = Lut3d::parse_cube(&cube(2, "", &identity2())).expect("parses");
        assert_eq!(lut.size(), 2);
    }

    #[test]
    fn trilinear_identity_returns_the_input() {
        let lut = Lut3d::parse_cube(&cube(2, "", &identity2())).expect("parses");
        for input in [[0.1, 0.5, 0.9], [0.0, 0.0, 0.0], [1.0, 0.3, 0.7]] {
            let out = lut.sample(input);
            for k in 0..3 {
                assert!((out[k] - input[k]).abs() < 1e-5, "{input:?} {out:?}");
            }
        }
    }

    #[test]
    fn a_red_blue_swap_cube_swaps_red_and_blue() {
        let entries: Vec<[f32; 3]> = identity2().iter().map(|e| [e[2], e[1], e[0]]).collect();
        let lut = Lut3d::parse_cube(&cube(2, "", &entries)).expect("parses");
        let out = lut.sample([0.2, 0.5, 0.8]);
        assert!((out[0] - 0.8).abs() < 1e-5 && (out[2] - 0.2).abs() < 1e-5);
        assert!((out[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn the_domain_scales_the_input() {
        let header = "DOMAIN_MIN 0 0 0\nDOMAIN_MAX 0.5 0.5 0.5";
        let lut = Lut3d::parse_cube(&cube(2, header, &identity2())).expect("parses");
        let out = lut.sample([0.25, 0.5, 1.0]);
        assert!((out[0] - 0.5).abs() < 1e-5);
        assert!((out[1] - 1.0).abs() < 1e-5);
        assert!((out[2] - 1.0).abs() < 1e-5);
    }

    /// A cube of size `n` that holds `v^2` of each node coordinate, red fastest.
    fn squared(n: u8) -> Vec<[f32; 3]> {
        let step = |i: u8| {
            let t = f32::from(i) / f32::from(n - 1);
            t * t
        };
        let mut v = Vec::new();
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    v.push([step(r), step(g), step(b)]);
                }
            }
        }
        v
    }

    #[test]
    fn a_three_cube_with_curved_entries_hits_nodes_and_edges() {
        let lut = Lut3d::parse_cube(&cube(3, "", &squared(3))).expect("parses");
        let out = lut.sample([0.0, 0.5, 1.0]);
        assert!(out[0].abs() < 1e-6, "{out:?}");
        assert!((out[1] - 0.25).abs() < 1e-6, "{out:?}");
        assert!((out[2] - 1.0).abs() < 1e-6, "{out:?}");
        // Between nodes 0.5 and 1.0 the value is the linear mix of 0.25 and 1.0.
        let mid = lut.sample([0.75, 0.75, 0.75]);
        for value in mid {
            assert!((value - 0.625).abs() < 1e-6, "{mid:?}");
        }
    }

    #[test]
    fn the_upper_edge_of_a_larger_table_is_exact() {
        let lut = Lut3d::parse_cube(&cube(17, "", &squared(17))).expect("parses");
        let out = lut.sample([1.0, 1.0, 1.0]);
        assert_eq!(out, [1.0, 1.0, 1.0]);
        let node = lut.sample([8.0 / 16.0, 15.0 / 16.0, 1.0 / 16.0]);
        assert!((node[0] - 0.25).abs() < 1e-5 && (node[2] - 1.0 / 256.0).abs() < 1e-5);
    }

    #[test]
    fn crlf_exponents_comments_and_blank_lines_parse() {
        let mut text = String::from("LUT_3D_SIZE 2\r\n\r\n");
        for e in identity2() {
            let _ = write!(text, "{:E} {} {} # point\r\n", e[0] * 0.1, e[1], e[2]);
        }
        text.push_str("1.0E-1 0 0\r\n");
        // The last line is one too many; remove it again and add a blank line.
        text.truncate(text.len() - "1.0E-1 0 0\r\n".len());
        text.push_str("\r\n");
        let lut = Lut3d::parse_cube(text.as_bytes()).expect("parses");
        let out = lut.sample([1.0, 0.0, 0.0]);
        assert!((out[0] - 0.1).abs() < 1e-6, "{out:?}");
    }

    #[test]
    fn the_input_range_sets_the_domain_of_all_channels() {
        let text =
            String::from_utf8(cube(2, "LUT_3D_INPUT_RANGE 0 0.5", &identity2())).expect("utf8");
        let lut = Lut3d::parse_cube(text.as_bytes()).expect("parses");
        let out = lut.sample([0.25, 0.5, 1.0]);
        assert!(
            (out[0] - 0.5).abs() < 1e-5
                && (out[1] - 1.0).abs() < 1e-5
                && (out[2] - 1.0).abs() < 1e-5
        );
    }

    #[test]
    fn bad_files_give_none() {
        let good = identity2();
        // Sizes out of range, or missing.
        assert!(Lut3d::parse_cube(&cube(1, "", &good)).is_none());
        assert!(Lut3d::parse_cube(&cube(257, "", &good)).is_none());
        assert!(Lut3d::parse_cube(&cube(100, "", &good)).is_none());
        assert!(Lut3d::parse_cube(b"0 0 0\n").is_none());
        // Short and long data.
        assert!(Lut3d::parse_cube(&cube(2, "", &good[..7])).is_none());
        let mut long = good.clone();
        long.push([0.0; 3]);
        assert!(Lut3d::parse_cube(&cube(2, "", &long)).is_none());
        // Not numbers.
        let text = String::from_utf8(cube(2, "", &good))
            .expect("utf8")
            .replace("1 0 0", "x 0 0");
        assert!(Lut3d::parse_cube(text.as_bytes()).is_none());
        assert!(Lut3d::parse_cube(b"LUT_3D_SIZE two\n").is_none());
        // A zero-width domain, a 1D table, and bytes that are not text.
        let flat = "DOMAIN_MIN 0 0 0\nDOMAIN_MAX 0 1 1";
        assert!(Lut3d::parse_cube(&cube(2, flat, &good)).is_none());
        assert!(Lut3d::parse_cube(b"LUT_1D_SIZE 4\n0 0 0\n").is_none());
        assert!(Lut3d::parse_cube(&[0xff, 0xfe, 0x00]).is_none());
        assert!(Lut3d::parse_cube(b"").is_none());
    }
}
