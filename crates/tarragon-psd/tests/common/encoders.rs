//! Encoders for the descriptor and adjustment blocks of a synthetic PSD.

use super::builder::{push_u16, push_u32};

// --------------------------------------------------------------------------
// Descriptor and block encoders
// --------------------------------------------------------------------------

/// Push a four-byte code or a length-prefixed id.
pub fn push_id(data: &mut Vec<u8>, id: &[u8]) {
    if id.len() == 4 {
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(id);
    } else {
        push_u32(data, u32::try_from(id.len()).expect("id length"));
        data.extend_from_slice(id);
    }
}

/// Push a unicode string: a count then UTF-16BE units.
pub fn push_unicode(data: &mut Vec<u8>, value: &str) {
    let units: Vec<u16> = value.encode_utf16().collect();
    push_u32(data, u32::try_from(units.len()).expect("unicode length"));
    for unit in units {
        push_u16(data, unit);
    }
}

/// Encode one descriptor body (no version).
pub fn descriptor_body(class_id: &[u8], items: &[Vec<u8>]) -> Vec<u8> {
    let mut data = Vec::new();
    push_unicode(&mut data, "");
    push_id(&mut data, class_id);
    push_u32(&mut data, u32::try_from(items.len()).expect("item count"));
    for item in items {
        data.extend_from_slice(item);
    }
    data
}

/// Encode one descriptor item: a key and a typed value.
pub fn descriptor_item(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    push_id(&mut data, key);
    data.extend_from_slice(value);
    data
}

/// Encode a `doub` value.
pub fn value_double(value: f64) -> Vec<u8> {
    let mut data = b"doub".to_vec();
    data.extend_from_slice(&value.to_be_bytes());
    data
}

/// Encode a `long` value.
pub fn value_integer(value: i32) -> Vec<u8> {
    let mut data = b"long".to_vec();
    data.extend_from_slice(&value.to_be_bytes());
    data
}

/// Encode an `UntF` value.
pub fn value_unit_float(unit: [u8; 4], value: f64) -> Vec<u8> {
    let mut data = b"UntF".to_vec();
    data.extend_from_slice(&unit);
    data.extend_from_slice(&value.to_be_bytes());
    data
}

/// Encode an `enum` value.
pub fn value_enum(type_id: &[u8], value: &[u8]) -> Vec<u8> {
    let mut data = b"enum".to_vec();
    push_id(&mut data, type_id);
    push_id(&mut data, value);
    data
}

/// Encode a `bool` value.
pub fn value_boolean(value: bool) -> Vec<u8> {
    let mut data = b"bool".to_vec();
    data.push(u8::from(value));
    data
}

/// Encode an `Objc` nested descriptor.
pub fn value_descriptor(body: &[u8]) -> Vec<u8> {
    let mut data = b"Objc".to_vec();
    data.extend_from_slice(body);
    data
}

/// Encode a `VlLs` list of already-typed values.
pub fn value_list(values: &[Vec<u8>]) -> Vec<u8> {
    let mut data = b"VlLs".to_vec();
    push_u32(&mut data, u32::try_from(values.len()).expect("list length"));
    for value in values {
        data.extend_from_slice(value);
    }
    data
}

/// Prefix a descriptor body with its version.
pub fn versioned(body: &[u8]) -> Vec<u8> {
    let mut data = 16u32.to_be_bytes().to_vec();
    data.extend_from_slice(body);
    data
}

/// Encode an `RGBC` color descriptor.
pub fn rgbc(red: f64, green: f64, blue: f64) -> Vec<u8> {
    descriptor_body(
        b"RGBC",
        &[
            descriptor_item(b"Rd  ", &value_double(red)),
            descriptor_item(b"Grn ", &value_double(green)),
            descriptor_item(b"Bl  ", &value_double(blue)),
        ],
    )
}

/// Build a `SoCo` solid-color block.
pub fn soco(red: f64, green: f64, blue: f64) -> Vec<u8> {
    versioned(&descriptor_body(
        b"null",
        &[descriptor_item(
            b"Clr ",
            &value_descriptor(&rgbc(red, green, blue)),
        )],
    ))
}

