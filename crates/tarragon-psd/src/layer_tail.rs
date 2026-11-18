//! The tagged blocks after the global mask: the layer info of a 16-bit or
//! 32-bit document and the document pattern table.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use crate::content::LayerContent;
use crate::error::PsdError;
use crate::layer::{LayerInfo, LayerRecord};
use crate::layer_blocks::read_tagged_header;
use crate::layer_info::parse_layer_info;
use crate::limits::ParseOptions;
use crate::model::PsdHeader;
use crate::patterns::{MAX_DECODED_PIXELS, PatternEntry, find_pattern, parse_pattern_block};
use crate::reader::to_usize;
use crate::window::Window;

/// The largest pattern table the parser reads into memory, in bytes.
///
/// A pattern keeps its record bytes for a lazy decode, and the decoded pixels
/// have their own budget. A larger table is skipped, and the pattern fills
/// that name it fall back to unsupported.
pub(crate) const MAX_PATTERN_BLOCK: u64 = 128 * 1024 * 1024;

/// What the tagged blocks after the global mask hold.
#[derive(Default)]
pub(crate) struct TaggedTail {
    /// The layer info of a 16-bit or 32-bit document.
    pub(crate) layer_info: Option<LayerInfo>,
    /// The document pattern table.
    pub(crate) patterns: Vec<PatternEntry>,
    /// Why a pattern table was skipped, when it was too large to read.
    pub(crate) pattern_note: Option<String>,
    /// The first error that stopped a layer info parse or the block walk.
    pub(crate) error: Option<PsdError>,
}

/// Mark a pattern fill unsupported when the pattern table lacks its pattern.
pub(crate) fn drop_unmatched_pattern_fills(layers: &mut [LayerRecord], patterns: &[PatternEntry]) {
    for layer in layers {
        if let LayerContent::Pattern(fill) = &layer.content
            && find_pattern(patterns, &fill.id, &fill.name).is_none()
        {
            layer.content = LayerContent::Unsupported;
        }
    }
}

/// Read the tagged blocks after the global mask, up to the section end.
///
/// A layer info block is parsed as records with its channel data left in the
/// file. A pattern table is read whole when it is no larger than
/// `pattern_limit` (production passes [`MAX_PATTERN_BLOCK`]). Every other block is skipped unread.
pub(crate) fn read_tagged_tail(
    section: &mut Window,
    header: &PsdHeader,
    options: &ParseOptions,
    want_layers: bool,
    pattern_limit: u64,
    tail: &mut TaggedTail,
) -> Result<(), PsdError> {
    let source = section.source();
    let pattern_budget = Arc::new(AtomicUsize::new(MAX_DECODED_PIXELS));
    while section.remaining() >= 12 {
        options.check()?;
        let Some(block) = read_tagged_header(section, header.version)? else {
            break;
        };
        if block.is_layer_info() {
            if want_layers && tail.layer_info.is_none() && tail.error.is_none() {
                let end = block.offset + block.length;
                match parse_layer_info(source, block.offset, end, header, options) {
                    Ok(found) => tail.layer_info = Some(found),
                    Err(error) if error.is_transient() => return Err(error),
                    Err(error) => tail.error = Some(error),
                }
            }
        } else if block.is_pattern_table() {
            if block.length <= pattern_limit {
                let data = source.read_vec(block.offset, to_usize(block.length)?)?;
                parse_pattern_block(&data, &pattern_budget, options, &mut tail.patterns)?;
            } else {
                // Pattern fills that need this table fall back to unsupported,
                // so a caller needs to know why.
                tail.pattern_note = Some(format!(
                    "pattern table of {} bytes skipped, the limit is {} MiB",
                    block.length,
                    pattern_limit / (1024 * 1024)
                ));
            }
        }
        // These blocks pad to a four-byte boundary.
        let pad = (4 - block.length % 4) % 4;
        section.seek(block.offset + block.length);
        section.skip(pad.min(section.remaining()))?;
    }
    Ok(())
}
