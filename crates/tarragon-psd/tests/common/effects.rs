//! Encoders and builders for layer-effects tests.
//!
//! The helpers build the object-based `lfx2` block and the legacy `lrFX`
//! block. They follow the same descriptor bytes the Photoshop writer uses.
//! The document helpers build one small synthetic file for a paint test.

use super::builder::{DocumentSpec, LayerSpec, build_psd, push_u16, push_u32};
use super::encoders::{
    descriptor_body, descriptor_item, rgbc, value_boolean, value_descriptor, value_double,
    value_enum, value_integer, value_list, value_unit_float,
};

/// The version bytes before an object-based effects descriptor.
fn lfx2_header() -> Vec<u8> {
    let mut data = 1u32.to_be_bytes().to_vec();
    data.extend_from_slice(&16u32.to_be_bytes());
    data
}

/// Build the lfx2 block data from top-level effect bodies.
#[must_use]
pub fn lfx2(effects: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let items: Vec<Vec<u8>> = effects
        .iter()
        .map(|(key, body)| descriptor_item(key, &value_descriptor(body)))
        .collect();
    let mut data = lfx2_header();
    data.extend_from_slice(&descriptor_body(b"null", &items));
    data
}

/// Build the lfx2 block data with a false master switch.
#[must_use]
pub fn lfx2_disabled(effects: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut items: Vec<Vec<u8>> = vec![descriptor_item(b"masterFXSwitch", &value_boolean(false))];
    items.extend(
        effects
            .iter()
            .map(|(key, body)| descriptor_item(key, &value_descriptor(body))),
    );
    let mut data = lfx2_header();
    data.extend_from_slice(&descriptor_body(b"null", &items));
    data
}

/// The shared enabled, blend, and opacity items.
#[must_use]
pub fn common(enabled: bool, blend: &[u8], opacity: f64) -> Vec<Vec<u8>> {
    vec![
        descriptor_item(b"enab", &value_boolean(enabled)),
        descriptor_item(b"present", &value_boolean(true)),
        descriptor_item(b"showInDialog", &value_boolean(true)),
        descriptor_item(b"Md  ", &value_enum(b"BlnM", blend)),
        descriptor_item(b"Opct", &value_unit_float(*b"#Prc", opacity)),
    ]
}

/// A color item.
#[must_use]
pub fn color_item(key: &[u8], red: f64, green: f64, blue: f64) -> Vec<u8> {
    descriptor_item(key, &value_descriptor(&rgbc(red, green, blue)))
}

/// A pixel item.
#[must_use]
pub fn pixel_item(key: &[u8], value: f64) -> Vec<u8> {
    descriptor_item(key, &value_unit_float(*b"#Pxl", value))
}

/// A percent item.
#[must_use]
pub fn percent_item(key: &[u8], value: f64) -> Vec<u8> {
    descriptor_item(key, &value_unit_float(*b"#Prc", value))
}

/// An angle item.
#[must_use]
pub fn angle_item(key: &[u8], value: f64) -> Vec<u8> {
    descriptor_item(key, &value_unit_float(*b"#Ang", value))
}

/// A boolean item.
#[must_use]
pub fn bool_item(key: &[u8], value: bool) -> Vec<u8> {
    descriptor_item(key, &value_boolean(value))
}

/// Set a boolean item in a descriptor body in place.
///
/// The body must already hold the item. The helper rewrites the value byte
/// only, so the item order and every other field stay as they are.
pub fn set_bool_item(body: &mut [u8], key: &[u8], value: bool) {
    let mut pattern = key.to_vec();
    pattern.extend_from_slice(b"bool");
    let position = body
        .windows(pattern.len())
        .position(|window| window == pattern.as_slice())
        .unwrap_or_else(|| panic!("key {key:?} is in the body"));
    body[position + key.len() + 4] = u8::from(value);
}

