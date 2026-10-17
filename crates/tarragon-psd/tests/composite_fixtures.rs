//! PSD and PSB compositor behavior.
//!
//! The fixture tests read the committed corpus in `tests/fixtures/psd/`. The
//! synthetic tests build small PSD files in memory to cover the features the
//! corpus lacks: hidden layers, fill opacity, 8/16-bit parity, and the
//! pass-through versus isolated group difference.
//!
//! These tests use the generator's known pixel formulas and analytic blend
//! math.

use std::path::{Path, PathBuf};

use tarragon_psd::blend::BlendMode;
use tarragon_psd::composite::CompositeRegion;
use tarragon_psd::{ParseOptions, PsdDocument, Viewport};

/// The absolute path of a committed fixture.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/psd")
        .join(name)
}

/// Composite a fixture over its full canvas.
fn composite_fixture(name: &str) -> CompositeRegion {
    let document = PsdDocument::open(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let viewport = Viewport::full(document.file().header.width, document.file().header.height);
    document
        .composite_region(viewport, &ParseOptions::none())
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Composite owned PSD bytes over the full canvas.
fn composite_bytes(bytes: Vec<u8>) -> CompositeRegion {
    let document =
        PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("the synthetic file parses");
    let viewport = Viewport::full(document.file().header.width, document.file().header.height);
    document
        .composite_region(viewport, &ParseOptions::none())
        .expect("the synthetic file composites")
}

// --------------------------------------------------------------------------
// Synthetic PSD builder
// --------------------------------------------------------------------------

/// A layer mask block for the synthetic builder.
struct MaskSpec {
    /// The mask rectangle: top, left, bottom, right.
    rect: (i32, i32, i32, i32),
    /// The default color, 0 or 255.
    default_color: u8,
    /// The mask flag bits.
    flags: u8,
}

/// One layer in a synthetic document.
struct LayerSpec {
    /// The layer rectangle: top, left, bottom, right.
    rect: (i32, i32, i32, i32),
    /// The channel samples: `(id, width * height samples)`.
    channels: Vec<(i16, Vec<u16>)>,
    /// The four blend-key bytes.
    blend: [u8; 4],
    /// The layer opacity.
    opacity: u8,
    /// The fill opacity block value, when present.
    fill_opacity: Option<u8>,
    /// The raw layer flag bits.
    flags: u8,
    /// The clipping flag.
    clipping: u8,
    /// The section divider code, when the record opens or closes a group.
    section: Option<u32>,
    /// Additional tagged blocks, in record order.
    blocks: Vec<([u8; 4], Vec<u8>)>,
    /// The layer mask block, when present.
    mask: Option<MaskSpec>,
}

impl LayerSpec {
    /// An opaque pixel layer with color channels and no alpha.
    fn opaque(rect: (i32, i32, i32, i32), values: [u16; 3]) -> Self {
        let width = usize::try_from(rect.3 - rect.1).expect("width");
        let height = usize::try_from(rect.2 - rect.0).expect("height");
        let channels = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                (
                    i16::try_from(index).expect("channel id"),
                    vec![*value; width * height],
                )
            })
            .collect();
        Self {
            rect,
            channels,
            blend: *b"norm",
            opacity: 255,
            fill_opacity: None,
            flags: 0,
            clipping: 0,
            section: None,
            blocks: Vec::new(),
            mask: None,
        }
    }

    /// A pixel layer with a constant alpha channel.
    fn with_alpha(rect: (i32, i32, i32, i32), values: [u16; 4]) -> Self {
        let width = usize::try_from(rect.3 - rect.1).expect("width");
        let height = usize::try_from(rect.2 - rect.0).expect("height");
        let channels = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let id = if index < 3 {
                    i16::try_from(index).expect("channel id")
                } else {
                    -1
                };
                (id, vec![*value; width * height])
            })
            .collect();
        Self {
            rect,
            channels,
            blend: *b"norm",
            opacity: 255,
            fill_opacity: None,
            flags: 0,
            clipping: 0,
            section: None,
            blocks: Vec::new(),
            mask: None,
        }
    }

    /// A color-only layer that carries no channel data, for a fill or adjustment.
    fn content(rect: (i32, i32, i32, i32)) -> Self {
        Self {
            rect,
            channels: Vec::new(),
            blend: *b"norm",
            opacity: 255,
            fill_opacity: None,
            flags: 0,
            clipping: 0,
            section: None,
            blocks: Vec::new(),
            mask: None,
        }
    }

    /// Attach one tagged block.
    fn with_block(mut self, key: [u8; 4], data: Vec<u8>) -> Self {
        self.blocks.push((key, data));
        self
    }

    /// Attach a layer mask.
    fn with_mask(mut self, rect: (i32, i32, i32, i32), default_color: u8) -> Self {
        self.mask = Some(MaskSpec {
            rect,
            default_color,
            flags: 0,
        });
        self
    }

    /// An empty group marker record.
    fn group_marker(section: u32, blend: [u8; 4]) -> Self {
        Self {
            rect: (0, 0, 0, 0),
            channels: Vec::new(),
            blend,
            opacity: 255,
            fill_opacity: None,
            flags: 0,
            clipping: 0,
            section: Some(section),
            blocks: Vec::new(),
            mask: None,
        }
    }
}

