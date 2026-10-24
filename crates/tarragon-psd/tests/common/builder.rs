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
    /// A complete mask body that replaces the generated 18-byte body.
    pub raw_body: Option<Vec<u8>>,
}

/// One layer in a synthetic document.
pub struct LayerSpec {
    /// The layer name. A missing name uses the layer index.
    pub name: Option<String>,
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
    /// The compression code written before each channel block. The samples stay
    /// raw, so a code other than 0 makes the layer undecodable.
    pub compression: u16,
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
            name: None,
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
            compression: 0,
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
            name: None,
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
            compression: 0,
        }
    }

    /// A color-only layer that carries no channel data, for a fill or adjustment.
    pub fn content(rect: (i32, i32, i32, i32)) -> Self {
        Self {
            name: None,
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
            compression: 0,
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
            raw_body: None,
        });
        self
    }

    /// Attach a layer mask with a complete hand-built body.
    pub fn with_raw_mask(mut self, body: Vec<u8>) -> Self {
        self.mask = Some(MaskSpec {
            rect: (0, 0, 0, 0),
            default_color: 0,
            flags: 0,
            raw_body: Some(body),
        });
        self
    }

    /// A gray pixel layer: one gray channel, no alpha.
    pub fn gray(rect: (i32, i32, i32, i32), value: u16) -> Self {
        let width = usize::try_from(rect.3 - rect.1).expect("width");
        let height = usize::try_from(rect.2 - rect.0).expect("height");
        let mut layer = Self::content(rect);
        layer.channels.push((0, vec![value; width * height]));
        layer
    }

    /// An empty group marker record.
    pub fn group_marker(section: u32, blend: [u8; 4]) -> Self {
        Self {
            name: None,
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
            compression: 0,
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
    /// The image resource blocks: `(id, data)` pairs.
    pub resources: Vec<(u16, Vec<u8>)>,
    /// True to write a PSB file with eight-byte lengths.
    pub psb: bool,
    /// The key of a tagged block that holds the layer info after the global
    /// mask, with an empty main layer info. `None` writes the main layer info.
    pub layer_info_block: Option<[u8; 4]>,
    /// True to write a negative layer count, which marks merged transparency.
    pub merged_alpha: bool,
    /// The merged channel count, when it differs from the color mode default.
    pub merged_channels: Option<u16>,
    /// The complete merged image section, with its compression field. It
    /// replaces the generated raw planes.
    pub merged_section: Option<Vec<u8>>,
    /// Raw bytes written after the global mask, in place of tagged blocks.
    pub section_tail: Vec<u8>,
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
            resources: Vec::new(),
            psb: false,
            layer_info_block: None,
            merged_alpha: false,
            merged_channels: None,
            merged_section: None,
            section_tail: Vec::new(),
        }
    }

    /// The merged channel count written to the header.
    fn header_channels(&self) -> u16 {
        self.merged_channels.unwrap_or(match self.color_mode {
            4 => 4,
            1 => 1,
            _ => 3,
        })
    }
}

/// The keys that carry an eight-byte length in a PSB file.
const PSB_LARGE_KEYS: [&[u8; 4]; 13] = [
    b"LMsk", b"Lr16", b"Lr32", b"Layr", b"Mt16", b"Mt32", b"Mtrn", b"Alph", b"FMsk", b"lnk2",
    b"FEid", b"FXid", b"PxSD",
];

