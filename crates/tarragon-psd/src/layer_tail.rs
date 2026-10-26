//! The tagged blocks after the global mask: the layer info of a 16-bit or
//! 32-bit document and the document pattern table.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use crate::content::LayerContent;
use crate::error::PsdError;
use crate::layer::{LayerInfo, LayerRecord, parse_layer_info};
use crate::layer_blocks::read_tagged_block;
use crate::limits::ParseOptions;
use crate::model::PsdHeader;
use crate::patterns::{MAX_DECODED_PIXELS, PatternEntry, find_pattern, parse_pattern_block};
use crate::reader::Reader;

/// What the tagged blocks after the global mask hold.
#[derive(Default)]
pub(crate) struct TaggedTail {
    /// The layer info of a 16-bit or 32-bit document.
    pub(crate) layer_info: Option<LayerInfo>,
    /// The document pattern table.
    pub(crate) patterns: Vec<PatternEntry>,
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

/// Walk the tagged blocks after the global mask.
///
/// A 16-bit or 32-bit document writes an empty main layer info and keeps the
/// layers in an `Lr16`, `Lr32`, or `Layr` block. The first such block wins, and
/// only when `want_layers` is set. The `Patt`, `Pat2` and `Pat3` blocks hold the
/// pattern table. The walk always reaches the end of the blocks, so the order of
/// the blocks does not matter.
pub(crate) fn read_tagged_tail(
    section: &mut Reader,
    header: &PsdHeader,
    options: &ParseOptions,
    want_layers: bool,
    tail: &mut TaggedTail,
) -> Result<(), PsdError> {
    let pattern_budget = Arc::new(AtomicUsize::new(MAX_DECODED_PIXELS));
    while section.remaining() >= 12 {
        options.check()?;
        let Some(block) = read_tagged_block(section, header.version)? else {
            break;
        };
        if block.is_layer_info() {
            if want_layers && tail.layer_info.is_none() && tail.error.is_none() {
                let mut body = Reader::at(block.data, block.offset);
                match parse_layer_info(&mut body, header, options) {
                    Ok(found) => tail.layer_info = Some(found),
                    Err(error) if error.is_transient() => return Err(error),
                    Err(error) => tail.error = Some(error),
                }
            }
        } else if block.is_pattern_table() {
            parse_pattern_block(block.data, &pattern_budget, options, &mut tail.patterns)?;
        }
        // These blocks pad to a four-byte boundary.
        let pad = (4 - block.data.len() % 4) % 4;
        section.skip(pad.min(section.remaining()))?;
    }
    Ok(())
}