/// A synthetic PSD document.
struct DocumentSpec {
    /// The canvas width.
    width: u32,
    /// The canvas height.
    height: u32,
    /// The bit depth, 8 or 16.
    depth: u16,
    /// The color mode code: 3 is RGB, 4 is CMYK.
    color_mode: u16,
    /// The layers, bottom to top.
    layers: Vec<LayerSpec>,
    /// The merged image planes. Empty means zeros.
    merged: Vec<Vec<u16>>,
}

impl DocumentSpec {
    /// A new RGB or CMYK document with no layers.
    fn new(width: u32, height: u32, depth: u16, color_mode: u16) -> Self {
        Self {
            width,
            height,
            depth,
            color_mode,
            layers: Vec::new(),
            merged: Vec::new(),
        }
    }
}

/// Push a big-endian `u16`.
fn push_u16(data: &mut Vec<u8>, value: u16) {
    data.extend_from_slice(&value.to_be_bytes());
}

/// Push a big-endian `u32`.
fn push_u32(data: &mut Vec<u8>, value: u32) {
    data.extend_from_slice(&value.to_be_bytes());
}

/// Push a big-endian `i32`.
fn push_i32(data: &mut Vec<u8>, value: i32) {
    data.extend_from_slice(&value.to_be_bytes());
}

/// The bytes per sample for a depth.
fn sample_bytes(depth: u16) -> usize {
    if depth == 8 { 1 } else { 2 }
}

/// Append samples in the file byte order for a depth.
fn push_samples(data: &mut Vec<u8>, samples: &[u16], depth: u16) {
    if depth == 8 {
        for sample in samples {
            data.push(u8::try_from(*sample).expect("8-bit sample"));
        }
    } else {
        for sample in samples {
            push_u16(data, *sample);
        }
    }
}

/// Build the extra-data block of one layer record.
fn layer_extra(spec: &LayerSpec, name: &str) -> Vec<u8> {
    let mut extra = Vec::new();
    // The mask data: a length then the mask body, or an empty mask.
    if let Some(mask) = &spec.mask {
        push_u32(&mut extra, 18);
        push_i32(&mut extra, mask.rect.0);
        push_i32(&mut extra, mask.rect.1);
        push_i32(&mut extra, mask.rect.2);
        push_i32(&mut extra, mask.rect.3);
        extra.push(mask.default_color);
        extra.push(mask.flags);
    } else {
        push_u32(&mut extra, 0);
    }
    // No blending ranges.
    push_u32(&mut extra, 0);
    // A padded Pascal name.
    let name_bytes = name.as_bytes();
    extra.push(u8::try_from(name_bytes.len()).expect("name length"));
    extra.extend_from_slice(name_bytes);
    let consumed = name_bytes.len() + 1;
    let pad = (4 - (consumed % 4)) % 4;
    extra.extend(std::iter::repeat_n(0u8, pad));
    // The fill opacity block.
    if let Some(value) = spec.fill_opacity {
        extra.extend_from_slice(b"8BIM");
        extra.extend_from_slice(b"iOpa");
        push_u32(&mut extra, 1);
        extra.push(value);
    }
    // The parsed tagged blocks (fill descriptors, adjustment blocks, masks).
    for (key, data) in &spec.blocks {
        extra.extend_from_slice(b"8BIM");
        extra.extend_from_slice(key);
        push_u32(&mut extra, u32::try_from(data.len()).expect("block length"));
        extra.extend_from_slice(data);
    }
    extra
}

/// Build one layer record with its section divider inside the extra data.
fn layer_record(spec: &LayerSpec, depth: u16, name: &str) -> Vec<u8> {
    let (top, left, bottom, right) = spec.rect;
    let width = usize::try_from(right - left).expect("width");
    let height = usize::try_from(bottom - top).expect("height");
    let bytes = sample_bytes(depth);
    let mut record = Vec::new();
    push_i32(&mut record, top);
    push_i32(&mut record, left);
    push_i32(&mut record, bottom);
    push_i32(&mut record, right);
    push_u16(
        &mut record,
        u16::try_from(spec.channels.len()).expect("channel count"),
    );
    for (id, _) in &spec.channels {
        record.extend_from_slice(&id.to_be_bytes());
        let data_length = width * height * bytes;
        push_u32(
            &mut record,
            u32::try_from(data_length + 2).expect("channel length"),
        );
    }
    record.extend_from_slice(b"8BIM");
    record.extend_from_slice(&spec.blend);
    record.push(spec.opacity);
    record.push(spec.clipping);
    record.push(spec.flags);
    record.push(0);
    let mut extra = layer_extra(spec, name);
    if let Some(section) = spec.section {
        extra.extend_from_slice(b"8BIM");
        extra.extend_from_slice(b"lsct");
        push_u32(&mut extra, 4);
        push_u32(&mut extra, section);
    }
    push_u32(
        &mut record,
        u32::try_from(extra.len()).expect("extra length"),
    );
    record.extend_from_slice(&extra);
    record
}