/// Build a `GdFl` linear gradient block from black to white at `angle` degrees.
pub fn gdfl_black_to_white(angle: f64) -> Vec<u8> {
    gdfl_black_to_white_midpoint(angle, 50)
}

/// [`gdfl_black_to_white`] with the ending stop's `Mdpn` set to
/// `ending_midpoint` percent. A midpoint is stored on the stop that ends its
/// segment.
pub fn gdfl_black_to_white_midpoint(angle: f64, ending_midpoint: i32) -> Vec<u8> {
    let stop = |location: i32, midpoint: i32, red: f64, green: f64, blue: f64| {
        value_descriptor(&descriptor_body(
            b"Clrt",
            &[
                descriptor_item(b"Lctn", &value_integer(location)),
                descriptor_item(b"Clr ", &value_descriptor(&rgbc(red, green, blue))),
                descriptor_item(b"Mdpn", &value_integer(midpoint)),
            ],
        ))
    };
    let grad = descriptor_body(
        b"Grdn",
        &[
            descriptor_item(
                b"Clrs",
                &value_list(&[
                    stop(0, 50, 0.0, 0.0, 0.0),
                    stop(4096, ending_midpoint, 255.0, 255.0, 255.0),
                ]),
            ),
            descriptor_item(b"Trns", &value_list(&[])),
        ],
    );
    versioned(&descriptor_body(
        b"null",
        &[
            descriptor_item(b"Angl", &value_unit_float(*b"#Ang", angle)),
            descriptor_item(b"Type", &value_enum(b"GrdT", b"Lnr ")),
            descriptor_item(b"Grad", &value_descriptor(&grad)),
            descriptor_item(b"Scl ", &value_unit_float(*b"#Prc", 100.0)),
        ],
    ))
}

/// Build one ten-byte levels record.
pub fn levels_record(
    input_black: u16,
    input_white: u16,
    output_black: u16,
    output_white: u16,
    gamma: u16,
) -> Vec<u8> {
    let mut data = Vec::new();
    for value in [input_black, input_white, output_black, output_white, gamma] {
        push_u16(&mut data, value);
    }
    data
}

/// Build a `levl` block: the master record then the R, G, B records.
pub fn levels_block(master: &[u8], red: &[u8], green: &[u8], blue: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, 2);
    for record in [master, red, green, blue] {
        data.extend_from_slice(record);
    }
    data
}

/// The identity levels record.
pub fn identity_levels() -> Vec<u8> {
    levels_record(0, 255, 0, 255, 100)
}

/// Build a `curv` block with one master curve.
pub fn curves_block(points: &[(u16, u16)]) -> Vec<u8> {
    let mut data = vec![0u8];
    push_u16(&mut data, 1);
    data.extend_from_slice(&1u32.to_be_bytes());
    push_u16(&mut data, u16::try_from(points.len()).expect("point count"));
    for (input, output) in points {
        push_u16(&mut data, *output);
        push_u16(&mut data, *input);
    }
    data
}

/// Build a `brit` legacy brightness/contrast block.
pub fn brit_block(brightness: i16, contrast: i16) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&brightness.to_be_bytes());
    data.extend_from_slice(&contrast.to_be_bytes());
    push_u16(&mut data, 127);
    data.push(0);
    data
}

// --------------------------------------------------------------------------
// Fixed-layout color adjustment blocks
// --------------------------------------------------------------------------

/// Push a big-endian `i16`.
fn push_i16(data: &mut Vec<u8>, value: i16) {
    data.extend_from_slice(&value.to_be_bytes());
}

/// Build an `expA` block.
pub fn expa_block(exposure: f32, offset: f32, gamma: f32) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, 1);
    for value in [exposure, offset, gamma] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    data
}