/// Set an enumerated item in a descriptor body in place.
///
/// The body must already hold the item. The layout is the key, the `enum`
/// type, the type id, and the value, and each id carries a length prefix.
pub fn set_enum_item(body: &mut [u8], key: &[u8], value: [u8; 4]) {
    let mut pattern = key.to_vec();
    pattern.extend_from_slice(b"enum");
    let position = body
        .windows(pattern.len())
        .position(|window| window == pattern.as_slice())
        .unwrap_or_else(|| panic!("key {key:?} is in the body"));
    let start = position + key.len() + 16;
    body[start..start + 4].copy_from_slice(&value);
}

/// A background plus one effect square at (24,24)-(40,40).
///
/// The last argument is the layer opacity.
#[must_use]
pub fn square_document_with_opacity(
    key: [u8; 4],
    data: Vec<u8>,
    color: [u16; 3],
    opacity: u8,
) -> Vec<u8> {
    let mut spec = square_document_spec(key, data, color);
    spec.layers[1].opacity = opacity;
    build_psd(&spec)
}

/// A background plus one effect square at (24,24)-(40,40).
///
/// The last argument is the fill opacity block value.
#[must_use]
pub fn square_document_with_fill_opacity(
    key: [u8; 4],
    data: Vec<u8>,
    color: [u16; 3],
    fill_opacity: Option<u8>,
) -> Vec<u8> {
    let mut spec = square_document_spec(key, data, color);
    spec.layers[1].fill_opacity = fill_opacity;
    build_psd(&spec)
}

/// The document behind the square helpers, before the opacity is set.
fn square_document_spec(key: [u8; 4], data: Vec<u8>, color: [u16; 3]) -> DocumentSpec {
    let mut spec = DocumentSpec::new(64, 64, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 64, 64), [200, 200, 200]));
    spec.layers.push(
        LayerSpec::with_alpha((24, 24, 40, 40), [color[0], color[1], color[2], 255])
            .with_block(key, data),
    );
    spec
}

/// A linear identity contour descriptor.
#[must_use]
pub fn linear_contour() -> Vec<u8> {
    let point = |input: f64, output: f64| {
        value_descriptor(&descriptor_body(
            b"CrPt",
            &[
                descriptor_item(b"Hrzn", &value_double(input)),
                descriptor_item(b"Vrtc", &value_double(output)),
            ],
        ))
    };
    let items = [descriptor_item(
        b"Crv ",
        &value_list(&[point(0.0, 0.0), point(255.0, 255.0)]),
    )];
    value_descriptor(&descriptor_body(b"ShpC", &items))
}

/// The `Grad` item of a two-stop black-to-white linear gradient.
#[must_use]
fn black_to_white_gradient_item() -> Vec<u8> {
    let stop = |location: i32, grey: f64| {
        value_descriptor(&descriptor_body(
            b"Clrt",
            &[
                descriptor_item(b"Lctn", &value_integer(location)),
                color_item(b"Clr ", grey, grey, grey),
                descriptor_item(b"Mdpn", &value_integer(50)),
            ],
        ))
    };
    descriptor_item(
        b"Grad",
        &value_descriptor(&descriptor_body(
            b"Grdn",
            &[
                descriptor_item(b"Clrs", &value_list(&[stop(0, 0.0), stop(4096, 255.0)])),
                descriptor_item(b"Trns", &value_list(&[])),
            ],
        )),
    )
}

/// Build a drop shadow or inner shadow effect.
#[allow(
    clippy::too_many_arguments,
    reason = "the fixture names each shadow field explicitly"
)]
#[must_use]
pub fn shadow_effect(
    inner: bool,
    color: [f64; 3],
    angle: f64,
    distance: f64,
    size: f64,
    spread: f64,
    opacity: f64,
    knocks_out: bool,
) -> ([u8; 4], Vec<u8>) {
    let mut items = common(true, b"Mltp", opacity);
    items.push(color_item(b"Clr ", color[0], color[1], color[2]));
    items.push(angle_item(b"lagl", angle));
    items.push(bool_item(b"uglg", false));
    items.push(pixel_item(b"Dstn", distance));
    items.push(percent_item(b"Ckmt", spread));
    items.push(pixel_item(b"blur", size));
    items.push(bool_item(b"layerConceals", knocks_out));
    items.push(descriptor_item(b"TrnS", &linear_contour()));
    let key = if inner { *b"IrSh" } else { *b"DrSh" };
    (key, descriptor_body(&key, &items))
}