/// Build a complete synthetic PSD file.
fn build_psd(spec: &DocumentSpec) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"8BPS");
    push_u16(&mut data, 1);
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let merged_channels: u16 = if spec.color_mode == 4 { 4 } else { 3 };
    push_u16(&mut data, merged_channels);
    push_u32(&mut data, spec.height);
    push_u32(&mut data, spec.width);
    push_u16(&mut data, spec.depth);
    push_u16(&mut data, spec.color_mode);
    // Empty color-mode data and image resources.
    push_u32(&mut data, 0);
    push_u32(&mut data, 0);

    let mut info = Vec::new();
    push_u16(
        &mut info,
        u16::try_from(spec.layers.len()).expect("layer count"),
    );
    for (index, layer) in spec.layers.iter().enumerate() {
        info.extend_from_slice(&layer_record(layer, spec.depth, &format!("Layer {index}")));
    }
    // The channel data blocks follow the records, in record order.
    for layer in &spec.layers {
        for (_, samples) in &layer.channels {
            push_u16(&mut info, 0);
            push_samples(&mut info, samples, spec.depth);
        }
    }

    // The layer-and-mask section.
    let mut section = Vec::new();
    push_u32(
        &mut section,
        u32::try_from(info.len()).expect("layer info length"),
    );
    section.extend_from_slice(&info);
    push_u32(&mut section, 0);
    push_u32(
        &mut data,
        u32::try_from(section.len()).expect("section length"),
    );
    data.extend_from_slice(&section);

    // A raw merged image.
    push_u16(&mut data, 0);
    if spec.merged.is_empty() {
        let plane = usize::try_from(spec.width).expect("width")
            * usize::try_from(spec.height).expect("height")
            * sample_bytes(spec.depth);
        data.extend(std::iter::repeat_n(
            0u8,
            plane * usize::from(merged_channels),
        ));
    } else {
        for plane in &spec.merged {
            push_samples(&mut data, plane, spec.depth);
        }
    }
    data
}

/// The 8-bit or 16-bit sample for an 8-bit value.
fn scaled(value: u16, depth: u16) -> u16 {
    if depth == 8 { value } else { value * 257 }
}

/// Read one output pixel as RGBA8.
fn sample_rgba8(region: &CompositeRegion, x: u32, y: u32) -> [u8; 4] {
    let pixels = region.rgba8();
    let index = usize::try_from(y).expect("y") * usize::try_from(region.width()).expect("width")
        + usize::try_from(x).expect("x");
    [
        pixels[index * 4],
        pixels[index * 4 + 1],
        pixels[index * 4 + 2],
        pixels[index * 4 + 3],
    ]
}

// --------------------------------------------------------------------------
// Descriptor and block encoders
// --------------------------------------------------------------------------

/// Push a four-byte code or a length-prefixed id.
fn push_id(data: &mut Vec<u8>, id: &[u8]) {
    if id.len() == 4 {
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(id);
    } else {
        push_u32(data, u32::try_from(id.len()).expect("id length"));
        data.extend_from_slice(id);
    }
}

/// Push a unicode string: a count then UTF-16BE units.
fn push_unicode(data: &mut Vec<u8>, value: &str) {
    let units: Vec<u16> = value.encode_utf16().collect();
    push_u32(data, u32::try_from(units.len()).expect("unicode length"));
    for unit in units {
        push_u16(data, unit);
    }
}