/// Build a `mixr` block from three records of `[red, green, blue, constant]`.
pub fn mixr_block(monochrome: bool, records: [[i16; 4]; 3]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, 1);
    push_u16(&mut data, u16::from(monochrome));
    for record in records.into_iter().chain([[0; 4]]) {
        for value in &record[..3] {
            push_i16(&mut data, *value);
        }
        // The fourth source, used only by CMYK, then the constant.
        push_i16(&mut data, 0);
        push_i16(&mut data, record[3]);
    }
    data
}

/// Build a `blnc` block: shadows, midtones and highlights.
pub fn blnc_block(tones: [[i16; 3]; 3], preserve_luminosity: bool) -> Vec<u8> {
    let mut data = Vec::new();
    for value in tones.iter().flatten() {
        push_i16(&mut data, *value);
    }
    data.push(u8::from(preserve_luminosity));
    data.push(0);
    data
}

/// Build a `hue2` block with the default range extents.
///
/// `colorize` and `master` are `[hue, saturation, lightness]`. Each range edit
/// is `[hue, saturation, lightness]`: reds, yellows, greens, cyans, blues,
/// magentas.
pub fn hue2_block(colorize: Option<[i16; 3]>, master: [i16; 3], edits: [[i16; 3]; 6]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, 2);
    data.push(u8::from(colorize.is_some()));
    data.push(0);
    for value in colorize.unwrap_or([0; 3]).into_iter().chain(master) {
        push_i16(&mut data, value);
    }
    for (index, edit) in edits.iter().enumerate() {
        let centre = i16::try_from(index * 60).expect("centre");
        for offset in [-45i16, -15, 15, 45] {
            push_i16(&mut data, (centre + offset).rem_euclid(360));
        }
        for value in edit {
            push_i16(&mut data, *value);
        }
    }
    data
}

/// Build a `selc` block. `records` are reds, yellows, greens, cyans, blues,
/// magentas, whites, neutrals, blacks, each `[cyan, magenta, yellow, black]`.
pub fn selc_block(absolute: bool, records: [[i16; 4]; 9]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, 1);
    push_u16(&mut data, u16::from(absolute));
    data.extend_from_slice(&[0; 8]);
    for value in records.iter().flatten() {
        push_i16(&mut data, *value);
    }
    data
}

/// Build a version 2 `phfl` block with an RGB filter color (8-bit values).
pub fn phfl_block(rgb: [u8; 3], density_percent: u32, preserve_luminosity: bool) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, 2);
    push_u16(&mut data, 0);
    for value in rgb {
        push_u16(&mut data, u16::from(value) * 257);
    }
    push_u16(&mut data, 0);
    push_u32(&mut data, density_percent);
    data.push(u8::from(preserve_luminosity));
    data.extend_from_slice(&[0; 3]);
    data
}

/// Encode a raw-bytes value (`tdta`): a length, then the bytes.
pub fn value_raw(kind: [u8; 4], bytes: &[u8]) -> Vec<u8> {
    let mut data = kind.to_vec();
    push_u32(&mut data, u32::try_from(bytes.len()).expect("raw length"));
    data.extend_from_slice(bytes);
    data
}

/// Build a `clrL` block. `format` is the `LUTFormat` id; `file` is the `LUT3DFileData` value.
pub fn clrl_block(lookup_type: &[u8], format: &[u8], file: Option<&[u8]>, dither: bool) -> Vec<u8> {
    let mut items = vec![
        descriptor_item(b"lookupType", &value_enum(b"colorLookupType", lookup_type)),
        descriptor_item(b"Dthr", &value_boolean(dither)),
        descriptor_item(b"LUTFormat", &value_enum(b"LUTFormatType", format)),
        descriptor_item(b"dataOrder", &value_enum(b"colorLookupOrder", b"rgbOrder")),
        descriptor_item(b"tableOrder", &value_enum(b"colorLookupOrder", b"bgrOrder")),
    ];
    if let Some(bytes) = file {
        items.push(descriptor_item(
            b"LUT3DFileData",
            &value_raw(*b"tdta", bytes),
        ));
    }
    let mut data = Vec::new();
    push_u16(&mut data, 1);
    data.extend_from_slice(&versioned(&descriptor_body(b"null", &items)));
    data
}
