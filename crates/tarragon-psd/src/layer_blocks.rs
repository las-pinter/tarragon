//! Tagged blocks: the signature, key, and length framing.
//!
//! The additional layer info of a layer record and the tagged blocks at the end
//! of the layer-and-mask section share one framing. A PSB file writes some keys
//! with an eight-byte length.

use crate::error::PsdError;
use crate::model::PsdVersion;
use crate::reader::{Reader, to_usize};

/// The tagged block signature.
const SIGNATURE: &[u8; 4] = b"8BIM";
/// The large-document tagged block signature.
const SIGNATURE_64: &[u8; 4] = b"8B64";
/// The keys that carry an eight-byte length in a PSB file.
const LARGE_LENGTH_KEYS: [&[u8; 4]; 13] = [
    b"LMsk", b"Lr16", b"Lr32", b"Layr", b"Mt16", b"Mt32", b"Mtrn", b"Alph", b"FMsk", b"lnk2",
    b"FEid", b"FXid", b"PxSD",
];
/// The keys that hold the layer info of a 16-bit or 32-bit document.
const LAYER_INFO_KEYS: [&[u8; 4]; 3] = [b"Lr16", b"Lr32", b"Layr"];

/// One tagged block: its key and its data.
pub(crate) struct TaggedBlock<'a> {
    /// The four key bytes.
    pub(crate) key: [u8; 4],
    /// The block data, without the framing.
    pub(crate) data: &'a [u8],
    /// The file offset of the first data byte.
    pub(crate) offset: u64,
}

impl TaggedBlock<'_> {
    /// True when the block holds the layer info of a 16-bit or 32-bit document.
    pub(crate) fn is_layer_info(&self) -> bool {
        LAYER_INFO_KEYS.contains(&&self.key)
    }
}

/// Read one tagged block, or `None` when the signature is not a block signature.
///
/// # Errors
///
/// Returns a typed error when the framing or the data runs past the reader.
pub(crate) fn read_tagged_block<'a>(
    reader: &mut Reader<'a>,
    version: PsdVersion,
) -> Result<Option<TaggedBlock<'a>>, PsdError> {
    let signature = reader.read_array::<4>()?;
    if &signature != SIGNATURE && &signature != SIGNATURE_64 {
        return Ok(None);
    }
    let key = reader.read_array::<4>()?;
    let length = if version == PsdVersion::Psb && LARGE_LENGTH_KEYS.contains(&&key) {
        reader.read_u64()?
    } else {
        u64::from(reader.read_u32()?)
    };
    let offset = reader.offset();
    let data = reader.take(to_usize(length)?)?;
    Ok(Some(TaggedBlock { key, data, offset }))
}

#[cfg(test)]
mod tests {
    use super::read_tagged_block;
    use crate::model::PsdVersion;
    use crate::reader::Reader;

    /// One block with the given key and length field bytes.
    fn block(key: [u8; 4], length: &[u8], data: &[u8]) -> Vec<u8> {
        let mut bytes = b"8BIM".to_vec();
        bytes.extend_from_slice(&key);
        bytes.extend_from_slice(length);
        bytes.extend_from_slice(data);
        bytes
    }

    #[test]
    fn a_psb_large_key_reads_an_eight_byte_length() {
        let bytes = block(*b"PxSD", &3u64.to_be_bytes(), b"abc");
        let mut reader = Reader::new(&bytes);
        let tagged = read_tagged_block(&mut reader, PsdVersion::Psb)
            .expect("the block reads")
            .expect("a block");
        assert_eq!(tagged.data, b"abc");
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn a_psb_ordinary_key_and_a_psd_large_key_read_four_bytes() {
        let ordinary = block(*b"lsct", &3u32.to_be_bytes(), b"abc");
        let mut reader = Reader::new(&ordinary);
        let tagged = read_tagged_block(&mut reader, PsdVersion::Psb)
            .expect("the block reads")
            .expect("a block");
        assert_eq!(tagged.data, b"abc");
        let large = block(*b"PxSD", &3u32.to_be_bytes(), b"abc");
        let mut reader = Reader::new(&large);
        let tagged = read_tagged_block(&mut reader, PsdVersion::Psd)
            .expect("the block reads")
            .expect("a block");
        assert_eq!(tagged.data, b"abc");
    }

    #[test]
    fn a_foreign_signature_ends_the_walk() {
        let mut bytes = block(*b"lsct", &0u32.to_be_bytes(), b"");
        bytes[..4].copy_from_slice(b"NOPE");
        let mut reader = Reader::new(&bytes);
        assert!(
            read_tagged_block(&mut reader, PsdVersion::Psd)
                .expect("no error")
                .is_none()
        );
    }
}