/// Encode one descriptor body (no version).
fn descriptor_body(class_id: &[u8], items: &[Vec<u8>]) -> Vec<u8> {
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
fn descriptor_item(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    push_id(&mut data, key);
    data.extend_from_slice(value);
    data
}

/// Encode a `doub` value.
fn value_double(value: f64) -> Vec<u8> {
    let mut data = b"doub".to_vec();
    data.extend_from_slice(&value.to_be_bytes());
    data
}

/// Encode a `long` value.
fn value_integer(value: i32) -> Vec<u8> {
    let mut data = b"long".to_vec();
    data.extend_from_slice(&value.to_be_bytes());
    data
}

/// Encode an `UntF` value.
fn value_unit_float(unit: [u8; 4], value: f64) -> Vec<u8> {
    let mut data = b"UntF".to_vec();
    data.extend_from_slice(&unit);
    data.extend_from_slice(&value.to_be_bytes());
    data
}

/// Encode an `enum` value.
fn value_enum(type_id: &[u8], value: &[u8]) -> Vec<u8> {
    let mut data = b"enum".to_vec();
    push_id(&mut data, type_id);
    push_id(&mut data, value);
    data
}

/// Encode a `bool` value.
fn value_boolean(value: bool) -> Vec<u8> {
    let mut data = b"bool".to_vec();
    data.push(u8::from(value));
    data
}

/// Encode an `Objc` nested descriptor.
fn value_descriptor(body: &[u8]) -> Vec<u8> {
    let mut data = b"Objc".to_vec();
    data.extend_from_slice(body);
    data
}

/// Encode a `VlLs` list of already-typed values.
fn value_list(values: &[Vec<u8>]) -> Vec<u8> {
    let mut data = b"VlLs".to_vec();
    push_u32(&mut data, u32::try_from(values.len()).expect("list length"));
    for value in values {
        data.extend_from_slice(value);
    }
    data
}

/// Prefix a descriptor body with its version.
fn versioned(body: &[u8]) -> Vec<u8> {
    let mut data = 16u32.to_be_bytes().to_vec();
    data.extend_from_slice(body);
    data
}

/// Encode an `RGBC` color descriptor.
fn rgbc(red: f64, green: f64, blue: f64) -> Vec<u8> {
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
fn soco(red: f64, green: f64, blue: f64) -> Vec<u8> {
    versioned(&descriptor_body(
        b"null",
        &[descriptor_item(
            b"Clr ",
            &value_descriptor(&rgbc(red, green, blue)),
        )],
    ))
}

/// Build a `GdFl` linear gradient block from black to white at `angle` degrees.
fn gdfl_black_to_white(angle: f64) -> Vec<u8> {
    gdfl_black_to_white_midpoint(angle, 50)
}

/// [`gdfl_black_to_white`] with the ending stop's `Mdpn` set to
/// `ending_midpoint` percent. A midpoint is stored on the stop that ends its
/// segment.
fn gdfl_black_to_white_midpoint(angle: f64, ending_midpoint: i32) -> Vec<u8> {
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
fn levels_record(
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
fn levels_block(master: &[u8], red: &[u8], green: &[u8], blue: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u16(&mut data, 2);
    for record in [master, red, green, blue] {
        data.extend_from_slice(record);
    }
    data
}

/// The identity levels record.
fn identity_levels() -> Vec<u8> {
    levels_record(0, 255, 0, 255, 100)
}

/// Build a `curv` block with one master curve.
fn curves_block(points: &[(u16, u16)]) -> Vec<u8> {
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
fn brit_block(brightness: i16, contrast: i16) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&brightness.to_be_bytes());
    data.extend_from_slice(&contrast.to_be_bytes());
    push_u16(&mut data, 127);
    data.push(0);
    data
}

// --------------------------------------------------------------------------
// Fixture tests
// --------------------------------------------------------------------------

#[test]
fn the_layer_groups_fixture_composites_its_pass_through_children() {
    // Background (30,30,30), then a pass-through group: Child A (200,80,40)
    // and Child B (40,120,200). Child B covers Child A.
    let region = composite_fixture("layer_groups.psd");
    assert_eq!(region.width(), 96);
    assert_eq!(sample_rgba8(&region, 0, 0), [40, 120, 200, 255]);
    assert_eq!(sample_rgba8(&region, 95, 95), [40, 120, 200, 255]);
}

#[test]
fn the_clipping_fixture_restricts_to_the_base() {
    // Base (200,200,200); Clip A (220,60,60) full canvas; Clip B (60,60,220)
    // only in the top-left 48x48. Both clip to the base alpha.
    let region = composite_fixture("clipping_mask.psd");
    assert_eq!(sample_rgba8(&region, 10, 10), [60, 60, 220, 255]);
    assert_eq!(sample_rgba8(&region, 80, 80), [220, 60, 60, 255]);
    // The clip boundary is at x = 48.
    assert_eq!(sample_rgba8(&region, 47, 10), [60, 60, 220, 255]);
    assert_eq!(sample_rgba8(&region, 48, 10), [220, 60, 60, 255]);
}

#[test]
fn the_layer_mask_fixture_cuts_the_layer_alpha() {
    // The mask channel is ((x*4) & 0xFF) / 255. The layer color is
    // (120,180,90) with alpha 255, so the mask scales the output alpha.
    let region = composite_fixture("layer_mask.psd");
    let left = region.alpha(0, 0).expect("alpha");
    assert!(left.abs() < 1e-6, "the mask is black at x = 0");
    let middle = region.alpha(16, 0).expect("alpha");
    assert!((middle - 64.0 / 255.0).abs() < 1e-6, "mask at x = 16");
    let right = region.alpha(63, 0).expect("alpha");
    assert!((right - 252.0 / 255.0).abs() < 1e-6, "mask at x = 63");
    // The masked color is the layer color where the mask is not black.
    let color = region.color(16, 0).expect("color");
    assert!((color[0] - 120.0 / 255.0).abs() < 1e-6);
    assert!((color[1] - 180.0 / 255.0).abs() < 1e-6);
    assert!((color[2] - 90.0 / 255.0).abs() < 1e-6);
}

#[test]
fn the_cmyk_fixture_decodes_to_rgb() {
    // The layer samples are CMYK (200,40,120,20). The naive conversion is
    // R = (1-C)*(1-K), G = (1-M)*(1-K), B = (1-Y)*(1-K).
    let region = composite_fixture("cmyk_8bit.psd");
    assert_eq!(sample_rgba8(&region, 0, 0), [50, 198, 124, 255]);
    assert_eq!(sample_rgba8(&region, 63, 63), [50, 198, 124, 255]);
}

// --------------------------------------------------------------------------
// Synthetic tests
// --------------------------------------------------------------------------

#[test]
fn a_normal_layer_composites_over_an_opaque_background() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::with_alpha((0, 0, 2, 2), [200, 50, 10, 128]);
    top.blend = *b"norm";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    // a = 128/255. R = 200a + 100(1-a) = 150.2, G = 99.8, B = 104.6. The
    // union of a full backdrop and a partial source rounds one step low.
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[150, 99, 104]);
    assert!(pixel[3] >= 254, "opaque alpha, got {}", pixel[3]);
}

#[test]
fn a_multiply_layer_uses_the_multiply_formula() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [200, 100, 50]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [100, 200, 25]);
    top.blend = *b"mul ";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    // 200*100/255 = 78.4, 100*200/255 = 78.4, 50*25/255 = 4.9.
    assert_eq!(sample_rgba8(&region, 0, 0), [78, 78, 4, 255]);
}

