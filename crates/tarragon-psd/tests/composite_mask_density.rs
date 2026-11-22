//! Mask density and the user mask in channel -3 on synthetic in-memory files.
//!
//! Each scene is an 8 by 8 canvas: an opaque black base and a white layer on
//! top that carries the mask. The rules follow files saved by Photoshop: a
//! density below full lets the hidden part show, and when mask flag bit 3 is
//! set, channel -2 holds the rendered vector mask and the user mask is
//! channel -3.

mod common;

use common::{
    DocumentSpec, LayerSpec, build_psd, composite_bytes, rect_path, sample_rgba8, vmsk_block,
};
use tarragon_psd::{ParseOptions, PsdDocument, Viewport};

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

/// Mask flag bit 1: the user mask is disabled.
const DISABLED: u8 = 0x02;
/// Mask flag bit 3: channel -2 holds the rendered vector mask.
const VECTOR_RENDERED: u8 = 0x08;
/// Mask flag bit 4: the mask parameters are present.
const HAS_PARAMETERS: u8 = 0x10;

/// A canvas with a black base and a white layer that carries `edit`.
fn scene(edit: impl FnOnce(LayerSpec) -> LayerSpec) -> Vec<u8> {
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.layers.push(LayerSpec::opaque((0, 0, 8, 8), [0, 0, 0]));
    spec.layers
        .push(edit(LayerSpec::opaque((0, 0, 8, 8), [255, 255, 255])));
    build_psd(&spec)
}

fn push_rect(body: &mut Vec<u8>, rect: [i32; 4]) {
    for edge in rect {
        body.extend_from_slice(&edge.to_be_bytes());
    }
}

/// A mask body over the whole canvas, laid out as Photoshop writes it.
///
/// `flags` gains the parameters bit when `parameters` (the presence byte and
/// its values) is given. With `real` (the real default color), the real
/// fields come right after the flags and the parameters follow them;
/// without, the parameters follow the flags.
fn mask_body(default_color: u8, flags: u8, parameters: Option<&[u8]>, real: Option<u8>) -> Vec<u8> {
    let mut body = Vec::new();
    push_rect(&mut body, [0, 0, 8, 8]);
    let flags = if parameters.is_some() {
        flags | HAS_PARAMETERS
    } else {
        flags
    };
    body.extend_from_slice(&[default_color, flags]);
    if let Some(real_default) = real {
        body.extend_from_slice(&[0, real_default]);
        push_rect(&mut body, [0, 0, 8, 8]);
    }
    if let Some(parameters) = parameters {
        body.extend_from_slice(parameters);
    }
    if real.is_none() && parameters.is_none() {
        body.extend_from_slice(&[0, 0]);
    }
    body
}

/// The centered 4 by 4 square vector mask (pixels 2 to 5).
fn square() -> Vec<u8> {
    vmsk_block(0, &[rect_path(1, 0.25, 0.25, 0.75, 0.75)])
}

/// The red sample at a pixel.
fn red(region: &tarragon_psd::CompositeRegion, x: u32, y: u32) -> u8 {
    sample_rgba8(region, x, y)[0]
}

#[test]
fn a_half_density_shows_half_of_a_hidden_layer() {
    // The mask hides everything; a user density of 128 lets 1 - 128/255 show.
    let region = composite_bytes(scene(|layer| {
        layer.with_raw_mask(mask_body(0, 0, Some(&[0x01, 128]), None))
    }));
    let value = red(&region, 3, 3);
    assert!((126..=128).contains(&value), "got {value}");
}

#[test]
fn a_full_density_keeps_the_mask() {
    let region = composite_bytes(scene(|layer| {
        layer.with_raw_mask(mask_body(0, 0, Some(&[0x01, 255]), None))
    }));
    assert_eq!(sample_rgba8(&region, 3, 3), BLACK);
}

#[test]
fn with_the_vector_rendered_the_user_mask_is_channel_minus_three() {
    // Channel -2 shows and channel -3 hides; -2 is the rendered vector mask
    // here, and the user mask in -3 hides the layer.
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![255; 64]));
        layer.channels.push((-3, vec![0; 64]));
        layer.with_raw_mask(mask_body(0, VECTOR_RENDERED, None, Some(0)))
    }));
    assert_eq!(sample_rgba8(&region, 3, 3), BLACK);
}

