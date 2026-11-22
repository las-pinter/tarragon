//! The mask channels of a layer that the compositor reads.
//!
//! One rule picks both the channels the layer read decodes and the masks the
//! compositor builds, so the two cannot drift apart.

use crate::layer::LayerRecord;

impl LayerRecord {
    /// The user mask channel and the vector mask source of this layer.
    ///
    /// Without mask flag bit 3, channel -2 is the user mask and the vector
    /// path is drawn. Bit 3 says -2 holds the vector mask as the writer
    /// rendered it:
    /// - with the real mask fields, the user mask is channel -3 and the path
    ///   is drawn, or -2 stands in when no path was parsed;
    /// - without them, -2 is the vector mask and the path is not drawn.
    ///
    /// Mask flag bit 1 disables the user mask. A disabled path disables the
    /// vector mask.
    #[must_use]
    pub(crate) fn mask_channels(&self) -> MaskChannels {
        let path = self.vector_mask.as_ref();
        if path.is_some_and(|path| path.disabled) {
            return MaskChannels {
                user: self.user_channel(),
                vector: VectorSource::None,
            };
        }
        let drawn = if path.is_some() {
            VectorSource::Path
        } else {
            VectorSource::None
        };
        let Some(mask) = self.mask else {
            return MaskChannels {
                user: None,
                vector: drawn,
            };
        };
        let vector = if mask.flags & MASK_VECTOR_RENDERED == 0 {
            drawn
        } else if mask.real.is_some() && path.is_some() {
            VectorSource::Path
        } else {
            VectorSource::Channel
        };
        MaskChannels {
            user: self.user_channel(),
            vector,
        }
    }

    /// The user mask channel: none when the mask is absent or disabled, -3
    /// with bit 3 and the real mask fields, none with bit 3 alone, else -2.
    fn user_channel(&self) -> Option<i16> {
        let mask = self.mask?;
        if mask.flags & MASK_DISABLED != 0 {
            None
        } else if mask.flags & MASK_VECTOR_RENDERED != 0 {
            mask.real.map(|_| -3)
        } else {
            Some(-2)
        }
    }
}

/// Mask flag bit 1: the user mask is disabled.
const MASK_DISABLED: u8 = 0x02;
/// Mask flag bit 3: channel -2 holds the rendered vector mask.
const MASK_VECTOR_RENDERED: u8 = 0x08;

/// Where the vector mask of a layer comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VectorSource {
    /// The layer has no vector mask.
    None,
    /// The `vmsk` path, drawn by the rasterizer.
    Path,
    /// Channel -2 as stored.
    Channel,
}

/// The mask channels of one layer that the compositor reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MaskChannels {
    /// The user mask channel: -2, -3 (placed by the real mask fields), or
    /// `None` when the user mask is absent or disabled.
    pub(crate) user: Option<i16>,
    /// The vector mask source.
    pub(crate) vector: VectorSource,
}

impl MaskChannels {
    /// True when the compositor reads the channel `id`.
    #[must_use]
    pub(crate) fn reads(self, id: i16) -> bool {
        match id {
            -3 => self.user == Some(-3),
            -2 => self.user == Some(-2) || self.vector == VectorSource::Channel,
            _ => true,
        }
    }
}