#[test]
fn a_screen_layer_uses_the_screen_formula() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [50, 60, 70]);
    top.blend = *b"scrn";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [130, 174, 215, 255]);
}

#[test]
fn an_overlay_layer_uses_the_overlay_formula() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [50, 60, 220]);
    top.blend = *b"over";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [39, 94, 239, 255]);
}

#[test]
fn a_linear_dodge_layer_saturates_the_channels() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [200, 100, 55]);
    top.blend = *b"lddg";
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 250, 255, 255]);
}

#[test]
fn a_layer_opacity_scales_the_source_alpha() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [200, 0, 0]);
    top.opacity = 128;
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[150, 49, 49]);
    assert!(pixel[3] >= 254, "opaque alpha, got {}", pixel[3]);
}

#[test]
fn a_fill_opacity_scales_the_source_alpha() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [200, 0, 0]);
    top.fill_opacity = Some(128);
    spec.layers.push(top);
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[150, 49, 49]);
    assert!(pixel[3] >= 254, "opaque alpha, got {}", pixel[3]);
}

#[test]
fn a_hidden_layer_changes_nothing() {
    let mut visible = DocumentSpec::new(2, 2, 8, 3);
    visible
        .layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let baseline = composite_bytes(build_psd(&visible));

    let mut hidden = DocumentSpec::new(2, 2, 8, 3);
    hidden
        .layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 150, 200]));
    let mut top = LayerSpec::opaque((0, 0, 2, 2), [200, 50, 10]);
    top.flags = 0x02;
    hidden.layers.push(top);
    let with_hidden = composite_bytes(build_psd(&hidden));

    assert_eq!(with_hidden.rgba8(), baseline.rgba8());
    assert_eq!(sample_rgba8(&with_hidden, 0, 0), [100, 150, 200, 255]);
}

#[test]
fn a_pass_through_group_blends_onto_the_backdrop() {
    // Background (100,100,100) and a pass-through group with one multiply
    // child (50,50,50). The child multiplies with the backdrop.
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    spec.layers.push(LayerSpec::group_marker(3, *b"pass"));
    let mut child = LayerSpec::opaque((0, 0, 2, 2), [50, 50, 50]);
    child.blend = *b"mul ";
    spec.layers.push(child);
    spec.layers.push(LayerSpec::group_marker(1, *b"pass"));
    let region = composite_bytes(build_psd(&spec));
    // 100*50/255 = 19.6, truncated to 19.
    assert_eq!(sample_rgba8(&region, 0, 0), [19, 19, 19, 255]);
}

#[test]
fn an_isolated_group_blends_onto_a_transparent_buffer() {
    // The same layers, but the group is normal (isolated). The multiply child
    // sees a transparent buffer, so it keeps its own color.
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    spec.layers.push(LayerSpec::group_marker(3, *b"norm"));
    let mut child = LayerSpec::opaque((0, 0, 2, 2), [50, 50, 50]);
    child.blend = *b"mul ";
    spec.layers.push(child);
    spec.layers.push(LayerSpec::group_marker(1, *b"norm"));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [50, 50, 50, 255]);
}

