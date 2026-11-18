//! Hand-built raw PSD bytes and a fixture reader for the parser tests.

use tarragon_psd::parse::read_bytes;

use super::fixture;

/// Read a fixture's bytes and parsed structure.
pub fn read_fixture(name: &str) -> (Vec<u8>, tarragon_psd::PsdFile) {
    let bytes = std::fs::read(fixture(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
    let file = read_bytes(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
    (bytes, file)
}

/// Build a minimal raw-compression PSD for the given shape.
pub fn raw_psd(width: u32, height: u32, channels: u16, mode: u16, planes: &[Vec<u8>]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"8BPS");
    data.extend_from_slice(&1u16.to_be_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    data.extend_from_slice(&channels.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&8u16.to_be_bytes());
    data.extend_from_slice(&mode.to_be_bytes());
    // Empty color-mode data and image resources.
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes());
    // An empty layer-and-mask section.
    data.extend_from_slice(&0u32.to_be_bytes());
    // A raw merged image.
    data.extend_from_slice(&0u16.to_be_bytes());
    for plane in planes {
        data.extend_from_slice(plane);
    }
    data
}

/// The file offset of the layer-and-mask section length field.
pub fn section_offset(bytes: &[u8]) -> usize {
    let color = u32::from_be_bytes([bytes[26], bytes[27], bytes[28], bytes[29]]) as usize;
    let resources_at = 30 + color;
    let resources = u32::from_be_bytes([
        bytes[resources_at],
        bytes[resources_at + 1],
        bytes[resources_at + 2],
        bytes[resources_at + 3],
    ]) as usize;
    resources_at + 4 + resources
}

/// Build a minimal PSD with one layer and the given channel blocks.
///
/// Each channel is `(id, declared_length, block)`. The block bytes are stored
/// in the file, and the declared length is written into the record. A declared
/// length that differs from the block is the corruption under test.
pub fn layered_psd(width: u32, height: u32, channels: &[(i16, u32, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    // Header: 1x1 or width x height RGB, 8-bit, three merged channels.
    data.extend_from_slice(b"8BPS");
    data.extend_from_slice(&1u16.to_be_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    data.extend_from_slice(&3u16.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&8u16.to_be_bytes());
    data.extend_from_slice(&3u16.to_be_bytes());
    // Empty color-mode data and image resources.
    data.extend_from_slice(&0u32.to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes());

    // The extra data: mask length, blending ranges length, pascal name.
    let mut extra = Vec::new();
    extra.extend_from_slice(&0u32.to_be_bytes());
    extra.extend_from_slice(&0u32.to_be_bytes());
    extra.extend_from_slice(&[0, 0, 0, 0]);

    // One layer record.
    let mut record = Vec::new();
    record.extend_from_slice(&0i32.to_be_bytes());
    record.extend_from_slice(&0i32.to_be_bytes());
    record.extend_from_slice(
        &i32::try_from(height)
            .expect("height fits i32")
            .to_be_bytes(),
    );
    record.extend_from_slice(&i32::try_from(width).expect("width fits i32").to_be_bytes());
    record.extend_from_slice(
        &u16::try_from(channels.len())
            .expect("channel count fits u16")
            .to_be_bytes(),
    );
    for (id, declared, _) in channels {
        record.extend_from_slice(&id.to_be_bytes());
        record.extend_from_slice(&declared.to_be_bytes());
    }
    record.extend_from_slice(b"8BIM");
    record.extend_from_slice(b"norm");
    record.push(255);
    record.push(0);
    record.push(0);
    record.push(0);
    record.extend_from_slice(
        &u32::try_from(extra.len())
            .expect("extra fits u32")
            .to_be_bytes(),
    );
    record.extend_from_slice(&extra);

    // Layer info: count, record, then the channel data blocks.
    let mut info = Vec::new();
    info.extend_from_slice(&1i16.to_be_bytes());
    info.extend_from_slice(&record);
    for (_, _, block) in channels {
        info.extend_from_slice(block);
    }

    // The layer-and-mask section: layer_info_length, info, global mask length.
    data.extend_from_slice(
        &u32::try_from(4 + info.len() + 4)
            .expect("section fits u32")
            .to_be_bytes(),
    );
    data.extend_from_slice(
        &u32::try_from(info.len())
            .expect("info fits u32")
            .to_be_bytes(),
    );
    data.extend_from_slice(&info);
    data.extend_from_slice(&0u32.to_be_bytes());

    // Merged image: raw, three channels.
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend(std::iter::repeat_n(
        0u8,
        usize::try_from(width * height * 3).unwrap_or(0),
    ));
    data
}

/// An RLE merged image section: the compression code, the row length table of
/// every channel, then the packed rows of every channel.
///
/// Each plane holds one 8-bit channel, `width` bytes per row. A PSB writes
/// four-byte row lengths.
#[must_use]
pub fn rle_merged_section(width: usize, planes: &[Vec<u8>], psb: bool) -> Vec<u8> {
    let packed: Vec<Vec<Vec<u8>>> = planes
        .iter()
        .map(|plane| plane.chunks(width).map(super::packbits_row).collect())
        .collect();
    let mut section = Vec::new();
    super::push_u16(&mut section, 1);
    for rows in &packed {
        for row in rows {
            if psb {
                super::push_u32(&mut section, u32::try_from(row.len()).expect("row length"));
            } else {
                super::push_u16(&mut section, u16::try_from(row.len()).expect("row length"));
            }
        }
    }
    for rows in &packed {
        for row in rows {
            section.extend_from_slice(row);
        }
    }
    section
}
