//! Encoders for the pattern table blocks and the `PtFl` block of a synthetic PSD.

use super::builder::{push_i32, push_u16, push_u32};
use super::encoders::{
    descriptor_body, descriptor_item, push_unicode, value_boolean, value_descriptor, value_double,
    value_unit_float, versioned,
};

/// One pattern record for the synthetic builder.
pub struct PatternSpec {
    /// The image mode: 1 grey, 2 indexed, 3 RGB, 4 CMYK.
    pub mode: u32,
    /// The tile width.
    pub width: u32,
    /// The tile height.
    pub height: u32,
    /// The pattern name.
    pub name: String,
    /// The pattern id.
    pub id: String,
    /// The bit depth of every channel.
    pub depth: u16,
    /// The color planes, one sample per pixel of the channel rect.
    pub planes: Vec<Vec<u16>>,
    /// The alpha plane, when present.
    pub alpha: Option<Vec<u16>>,
    /// True to write RLE channels. False writes raw channels.
    pub rle: bool,
    /// The 768-byte palette of an indexed pattern.
    pub palette: Option<Vec<u8>>,
    /// The rect of every channel: top, left, bottom, right. `None` is the whole
    /// pattern.
    pub channel_rect: Option<(i32, i32, i32, i32)>,
    /// A value for the 16-bit depth field, in place of the real depth.
    pub short_depth: Option<u16>,
    /// True to omit the 4 extra bytes after an indexed palette.
    pub tight_palette: bool,
}

impl PatternSpec {
    /// An 8-bit RGB pattern from a row-major pixel list.
    pub fn rgb8(id: &str, width: u32, height: u32, pixels: &[[u8; 3]]) -> Self {
        let plane = |channel: usize| {
            pixels
                .iter()
                .map(|pixel| u16::from(pixel[channel]))
                .collect()
        };
        Self {
            mode: 3,
            width,
            height,
            name: format!("name {id}"),
            id: id.to_owned(),
            depth: 8,
            planes: vec![plane(0), plane(1), plane(2)],
            alpha: None,
            rle: false,
            palette: None,
            channel_rect: None,
            short_depth: None,
            tight_palette: false,
        }
    }

    /// A grey pattern from a row-major sample list.
    pub fn grey(id: &str, width: u32, height: u32, depth: u16, samples: &[u16]) -> Self {
        Self {
            mode: 1,
            planes: vec![samples.to_vec()],
            depth,
            ..Self::rgb8(id, width, height, &[])
        }
    }
}

/// Frame one tagged block: `8BIM`, key, length, data and padding to four bytes.
pub fn tagged_block(key: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut block = b"8BIM".to_vec();
    block.extend_from_slice(&key);
    push_u32(&mut block, u32::try_from(data.len()).expect("block length"));
    block.extend_from_slice(data);
    block.extend(std::iter::repeat_n(0u8, (4 - data.len() % 4) % 4));
    block
}

/// Encode one row with `PackBits`: repeat packets for runs, literals otherwise.
pub fn packbits_row(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < row.len() {
        let mut run = 1;
        while at + run < row.len() && row[at + run] == row[at] && run < 128 {
            run += 1;
        }
        if run >= 2 {
            out.push(u8::try_from(257 - run).expect("repeat control"));
            out.push(row[at]);
            at += run;
            continue;
        }
        let start = at;
        while at < row.len() && at - start < 128 && !(at + 1 < row.len() && row[at] == row[at + 1])
        {
            at += 1;
        }
        out.push(u8::try_from(at - start - 1).expect("literal control"));
        out.extend_from_slice(&row[start..at]);
    }
    out
}

/// Encode the RLE data of a channel: the row length table then the rows.
pub fn packbits_rows(bytes: &[u8], row_bytes: usize) -> Vec<u8> {
    let rows: Vec<Vec<u8>> = if row_bytes == 0 {
        Vec::new()
    } else {
        bytes.chunks(row_bytes).map(packbits_row).collect()
    };
    let mut out = Vec::new();
    for row in &rows {
        push_u16(&mut out, u16::try_from(row.len()).expect("row length"));
    }
    for row in &rows {
        out.extend_from_slice(row);
    }
    out
}

/// Encode one channel array of the virtual memory array list.
fn channel_array(spec: &PatternSpec, samples: &[u16], rect: (i32, i32, i32, i32)) -> Vec<u8> {
    let mut bytes = Vec::new();
    for sample in samples {
        if spec.depth == 8 {
            bytes.push(u8::try_from(*sample).expect("8-bit sample"));
        } else {
            push_u16(&mut bytes, *sample);
        }
    }
    let width = usize::try_from(rect.3 - rect.1).expect("channel width");
    let data = if spec.rle {
        packbits_rows(&bytes, width * usize::from(spec.depth / 8))
    } else {
        bytes
    };
    let mut array = Vec::new();
    push_u32(&mut array, 1);
    push_u32(
        &mut array,
        u32::try_from(23 + data.len()).expect("array length"),
    );
    push_u32(&mut array, u32::from(spec.depth));
    for edge in [rect.0, rect.1, rect.2, rect.3] {
        push_i32(&mut array, edge);
    }
    push_u16(&mut array, spec.short_depth.unwrap_or(spec.depth));
    array.push(u8::from(spec.rle));
    array.extend_from_slice(&data);
    array
}