#[test]
fn a_clipping_layer_is_restricted_to_the_base_alpha() {
    // Base: opaque color with alpha 128 on the left pixel and 0 on the right.
    // The clip layer covers both pixels, but the right pixel must stay clear.
    let mut spec = DocumentSpec::new(2, 1, 8, 3);
    let mut base = LayerSpec::opaque((0, 0, 1, 2), [200, 200, 200]);
    base.channels.push((-1, vec![128, 0]));
    spec.layers.push(base);
    let mut clip = LayerSpec::opaque((0, 0, 1, 2), [0, 0, 255]);
    clip.clipping = 1;
    spec.layers.push(clip);
    let region = composite_bytes(build_psd(&spec));
    let left = region.alpha(0, 0).expect("left alpha");
    assert!(left > 0.4, "the base alpha shows through: {left}");
    let right = region.alpha(1, 0).expect("right alpha");
    assert!(right.abs() < 1e-6, "the clip is cut by the base: {right}");
}

#[test]
fn an_eight_bit_and_a_sixteen_bit_image_agree() {
    let mut eight = DocumentSpec::new(2, 1, 8, 3);
    eight
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 2), [100, 150, 200]));
    let mut eight_top = LayerSpec::with_alpha((0, 0, 1, 2), [200, 60, 30, 128]);
    eight_top.blend = *b"mul ";
    eight.layers.push(eight_top);

    let mut sixteen = DocumentSpec::new(2, 1, 16, 3);
    let background = [scaled(100, 16), scaled(150, 16), scaled(200, 16)];
    sixteen
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 2), background));
    let top = [
        scaled(200, 16),
        scaled(60, 16),
        scaled(30, 16),
        scaled(128, 16),
    ];
    let mut sixteen_top = LayerSpec::with_alpha((0, 0, 1, 2), top);
    sixteen_top.blend = *b"mul ";
    sixteen.layers.push(sixteen_top);

    let region8 = composite_bytes(build_psd(&eight));
    let region16 = composite_bytes(build_psd(&sixteen));
    assert_eq!(region8.rgba8(), region16.rgba8());
}

#[test]
fn a_flattened_document_uses_the_merged_background() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    // A merged gradient: R = x, G = y, B = 10. There are no layers.
    spec.merged = vec![vec![0, 1, 0, 1], vec![0, 0, 1, 1], vec![10, 10, 10, 10]];
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [0, 0, 10, 255]);
    assert_eq!(sample_rgba8(&region, 1, 1), [1, 1, 10, 255]);
}

#[test]
fn a_blend_mode_unit_covers_every_named_mode() {
    // The fixture corpus covers a few modes. This checks that the public blend
    // entry returns a value for every mode without panicking.
    let modes = [
        BlendMode::PassThrough,
        BlendMode::Normal,
        BlendMode::Dissolve,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::LinearDodge,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];
    for mode in modes {
        let out = tarragon_psd::blend::blend_rgb(mode, [0.4, 0.5, 0.6], [0.6, 0.3, 0.2]);
        for channel in out {
            assert!((0.0..=1.0).contains(&channel), "{mode:?}");
        }
    }
}

#[test]
fn vivid_light_lets_a_source_extreme_win_over_the_backdrop() {
    // A black source over a white backdrop is black, not white.
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [255, 255, 255]));
    let mut source = LayerSpec::opaque((0, 0, 1, 1), [0, 0, 0]);
    source.blend = *b"vLit";
    spec.layers.push(source);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [0, 0, 0, 255]);
}

#[test]
fn hard_mix_keeps_its_photoshop_corner_cases() {
    // A white backdrop under a black source stays white for Hard Mix.
    let mut light = DocumentSpec::new(1, 1, 8, 3);
    light
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [255, 255, 255]));
    let mut dark_source = LayerSpec::opaque((0, 0, 1, 1), [0, 0, 0]);
    dark_source.blend = *b"hMix";
    light.layers.push(dark_source);
    let region = composite_bytes(build_psd(&light));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 255, 255, 255]);

    // A black backdrop under a white source stays black.
    let mut dark = DocumentSpec::new(1, 1, 8, 3);
    dark.layers.push(LayerSpec::opaque((0, 0, 1, 1), [0, 0, 0]));
    let mut light_source = LayerSpec::opaque((0, 0, 1, 1), [255, 255, 255]);
    light_source.blend = *b"hMix";
    dark.layers.push(light_source);
    let region = composite_bytes(build_psd(&dark));
    assert_eq!(sample_rgba8(&region, 0, 0), [0, 0, 0, 255]);
}

#[test]
fn a_solid_color_fill_layer_paints_its_color() {
    let mut spec = DocumentSpec::new(2, 2, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 2, 2), [100, 100, 100]));
    spec.layers
        .push(LayerSpec::content((0, 0, 2, 2)).with_block(*b"SoCo", soco(255.0, 0.0, 0.0)));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 0, 0, 255]);
    assert_eq!(region.alpha(1, 1).expect("alpha"), 1.0);
}