#[test]
fn without_the_vector_rendered_channel_minus_three_is_not_read() {
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![0; 64]));
        layer.channels.push((-3, vec![255; 64]));
        layer.with_raw_mask(mask_body(0, 0, None, Some(0)))
    }));
    assert_eq!(sample_rgba8(&region, 3, 3), BLACK);
}

#[test]
fn a_disabled_user_mask_in_channel_minus_three_does_nothing() {
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![255; 64]));
        layer.channels.push((-3, vec![0; 64]));
        layer.with_raw_mask(mask_body(0, VECTOR_RENDERED | DISABLED, None, Some(0)))
    }));
    assert_eq!(sample_rgba8(&region, 3, 3), WHITE);
}

#[test]
fn a_missing_channel_minus_three_fills_the_real_background() {
    // Channel -2 shows everything but is the rendered vector mask; with no
    // channel -3, the real background 0 hides the layer.
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![255; 64]));
        layer.with_raw_mask(mask_body(255, VECTOR_RENDERED, None, Some(0)))
    }));
    assert_eq!(sample_rgba8(&region, 3, 3), BLACK);
}

#[test]
fn the_user_density_applies_to_channel_minus_three() {
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![255; 64]));
        layer.channels.push((-3, vec![0; 64]));
        layer.with_raw_mask(mask_body(0, VECTOR_RENDERED, Some(&[0x01, 128]), Some(0)))
    }));
    let value = red(&region, 3, 3);
    assert!((126..=128).contains(&value), "got {value}");
}

#[test]
fn the_vector_path_is_rendered_and_channel_minus_two_is_not_read() {
    // Channel -2 would hide everything; the path hides only the outside.
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![0; 64]));
        layer.channels.push((-3, vec![255; 64]));
        layer
            .with_raw_mask(mask_body(0, VECTOR_RENDERED, None, Some(0)))
            .with_block(*b"vmsk", square())
    }));
    assert_eq!(sample_rgba8(&region, 3, 3), WHITE);
    assert_eq!(sample_rgba8(&region, 0, 0), BLACK);
}

#[test]
fn the_user_mask_and_the_vector_path_multiply() {
    // Channel -3 hides everything at a user density of 128, so half shows;
    // the square path hides its outside fully.
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![255; 64]));
        layer.channels.push((-3, vec![0; 64]));
        layer
            .with_raw_mask(mask_body(0, VECTOR_RENDERED, Some(&[0x01, 128]), Some(0)))
            .with_block(*b"vmsk", square())
    }));
    let inside = red(&region, 3, 3);
    assert!((126..=128).contains(&inside), "got {inside}");
    assert_eq!(sample_rgba8(&region, 0, 0), BLACK);
}

#[test]
fn without_a_usable_path_channel_minus_two_is_the_vector_mask() {
    // The path block is too short to parse, so the rendered vector mask in
    // channel -2 hides the layer.
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![0; 64]));
        layer.channels.push((-3, vec![255; 64]));
        layer
            .with_raw_mask(mask_body(0, VECTOR_RENDERED, None, Some(0)))
            .with_block(*b"vmsk", vec![0; 3])
    }));
    assert_eq!(sample_rgba8(&region, 3, 3), BLACK);
}

#[test]
fn the_vector_density_applies_to_the_channel_minus_two_stand_in() {
    // The path block is too short to parse; channel -2 hides everything, and
    // a vector density of 128 lets half show.
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![0; 64]));
        layer.channels.push((-3, vec![255; 64]));
        layer
            .with_raw_mask(mask_body(0, VECTOR_RENDERED, Some(&[0x04, 128]), Some(0)))
            .with_block(*b"vmsk", vec![0; 3])
    }));
    let value = red(&region, 3, 3);
    assert!((126..=128).contains(&value), "got {value}");
}