/// Build an outer glow or inner glow effect.
#[must_use]
pub fn glow_effect(
    inner: bool,
    center: bool,
    color: [f64; 3],
    size: f64,
    spread: f64,
    range: f64,
    opacity: f64,
) -> ([u8; 4], Vec<u8>) {
    let mut items = common(true, b"Scrn", opacity);
    items.push(color_item(b"Clr ", color[0], color[1], color[2]));
    items.push(percent_item(b"Ckmt", spread));
    items.push(pixel_item(b"blur", size));
    items.push(percent_item(b"Inpr", range));
    items.push(value_enum_item(b"GlwT", b"GlwT", b"SfBL"));
    if inner && center {
        items.push(value_enum_item(b"glwS", b"GlwS", b"SrcC"));
    }
    items.push(descriptor_item(b"TrnS", &linear_contour()));
    let key = if inner { *b"IrGl" } else { *b"OrGl" };
    (key, descriptor_body(&key, &items))
}

/// An enumerated item.
#[must_use]
pub fn value_enum_item(key: &[u8], type_id: &[u8], value: &[u8]) -> Vec<u8> {
    descriptor_item(key, &value_enum(type_id, value))
}

/// Build a stroke effect.
#[must_use]
pub fn stroke_effect(color: [f64; 3], size: f64, inside: bool, opacity: f64) -> ([u8; 4], Vec<u8>) {
    let mut items = common(true, b"Nrml", opacity);
    items.push(pixel_item(b"Sz  ", size));
    items.push(value_enum_item(
        b"Styl",
        b"FStl",
        if inside { b"InsF" } else { b"OutF" },
    ));
    items.push(value_enum_item(b"PntT", b"FrFl", b"SClr"));
    items.push(color_item(b"Clr ", color[0], color[1], color[2]));
    (*b"FrFX", descriptor_body(b"FrFX", &items))
}

/// Build a gradient stroke effect.
#[must_use]
pub fn gradient_stroke_effect(
    angle: f64,
    size: f64,
    inside: bool,
    opacity: f64,
) -> ([u8; 4], Vec<u8>) {
    let mut items = common(true, b"Nrml", opacity);
    items.push(pixel_item(b"Sz  ", size));
    items.push(value_enum_item(
        b"Styl",
        b"FStl",
        if inside { b"InsF" } else { b"OutF" },
    ));
    items.push(value_enum_item(b"PntT", b"FrFl", b"GrFl"));
    items.push(black_to_white_gradient_item());
    items.push(angle_item(b"Angl", angle));
    items.push(value_enum_item(b"Type", b"GrdT", b"Lnr "));
    items.push(descriptor_item(b"Scl ", &value_unit_float(*b"#Prc", 100.0)));
    (*b"FrFX", descriptor_body(b"FrFX", &items))
}

/// Build a solid color overlay effect.
#[must_use]
pub fn solid_overlay_effect(color: [f64; 3], opacity: f64) -> ([u8; 4], Vec<u8>) {
    let mut items = common(true, b"Nrml", opacity);
    items.push(color_item(b"Clr ", color[0], color[1], color[2]));
    (*b"SoFi", descriptor_body(b"SoFi", &items))
}

/// Build a gradient overlay effect.
#[must_use]
pub fn gradient_overlay_effect(angle: f64, opacity: f64) -> ([u8; 4], Vec<u8>) {
    let mut items = common(true, b"Nrml", opacity);
    items.push(black_to_white_gradient_item());
    items.push(angle_item(b"Angl", angle));
    items.push(value_enum_item(b"Type", b"GrdT", b"Lnr "));
    items.push(descriptor_item(b"Scl ", &value_unit_float(*b"#Prc", 100.0)));
    (*b"GrFl", descriptor_body(b"GrFl", &items))
}