#[test]
fn a_solid_fill_layer_honors_its_opacity() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers.push(LayerSpec::opaque((0, 0, 1, 1), [0, 0, 0]));
    let mut fill = LayerSpec::content((0, 0, 1, 1)).with_block(*b"SoCo", soco(255.0, 255.0, 255.0));
    fill.opacity = 128;
    spec.layers.push(fill);
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    assert_eq!(&pixel[..3], &[128, 128, 128]);
    assert!(pixel[3] >= 254, "{pixel:?}");
}

#[test]
fn a_gradient_fill_layer_ramps_across_the_canvas() {
    let mut spec = DocumentSpec::new(16, 1, 8, 3);
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 16)).with_block(*b"GdFl", gdfl_black_to_white(0.0)));
    let region = composite_bytes(build_psd(&spec));
    let left = sample_rgba8(&region, 0, 0);
    let middle = sample_rgba8(&region, 8, 0);
    let right = sample_rgba8(&region, 15, 0);
    assert!(
        left[0] < middle[0] && middle[0] < right[0],
        "{left:?} {middle:?} {right:?}"
    );
    assert!(left[0] < 10, "{left:?}");
    assert!(right[0] > 200, "{right:?}");
    assert_eq!(left[3], 255);
}

#[test]
fn a_gradient_midpoint_moves_the_halfway_color() {
    let mut spec = DocumentSpec::new(16, 1, 8, 3);
    spec.layers.push(
        LayerSpec::content((0, 0, 1, 16))
            .with_block(*b"GdFl", gdfl_black_to_white_midpoint(0.0, 25)),
    );
    let region = composite_bytes(build_psd(&spec));
    // The ending stop carries Mdpn 25, so the ramp is half-way at t = 0.25
    // (x = 4). A midpoint is stored on the stop that ends its segment.
    let early = sample_rgba8(&region, 4, 0);
    assert!((i32::from(early[0]) - 128).abs() <= 2, "{early:?}");
    // At t = 0.5 (x = 8) the ramp then sits two-thirds of the way up the segment.
    let middle = sample_rgba8(&region, 8, 0);
    assert!((i32::from(middle[0]) - 170).abs() <= 2, "{middle:?}");
}

#[test]
fn a_fill_layer_mask_cuts_its_coverage() {
    // A white solid fill with a mask channel that is black over the second pixel.
    let mut spec = DocumentSpec::new(2, 1, 8, 3);
    let mut fill = LayerSpec::content((0, 0, 1, 2))
        .with_block(*b"SoCo", soco(255.0, 255.0, 255.0))
        .with_mask((0, 0, 1, 2), 255);
    fill.channels = vec![(-2, vec![255, 0])];
    spec.layers.push(fill);
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 255, 255, 255]);
    assert!(region.alpha(1, 0).expect("alpha").abs() < 1e-6);
}

#[test]
fn an_invert_adjustment_changes_the_backdrop() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [100, 150, 200]));
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"nvrt", Vec::new()));
    let region = composite_bytes(build_psd(&spec));
    let color = region.color(0, 0).expect("color");
    // The complement of (100, 150, 200) over 255, before truncation.
    assert!((color[0] - 155.0 / 255.0).abs() < 1e-4, "{color:?}");
    assert!((color[1] - 105.0 / 255.0).abs() < 1e-4, "{color:?}");
    assert!((color[2] - 55.0 / 255.0).abs() < 1e-4, "{color:?}");
}

#[test]
fn an_invert_adjustment_honors_its_opacity() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [100, 150, 200]));
    let mut adjustment = LayerSpec::content((0, 0, 1, 1)).with_block(*b"nvrt", Vec::new());
    adjustment.opacity = 128;
    spec.layers.push(adjustment);
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    // Halfway between 100 and 155, and so on.
    assert!((i32::from(pixel[0]) - 127).abs() <= 1, "{pixel:?}");
    assert!((i32::from(pixel[1]) - 127).abs() <= 1, "{pixel:?}");
    assert!((i32::from(pixel[2]) - 127).abs() <= 1, "{pixel:?}");
}

#[test]
fn a_levels_adjustment_maps_the_midtone() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    let master = levels_record(0, 255, 0, 128, 100);
    let block = levels_block(
        &master,
        &identity_levels(),
        &identity_levels(),
        &identity_levels(),
    );
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"levl", block));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [64, 64, 64, 255]);
}

#[test]
fn a_curves_adjustment_maps_the_midtone() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    let block = curves_block(&[(0, 0), (128, 192), (255, 255)]);
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"curv", block));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [192, 192, 192, 255]);
}

#[test]
fn a_brightness_contrast_adjustment_uses_the_legacy_formula() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"brit", brit_block(40, 0)));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [168, 168, 168, 255]);
}