#[test]
fn without_real_fields_channel_minus_two_is_the_vector_mask() {
    // Bit 3 and a path but no real fields, as Krita writes a raster mask:
    // channel -2 hides the left half and the path, covering everything, is
    // not drawn.
    let left_hidden: Vec<u16> = (0..64).map(|i| if i % 8 < 4 { 0 } else { 255 }).collect();
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, left_hidden));
        layer
            .with_raw_mask(mask_body(0, VECTOR_RENDERED, None, None))
            .with_block(*b"vmsk", vmsk_block(0, &[rect_path(1, 0.0, 0.0, 1.0, 1.0)]))
    }));
    assert_eq!(sample_rgba8(&region, 1, 3), BLACK);
    assert_eq!(sample_rgba8(&region, 6, 3), WHITE);
}

#[test]
fn a_vector_mask_density_shows_the_outside() {
    // The vector square hides its outside, and a vector density of 128 lets
    // half of that outside show.
    let region = composite_bytes(scene(|layer| {
        layer
            .with_raw_mask(mask_body(255, 0, Some(&[0x04, 128]), None))
            .with_block(*b"vmsk", square())
    }));
    assert_eq!(sample_rgba8(&region, 3, 3), WHITE);
    let outside = red(&region, 0, 0);
    assert!((126..=128).contains(&outside), "got {outside}");
}

/// The mask channel IDs decoded for the white layer of `scene(edit)`, which
/// always carries channels -2 and -3.
fn decoded_masks(edit: impl FnOnce(LayerSpec) -> LayerSpec) -> Vec<i16> {
    let bytes = scene(|mut layer| {
        layer.channels.push((-2, vec![0; 64]));
        layer.channels.push((-3, vec![0; 64]));
        edit(layer)
    });
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("the file opens");
    let region = document
        .read_layer_region(1, Viewport::full(8, 8), &ParseOptions::none())
        .expect("the layer decodes");
    region
        .channels
        .iter()
        .map(|channel| channel.id)
        .filter(|id| *id < -1)
        .collect()
}

#[test]
fn only_the_mask_channels_the_compositor_reads_are_decoded() {
    assert_eq!(decoded_masks(|layer| layer), Vec::<i16>::new());
    assert_eq!(
        decoded_masks(|layer| layer.with_raw_mask(mask_body(0, 0, None, Some(0)))),
        [-2]
    );
    assert_eq!(
        decoded_masks(|layer| {
            layer
                .with_raw_mask(mask_body(0, VECTOR_RENDERED, None, Some(0)))
                .with_block(*b"vmsk", square())
        }),
        [-3]
    );
    assert_eq!(
        decoded_masks(|layer| layer.with_raw_mask(mask_body(0, VECTOR_RENDERED, None, Some(0)))),
        [-2, -3]
    );
    assert_eq!(
        decoded_masks(|layer| {
            layer
                .with_raw_mask(mask_body(0, VECTOR_RENDERED, None, None))
                .with_block(*b"vmsk", square())
        }),
        [-2]
    );
}

#[test]
fn a_disabled_path_turns_off_the_vector_mask() {
    // The vector mask block with its disabled flag (0x4).
    let disabled = || vmsk_block(4, &[rect_path(1, 0.25, 0.25, 0.75, 0.75)]);
    // Without real fields, no channel -2 stand-in is decoded.
    assert_eq!(
        decoded_masks(|layer| {
            layer
                .with_raw_mask(mask_body(0, VECTOR_RENDERED, None, None))
                .with_block(*b"vmsk", disabled())
        }),
        Vec::<i16>::new()
    );
    // With them, only the user mask is decoded and the path hides nothing.
    let edit = |layer: LayerSpec| {
        layer
            .with_raw_mask(mask_body(0, VECTOR_RENDERED, None, Some(255)))
            .with_block(*b"vmsk", disabled())
    };
    assert_eq!(decoded_masks(edit), [-3]);
    let region = composite_bytes(scene(|mut layer| {
        layer.channels.push((-2, vec![0; 64]));
        layer.channels.push((-3, vec![255; 64]));
        edit(layer)
    }));
    assert_eq!(sample_rgba8(&region, 0, 0), WHITE);
}