/// Push a length field: four bytes in a PSD, eight in a PSB.
pub fn push_length(data: &mut Vec<u8>, value: usize, psb: bool) {
    if psb {
        data.extend_from_slice(&u64::try_from(value).expect("length").to_be_bytes());
    } else {
        push_u32(data, u32::try_from(value).expect("length"));
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
pub fn layer_extra(spec: &LayerSpec, name: &str, psb: bool) -> Vec<u8> {
    let mut extra = Vec::new();
    // The mask data: a length then the mask body, or an empty mask.
    if let Some(body) = spec.mask.as_ref().and_then(|mask| mask.raw_body.as_ref()) {
        push_u32(&mut extra, u32::try_from(body.len()).expect("mask length"));
        extra.extend_from_slice(body);
    } else if let Some(mask) = &spec.mask {
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
        push_length(&mut extra, data.len(), psb && PSB_LARGE_KEYS.contains(&key));
        extra.extend_from_slice(data);
    }
    extra
}

/// Build one layer record with its section divider inside the extra data.
pub fn layer_record(spec: &LayerSpec, depth: u16, name: &str, psb: bool) -> Vec<u8> {
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
        push_length(&mut record, data_length + 2, psb);
    }
    record.extend_from_slice(b"8BIM");
    record.extend_from_slice(&spec.blend);
    record.push(spec.opacity);
    record.push(spec.clipping);
    record.push(spec.flags);
    record.push(0);
    let mut extra = layer_extra(spec, name, psb);
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

/// Build a complete synthetic PSD or PSB file.
pub fn build_psd(spec: &DocumentSpec) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"8BPS");
    push_u16(&mut data, if spec.psb { 2 } else { 1 });
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let merged_channels = spec.header_channels();
    push_u16(&mut data, merged_channels);
    push_u32(&mut data, spec.height);
    push_u32(&mut data, spec.width);
    push_u16(&mut data, spec.depth);
    push_u16(&mut data, spec.color_mode);
    // Empty color-mode data, then the image resources.
    push_u32(&mut data, 0);
    let mut resources = Vec::new();
    for (id, body) in &spec.resources {
        resources.extend_from_slice(b"8BIM");
        push_u16(&mut resources, *id);
        // An empty Pascal name, padded to an even count.
        resources.extend_from_slice(&[0, 0]);
        push_u32(
            &mut resources,
            u32::try_from(body.len()).expect("resource length"),
        );
        resources.extend_from_slice(body);
        if body.len() % 2 == 1 {
            resources.push(0);
        }
    }
    push_u32(
        &mut data,
        u32::try_from(resources.len()).expect("resources length"),
    );
    data.extend_from_slice(&resources);

    let mut info = Vec::new();
    let count = i16::try_from(spec.layers.len()).expect("layer count");
    info.extend_from_slice(&(if spec.merged_alpha { -count } else { count }).to_be_bytes());
    for (index, layer) in spec.layers.iter().enumerate() {
        let name = layer
            .name
            .clone()
            .unwrap_or_else(|| format!("Layer {index}"));
        info.extend_from_slice(&layer_record(layer, spec.depth, &name, spec.psb));
    }
    // The channel data blocks follow the records, in record order.
    for layer in &spec.layers {
        for (_, samples) in &layer.channels {
            push_u16(&mut info, layer.compression);
            push_samples(&mut info, samples, spec.depth);
        }
    }

    // The layer-and-mask section: the layer info, the global mask, and an
    // optional tagged block that carries the layer info instead.
    let mut section = Vec::new();
    if let Some(key) = spec.layer_info_block {
        push_length(&mut section, 0, spec.psb);
        push_u32(&mut section, 0);
        section.extend_from_slice(b"8BIM");
        section.extend_from_slice(&key);
        push_length(
            &mut section,
            info.len(),
            spec.psb && PSB_LARGE_KEYS.contains(&&key),
        );
        section.extend_from_slice(&info);
        let pad = (4 - info.len() % 4) % 4;
        section.extend(std::iter::repeat_n(0u8, pad));
    } else {
        push_length(&mut section, info.len(), spec.psb);
        section.extend_from_slice(&info);
        push_u32(&mut section, 0);
        section.extend_from_slice(&spec.section_tail);
    }
    push_length(&mut data, section.len(), spec.psb);
    data.extend_from_slice(&section);

    // A raw merged image, unless the spec carries the whole section.
    if let Some(section) = &spec.merged_section {
        data.extend_from_slice(section);
        return data;
    }
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
