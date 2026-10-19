//! File header, color mode, compression, and the parsed file model.

use crate::decode::{ChannelRegion, ImageRegion, Viewport, decode_channel};
use crate::error::PsdError;
use crate::layer::{ChannelInfo, GlobalMask, LayerRecord};
use crate::limits::ParseOptions;
use crate::merged::MergedImage;
use crate::parse::sample_bytes;

/// The PSD or PSB file version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsdVersion {
    /// Photoshop document, version 1.
    Psd,
    /// Photoshop large document, version 2.
    Psb,
}

impl PsdVersion {
    /// Return the width of a row-length table entry in bytes.
    ///
    /// The channel data lengths use four or eight bytes (the layer record
    /// rule), but the RLE row counts use two or four.
    #[must_use]
    pub const fn rle_entry_bytes(self) -> usize {
        match self {
            Self::Psd => 2,
            Self::Psb => 4,
        }
    }
}

/// The color mode of a PSD or PSB file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// 1 bit per pixel bitmap.
    Bitmap,
    /// Grayscale.
    Grayscale,
    /// Indexed color.
    Indexed,
    /// RGB color.
    Rgb,
    /// CMYK color.
    Cmyk,
    /// Multichannel.
    Multichannel,
    /// Duotone.
    Duotone,
    /// Lab color.
    Lab,
    /// Any other mode code.
    Other(u16),
}

impl ColorMode {
    /// Map a file color-mode code to a [`ColorMode`].
    #[must_use]
    pub const fn from_code(code: u16) -> Self {
        match code {
            0 => Self::Bitmap,
            1 => Self::Grayscale,
            2 => Self::Indexed,
            3 => Self::Rgb,
            4 => Self::Cmyk,
            7 => Self::Multichannel,
            8 => Self::Duotone,
            9 => Self::Lab,
            other => Self::Other(other),
        }
    }

    /// The number of color channels for the mode, if the mode is composable.
    #[must_use]
    pub const fn color_channels(self) -> Option<usize> {
        match self {
            Self::Rgb => Some(3),
            Self::Cmyk => Some(4),
            _ => None,
        }
    }
}

/// The channel compression code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Uncompressed samples.
    Raw,
    /// Apple `PackBits` run-length encoding.
    Rle,
    /// zlib without prediction. The parser does not decode it.
    Zip,
    /// zlib with prediction. The parser does not decode it.
    ZipPrediction,
    /// Any other code.
    Other(u16),
}

impl Compression {
    /// Map a file compression code to a [`Compression`].
    #[must_use]
    pub const fn from_code(code: u16) -> Self {
        match code {
            0 => Self::Raw,
            1 => Self::Rle,
            2 => Self::Zip,
            3 => Self::ZipPrediction,
            other => Self::Other(other),
        }
    }

    /// The numeric code.
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Raw => 0,
            Self::Rle => 1,
            Self::Zip => 2,
            Self::ZipPrediction => 3,
            Self::Other(code) => code,
        }
    }
}

/// The 26-byte PSD or PSB file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PsdHeader {
    /// The four signature bytes, always `8BPS`.
    pub signature: [u8; 4],
    /// The file version.
    pub version: PsdVersion,
    /// The number of channels in the merged image.
    pub channels: u16,
    /// The image height in pixels.
    pub height: u32,
    /// The image width in pixels.
    pub width: u32,
    /// The bit depth per channel.
    pub depth: u16,
    /// The color mode.
    pub color_mode: ColorMode,
}

/// The parsed header, layers, global mask, and image data of a PSD or PSB.
#[derive(Debug, Clone, PartialEq)]
pub struct PsdFile {
    /// The file header.
    pub header: PsdHeader,
    /// The declared number of layer records.
    pub layer_count: usize,
    /// The layer records, in file order.
    pub layers: Vec<LayerRecord>,
    /// The global layer mask, when present.
    pub global_mask: Option<GlobalMask>,
    /// The merged image data description.
    pub merged: MergedImage,
}

impl PsdFile {
    /// The first layer record, when the file has layers.
    #[must_use]
    pub fn first_layer(&self) -> Option<&LayerRecord> {
        self.layers.first()
    }

    /// Decode the merged image over `viewport`.
    ///
    /// The returned channels cover exactly the viewport. A pixel outside a
    /// channel rectangle is zero. The compositor reads this.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the viewport is invalid, exceeds the cap,
    /// the color mode is unsupported, the deadline passed, or the bytes are
    /// truncated.
    pub fn read_merged_region(
        &self,
        data: &[u8],
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        self.check_region(viewport)?;
        let bytes_per_sample = sample_bytes(self.header.color_mode, self.header.depth)?;
        options.check()?;
        self.decode_channels(
            data,
            &self.merged.channels,
            viewport,
            bytes_per_sample,
            options,
        )
    }

    /// Decode one layer's channels over `viewport`.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the layer index is out of range, the
    /// viewport is invalid or exceeds the cap, the color mode is unsupported,
    /// the deadline passed, or the bytes are truncated.
    pub fn read_layer_region(
        &self,
        data: &[u8],
        index: usize,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        let layer = self.layers.get(index).ok_or(PsdError::LayerOutOfRange {
            index,
            count: self.layers.len(),
        })?;
        self.check_region(viewport)?;
        let bytes_per_sample = sample_bytes(self.header.color_mode, self.header.depth)?;
        options.check()?;
        self.decode_channels(data, &layer.channels, viewport, bytes_per_sample, options)
    }

    /// Validate a viewport against the canvas and the full-tier cap.
    pub(crate) fn check_region(&self, viewport: Viewport) -> Result<(), PsdError> {
        // The cap guard runs first so a huge request never reaches an alloc.
        let edge = viewport.width().max(viewport.height());
        if edge > crate::limits::FULL_TIER_MAX_EDGE {
            return Err(PsdError::CapExceeded {
                edge,
                cap: crate::limits::FULL_TIER_MAX_EDGE,
            });
        }
        if viewport.left > viewport.right
            || viewport.top > viewport.bottom
            || viewport.width() == 0
            || viewport.height() == 0
            || viewport.right > self.header.width
            || viewport.bottom > self.header.height
        {
            return Err(PsdError::InvalidViewport {
                left: viewport.left,
                top: viewport.top,
                right: viewport.right,
                bottom: viewport.bottom,
                width: self.header.width,
                height: self.header.height,
            });
        }
        Ok(())
    }

    /// Decode a list of channels over the viewport.
    fn decode_channels(
        &self,
        data: &[u8],
        channels: &[ChannelInfo],
        viewport: Viewport,
        bytes_per_sample: usize,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        let mut decoded: Vec<ChannelRegion> = Vec::with_capacity(channels.len());
        for channel in channels {
            decoded.push(decode_channel(
                data,
                channel,
                bytes_per_sample,
                self.header.version,
                viewport,
                options,
            )?);
        }
        Ok(ImageRegion {
            viewport,
            depth: self.header.depth,
            color_mode: self.header.color_mode,
            channels: decoded,
        })
    }
}