#[test]
fn a_modern_brightness_contrast_descriptor_is_used() {
    // The CgEd descriptor takes precedence over the legacy `brit` pair.
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    let cged = versioned(&descriptor_body(
        b"null",
        &[
            descriptor_item(b"Brgh", &value_integer(40)),
            descriptor_item(b"Cntr", &value_integer(0)),
            descriptor_item(b"useLegacy", &value_boolean(false)),
        ],
    ));
    spec.layers.push(
        LayerSpec::content((0, 0, 1, 1))
            .with_block(*b"brit", brit_block(40, 0))
            .with_block(*b"CgEd", cged),
    );
    let region = composite_bytes(build_psd(&spec));
    let pixel = sample_rgba8(&region, 0, 0);
    assert!(pixel[0] < 165 && pixel[0] > 150, "{pixel:?}");
}

#[test]
fn a_posterize_adjustment_quantizes_the_midtone() {
    let mut spec = DocumentSpec::new(1, 1, 8, 3);
    spec.layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [128, 128, 128]));
    let mut block = Vec::new();
    push_u16(&mut block, 4);
    push_u16(&mut block, 0);
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"post", block));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [170, 170, 170, 255]);
}

#[test]
fn a_threshold_adjustment_splits_on_grey() {
    let mut spec = DocumentSpec::new(2, 1, 8, 3);
    let mut base = LayerSpec::content((0, 0, 1, 2));
    base.channels = vec![(0, vec![128, 64]), (1, vec![128, 64]), (2, vec![128, 64])];
    spec.layers.push(base);
    let mut block = Vec::new();
    push_u16(&mut block, 128);
    push_u16(&mut block, 0);
    spec.layers
        .push(LayerSpec::content((0, 0, 1, 2)).with_block(*b"thrs", block));
    let region = composite_bytes(build_psd(&spec));
    assert_eq!(sample_rgba8(&region, 0, 0), [255, 255, 255, 255]);
    assert_eq!(sample_rgba8(&region, 1, 0), [0, 0, 0, 255]);
}

#[test]
fn an_eight_bit_and_a_sixteen_bit_invert_agree() {
    let mut eight = DocumentSpec::new(1, 1, 8, 3);
    eight
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [100, 150, 200]));
    eight
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"nvrt", Vec::new()));

    let mut sixteen = DocumentSpec::new(1, 1, 16, 3);
    let values = [scaled(100, 16), scaled(150, 16), scaled(200, 16)];
    sixteen.layers.push(LayerSpec::opaque((0, 0, 1, 1), values));
    sixteen
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"nvrt", Vec::new()));

    let region8 = composite_bytes(build_psd(&eight));
    let region16 = composite_bytes(build_psd(&sixteen));
    assert_eq!(region8.rgba8(), region16.rgba8());
}

#[test]
fn a_sixteen_bit_curves_and_levels_match_eight_bit() {
    // Curves sample a fixed 256-entry table and Levels apply directly, so both depths
    // give the same 8-bit result even when the gamma-2.0 shadow toe is in play.
    let levels = levels_block(
        &levels_record(0, 255, 0, 255, 200),
        &identity_levels(),
        &identity_levels(),
        &identity_levels(),
    );
    let curves = curves_block(&[(0, 0), (128, 192), (255, 255)]);

    let mut eight = DocumentSpec::new(1, 1, 8, 3);
    eight
        .layers
        .push(LayerSpec::opaque((0, 0, 1, 1), [100, 150, 200]));
    eight
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"levl", levels.clone()));
    eight
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"curv", curves.clone()));

    let mut sixteen = DocumentSpec::new(1, 1, 16, 3);
    sixteen.layers.push(LayerSpec::opaque(
        (0, 0, 1, 1),
        [scaled(100, 16), scaled(150, 16), scaled(200, 16)],
    ));
    sixteen
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"levl", levels));
    sixteen
        .layers
        .push(LayerSpec::content((0, 0, 1, 1)).with_block(*b"curv", curves));

    let region8 = composite_bytes(build_psd(&eight));
    let region16 = composite_bytes(build_psd(&sixteen));
    assert_eq!(region8.rgba8(), region16.rgba8());
}

#[test]
fn a_vector_mask_block_is_detected_but_not_applied() {
    // The parser records the block; rasterizing the path is a documented gap.
    // Both the `vmsk` and `vsms` keys mark a vector mask.
    for key in [b"vmsk", b"vsms"] {
        let mut spec = DocumentSpec::new(1, 1, 8, 3);
        spec.layers
            .push(LayerSpec::content((0, 0, 1, 1)).with_block(*key, vec![0u8; 24]));
        let document =
            PsdDocument::from_bytes(build_psd(&spec), &ParseOptions::none()).expect("parses");
        assert!(document.file().layers[0].has_vector_mask, "{key:?}");
    }
}
