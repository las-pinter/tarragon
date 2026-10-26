//! Color Lookup (`clrL`) layers on synthetic PSD files.

mod common;

use std::fmt::Write;

use common::{
    DocumentSpec, LayerSpec, build_psd, clrl_block, composite_bytes, sample_rgba8, scaled,
};

/// A 2x2x2 cube text; `map` turns the (r, g, b) corner into its output.
fn cube(map: impl Fn([f32; 3]) -> [f32; 3]) -> Vec<u8> {
    let mut text = String::from("TITLE \"t\"\nLUT_3D_SIZE 2\n");
    for b in [0.0f32, 1.0] {
        for g in [0.0f32, 1.0] {
            for r in [0.0f32, 1.0] {
                let out = map([r, g, b]);
                let _ = writeln!(text, "{} {} {}", out[0], out[1], out[2]);
            }
        }
    }
    text.into_bytes()
}

/// Composite one `clrL` block over one pixel, as RGB8.
fn run(depth: u16, pixel: [u8; 3], block: Vec<u8>) -> [u8; 3] {
    let mut spec = DocumentSpec::new(1, 1, depth, 3);
    let values = pixel.map(|value| scaled(u16::from(value), depth));
    spec.layers.push(LayerSpec::opaque((0, 0, 1, 1), values));
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"clrL", block));
    let region = composite_bytes(build_psd(&spec));
    let out = sample_rgba8(&region, 0, 0);
    [out[0], out[1], out[2]]
}

fn swap() -> Vec<u8> {
    clrl_block(
        b"3DLUT",
        b"LUTFormatCUBE",
        Some(&cube(|c| [c[2], c[1], c[0]])),
        false,
    )
}

#[test]
fn a_swap_cube_swaps_red_and_blue() {
    assert_eq!(run(8, [200, 100, 20], swap()), [20, 100, 200]);
}

#[test]
fn an_identity_cube_keeps_the_pixel() {
    let block = clrl_block(b"3DLUT", b"LUTFormatCUBE", Some(&cube(|c| c)), false);
    let out = run(8, [200, 100, 20], block);
    assert_eq!(out, [200, 100, 20]);
}

#[test]
fn the_dither_flag_does_not_change_the_result() {
    let block = clrl_block(
        b"3DLUT",
        b"LUTFormatCUBE",
        Some(&cube(|c| [c[2], c[1], c[0]])),
        true,
    );
    assert_eq!(run(8, [200, 100, 20], block), [20, 100, 200]);
}

#[test]
fn eight_and_sixteen_bit_agree_within_one_level() {
    let pixel = [200, 100, 20];
    let a = run(8, pixel, swap());
    let b = run(16, pixel, swap());
    for k in 0..3 {
        assert!(a[k].abs_diff(b[k]) <= 1, "{a:?} {b:?}");
    }
}

#[test]
fn unsupported_lookups_leave_the_pixel_alone() {
    let cases = [
        clrl_block(b"abstractProfile", b"LUTFormatCUBE", None, false),
        clrl_block(b"deviceLinkProfile", b"LUTFormatCUBE", None, false),
        clrl_block(
            b"3DLUT",
            b"LUTFormat3DL",
            Some(&cube(|c| [c[2], c[1], c[0]])),
            false,
        ),
        clrl_block(
            b"3DLUT",
            b"LUTFormatCUBE",
            Some(b"LUT_3D_SIZE 2\n1 2"),
            false,
        ),
        clrl_block(b"3DLUT", b"LUTFormatCUBE", None, false),
    ];
    for block in cases {
        assert_eq!(run(8, [200, 100, 20], block), [200, 100, 20]);
    }
}
