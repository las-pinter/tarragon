//! The synthetic PSD builder: the document and layer specs and the file writer.

// --------------------------------------------------------------------------
// Synthetic PSD builder
// --------------------------------------------------------------------------

/// A layer mask block for the synthetic builder.
pub struct MaskSpec {
    /// The mask rectangle: top, left, bottom, right.
    pub rect: (i32, i32, i32, i32),
    /// The default color, 0 or 255.
    pub default_color: u8,
    /// The mask flag bits.
    pub flags: u8,
}

/// One layer in a synthetic document.
pub struct LayerSpec {
    /// The layer rectangle: top, left, bottom, right.
    pub rect: (i32, i32, i32, i32),
    /// The channel samples: `(id, width * height samples)`.
    pub channels: Vec<(i16, Vec<u16>)>,
    /// The four blend-key bytes.
    pub blend: [u8; 4],
    /// The layer opacity.
    pub opacity: u8,
    /// The fill opacity block value, when present.
    pub fill_opacity: Option<u8>,
    /// The raw layer flag bits.
    pub flags: u8,
    /// The clipping flag.
    pub clipping: u8,
    /// The section divider code, when the record opens or closes a group.
    pub section: Option<u32>,
    /// Additional tagged blocks, in record order.
    pub blocks: Vec<([u8; 4], Vec<u8>)>,
    /// The layer mask block, when present.
    pub mask: Option<MaskSpec>,
}

impl LayerSpec {
    /// An opaque pixel layer with color channels and no alpha.
    pub fn opaque(rect: (i32, i32, i32, i32), values: [u16; 3]) -> Self {
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
    pub fn with_alpha(rect: (i32, i32, i32, i32), values: [u16; 4]) -> Self {
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
    pub fn content(rect: (i32, i32, i32, i32)) -> Self {
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
    pub fn with_block(mut self, key: [u8; 4], data: Vec<u8>) -> Self {
        self.blocks.push((key, data));
        self
    }

    /// Attach a layer mask.
    pub fn with_mask(mut self, rect: (i32, i32, i32, i32), default_color: u8) -> Self {
        self.mask = Some(MaskSpec {
            rect,
            default_color,
            flags: 0,
        });
        self
    }

    /// An empty group marker record.
    pub fn group_marker(section: u32, blend: [u8; 4]) -> Self {
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
pub struct DocumentSpec {
    /// The canvas width.
    pub width: u32,
    /// The canvas height.
    pub height: u32,
    /// The bit depth, 8 or 16.
    pub depth: u16,
    /// The color mode code: 3 is RGB, 4 is CMYK.
    pub color_mode: u16,
    /// The layers, bottom to top.
    pub layers: Vec<LayerSpec>,
    /// The merged image planes. Empty means zeros.
    pub merged: Vec<Vec<u16>>,
}

impl DocumentSpec {
    /// A new RGB or CMYK document with no layers.
    pub fn new(width: u32, height: u32, depth: u16, color_mode: u16) -> Self {
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
pub fn push_u16(data: &mut Vec<u8>, value: u16) {
    data.extend_from_slice(&value.to_be_bytes());
}

/// Push a big-endian `u32`.
pub fn push_u32(data: &mut Vec<u8>, value: u32) {
    data.extend_from_slice(&value.to_be_bytes());
}

/// Push a big-endian `i32`.
pub fn push_i32(data: &mut Vec<u8>, value: i32) {
    data.extend_from_slice(&value.to_be_bytes());
}

/// The bytes per sample for a depth.
pub fn sample_bytes(depth: u16) -> usize {
    if depth == 8 { 1 } else { 2 }
}

/// Append samples in the file byte order for a depth.
pub fn push_samples(data: &mut Vec<u8>, samples: &[u16], depth: u16) {
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
pub fn layer_extra(spec: &LayerSpec, name: &str) -> Vec<u8> {
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
pub fn layer_record(spec: &LayerSpec, depth: u16, name: &str) -> Vec<u8> {
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
pub fn build_psd(spec: &DocumentSpec) -> Vec<u8> {
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