/// Build a bevel and emboss effect.
#[must_use]
pub fn bevel_effect(
    size: f64,
    depth: f64,
    angle: f64,
    altitude: f64,
    highlight: [f64; 3],
    shadow: [f64; 3],
    opacity: f64,
) -> ([u8; 4], Vec<u8>) {
    let mut items = vec![
        bool_item(b"enab", true),
        value_enum_item(b"bvlS", b"BvlS", b"In  "),
        value_enum_item(b"bvlT", b"BvlT", b"SfBL"),
        value_enum_item(b"bvlD", b"In  ", b"In  "),
        percent_item(b"srgR", depth),
        pixel_item(b"blur", size),
        pixel_item(b"Sftn", 0.0),
        angle_item(b"lagl", angle),
        angle_item(b"Lald", altitude),
        bool_item(b"uglg", false),
    ];
    items.push(value_enum_item(b"hglM", b"BlnM", b"Scrn"));
    items.push(percent_item(b"hglO", opacity));
    items.push(color_item(
        b"hglC",
        highlight[0],
        highlight[1],
        highlight[2],
    ));
    items.push(value_enum_item(b"sdwM", b"BlnM", b"Mltp"));
    items.push(percent_item(b"sdwO", opacity));
    items.push(color_item(b"sdwC", shadow[0], shadow[1], shadow[2]));
    items.push(descriptor_item(b"TrnS", &linear_contour()));
    (*b"ebbl", descriptor_body(b"ebbl", &items))
}

/// Build a satin effect.
#[must_use]
pub fn satin_effect(
    color: [f64; 3],
    angle: f64,
    distance: f64,
    size: f64,
    invert: bool,
    opacity: f64,
) -> ([u8; 4], Vec<u8>) {
    let mut items = common(true, b"Mltp", opacity);
    items.push(color_item(b"Clr ", color[0], color[1], color[2]));
    items.push(angle_item(b"lagl", angle));
    items.push(pixel_item(b"Dstn", distance));
    items.push(pixel_item(b"blur", size));
    items.push(bool_item(b"Invr", invert));
    items.push(descriptor_item(b"MpgS", &linear_contour()));
    (*b"ChFX", descriptor_body(b"ChFX", &items))
}

/// Push a 16.16 fixed-point value.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to the u32 range first"
)]
pub fn push_fixed(data: &mut Vec<u8>, value: f32) {
    let raw = (f64::from(value) * 65536.0)
        .round()
        .clamp(0.0, f64::from(u32::MAX));
    push_u32(data, raw as u32);
}

/// Push a legacy color record.
pub fn push_legacy_color(data: &mut Vec<u8>, red: u16, green: u16, blue: u16) {
    push_u16(data, 0);
    for value in [red, green, blue, 0] {
        push_u16(data, value);
    }
}

/// Build a legacy `lrFX` block from keyed items.
#[must_use]
pub fn legacy_lrfx(items: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, 0);
    push_u16(&mut data, u16::try_from(items.len()).expect("item count"));
    for (key, body) in items {
        data.extend_from_slice(b"8BIM");
        data.extend_from_slice(key);
        push_u32(&mut data, u32::try_from(body.len()).expect("body length"));
        data.extend_from_slice(body);
    }
    data
}

/// Build a legacy drop shadow or inner shadow body.
#[must_use]
pub fn legacy_shadow_body(
    angle: f32,
    distance: f32,
    size: f32,
    opacity: u8,
    enabled: bool,
) -> Vec<u8> {
    legacy_shadow_body_with_light(angle, distance, size, opacity, enabled, false)
}

/// Build a legacy shadow body with a chosen global-light flag.
#[must_use]
pub fn legacy_shadow_body_with_light(
    angle: f32,
    distance: f32,
    size: f32,
    opacity: u8,
    enabled: bool,
    use_global: bool,
) -> Vec<u8> {
    let mut data = Vec::new();
    push_u32(&mut data, 0);
    push_fixed(&mut data, size);
    push_u32(&mut data, 0);
    push_fixed(&mut data, angle);
    push_fixed(&mut data, distance);
    push_legacy_color(&mut data, 0, 0, 0);
    data.extend_from_slice(b"8BIM");
    data.extend_from_slice(b"mul ");
    data.push(u8::from(enabled));
    data.push(u8::from(use_global));
    data.push(opacity);
    data
}