/// Encode one pattern record body (without the length prefix).
pub fn pattern_body(spec: &PatternSpec) -> Vec<u8> {
    let mut body = Vec::new();
    push_u32(&mut body, 1);
    push_u32(&mut body, spec.mode);
    push_u16(&mut body, u16::try_from(spec.height).expect("height"));
    push_u16(&mut body, u16::try_from(spec.width).expect("width"));
    push_unicode(&mut body, &format!("{}\0", spec.name));
    let id = format!("{}\0", spec.id);
    body.push(u8::try_from(id.len()).expect("id length"));
    body.extend_from_slice(id.as_bytes());
    if let Some(palette) = &spec.palette {
        body.extend_from_slice(palette);
        // A writer that adds four bytes after the palette.
        if !spec.tight_palette {
            push_u32(&mut body, 0);
        }
    }
    let full = (
        0,
        0,
        i32::try_from(spec.height).expect("height"),
        i32::try_from(spec.width).expect("width"),
    );
    let rect = spec.channel_rect.unwrap_or(full);
    let mut arrays = Vec::new();
    let mut count = 0u32;
    for plane in spec.planes.iter().chain(spec.alpha.iter()) {
        arrays.extend_from_slice(&channel_array(spec, plane, rect));
        count += 1;
    }
    // Two arrays that the writer marks as not written.
    push_u32(&mut arrays, 0);
    push_u32(&mut arrays, 0);
    let mut list = Vec::new();
    for edge in [full.0, full.1, full.2, full.3] {
        push_i32(&mut list, edge);
    }
    push_u32(&mut list, count);
    list.extend_from_slice(&arrays);
    push_u32(&mut body, 3);
    push_u32(&mut body, u32::try_from(list.len()).expect("list length"));
    body.extend_from_slice(&list);
    body
}

/// Encode one pattern record: the length, the body and padding to four bytes.
pub fn pattern_record(spec: &PatternSpec) -> Vec<u8> {
    let body = pattern_body(spec);
    let mut record = Vec::new();
    push_u32(
        &mut record,
        u32::try_from(body.len()).expect("record length"),
    );
    record.extend_from_slice(&body);
    record.extend(std::iter::repeat_n(0u8, (4 - body.len() % 4) % 4));
    record
}

/// Encode a `TEXT` value.
fn value_text(value: &str) -> Vec<u8> {
    let mut data = b"TEXT".to_vec();
    push_unicode(&mut data, value);
    data
}

/// The placement keys of a `PtFl` block.
pub struct PtflSpec {
    /// The pattern id. The trailing NUL is added.
    pub id: String,
    /// The pattern name. The trailing NUL is added.
    pub name: String,
    /// The scale in percent.
    pub scale: f64,
    /// The angle in degrees.
    pub angle: f64,
    /// The key and value of the link flag, when present.
    pub link: Option<([u8; 4], bool)>,
    /// The phase in pixels.
    pub phase: (f64, f64),
}

impl PtflSpec {
    /// A linked pattern at 100 percent, no rotation and no phase.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: format!("name {id}"),
            scale: 100.0,
            angle: 0.0,
            link: Some((*b"Algn", true)),
            phase: (0.0, 0.0),
        }
    }
}

/// Build a `PtFl` block.
pub fn ptfl_block(spec: &PtflSpec) -> Vec<u8> {
    let pattern = descriptor_body(
        b"Ptrn",
        &[
            descriptor_item(b"Nm  ", &value_text(&format!("{}\0", spec.name))),
            descriptor_item(b"Idnt", &value_text(&format!("{}\0", spec.id))),
        ],
    );
    let phase = descriptor_body(
        b"Pnt ",
        &[
            descriptor_item(b"Hrzn", &value_double(spec.phase.0)),
            descriptor_item(b"Vrtc", &value_double(spec.phase.1)),
        ],
    );
    let mut items = vec![
        descriptor_item(b"Ptrn", &value_descriptor(&pattern)),
        descriptor_item(b"Angl", &value_unit_float(*b"#Ang", spec.angle)),
        descriptor_item(b"Scl ", &value_unit_float(*b"#Prc", spec.scale)),
        descriptor_item(b"phase", &value_descriptor(&phase)),
    ];
    if let Some((key, linked)) = spec.link {
        items.push(descriptor_item(&key, &value_boolean(linked)));
    }
    versioned(&descriptor_body(b"null", &items))
}
