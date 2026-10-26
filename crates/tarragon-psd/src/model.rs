//! File header, color mode, compression, and the parsed file model.

use crate::decode::{ChannelRegion, ImageRegion, Viewport, decode_channel};
use crate::effects::GlobalLight;
use crate::error::PsdError;
use crate::layer::{ChannelInfo, GlobalMask, LayerRecord};
use crate::limits::ParseOptions;
use crate::merged::MergedImage;
use crate::patterns::PatternEntry;
use crate::source::Source;

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
            Self::Grayscale => Some(1),
            Self::Rgb => Some(3),
            Self::Cmyk => Some(4),
            _ => None,
        }
    }
}

/// The bytes per sample for the supported color modes.
pub(crate) fn sample_bytes(mode: ColorMode, depth: u16) -> Result<usize, PsdError> {
    match (mode, depth) {
        (ColorMode::Rgb | ColorMode::Cmyk | ColorMode::Grayscale, 8) => Ok(1),
        (ColorMode::Rgb | ColorMode::Grayscale, 16) => Ok(2),
        _ => Err(PsdError::UnsupportedColorMode { mode, depth }),
    }
}

/// The sample count of a document depth, used by adjustment lookup tables.
#[must_use]
pub(crate) const fn sample_levels(depth: u16) -> u32 {
    if depth >= 16 { 65_536 } else { 256 }
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
    /// The document global light for the effects that follow it.
    pub global_light: GlobalLight,
    /// The document pattern table from the `Patt`, `Pat2` and `Pat3` blocks.
    pub patterns: Vec<PatternEntry>,
    /// True when the layer count is negative, which marks the first extra
    /// merged channel as the transparency of the merged image.
    pub merged_has_alpha: bool,
    /// The message of the error raised while scanning the tagged blocks for
    /// layer info, when that scan failed. The file still parses, with no layers
    /// from those blocks.
    pub layer_info_error: Option<String>,
    /// Why the document pattern table was skipped, when it was too large to
    /// read. Pattern fills then render as unsupported.
    pub pattern_note: Option<String>,
    /// The message of the error raised while reading the merged image section,
    /// when a lenient parse kept the layers anyway. A strict parse never sets
    /// it: the strict parse fails instead. The merged description is then empty.
    pub merged_error: Option<String>,
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
    pub(crate) fn read_merged_region_from(
        &self,
        source: &Source,
        viewport: Viewport,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        self.check_region(viewport)?;
        let bytes_per_sample = sample_bytes(self.header.color_mode, self.header.depth)?;
        options.check()?;
        self.decode_channels(
            source,
            &self.merged.channels,
            viewport,
            1,
            bytes_per_sample,
            options,
        )
    }

    /// Decode the merged image over `viewport`, keeping every `step`th pixel.
    ///
    /// The output is `ceil(width / step)` by `ceil(height / step)` and its
    /// viewport is `0..width`, `0..height` of that size. The 8192 px cap
    /// applies to the output, not to the source, so a canvas wider than the cap
    /// reads through a step. The decoder keeps the output plus one source row
    /// and never decodes a skipped row.
    ///
    /// # Errors
    ///
    /// A step of zero is treated as one. Returns a typed error when the
    /// viewport is invalid, the output exceeds the cap, the color mode is unsupported, the deadline
    /// passed, or the bytes are truncated.
    pub(crate) fn read_merged_strided_from(
        &self,
        source: &Source,
        viewport: Viewport,
        step: u32,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        let step = step.max(1);
        let out = Viewport::full(
            viewport.width().div_ceil(step),
            viewport.height().div_ceil(step),
        );
        // The cap guard runs on the output, then the source rectangle is
        // checked against the canvas.
        Self::check_cap(out.width().max(out.height()))?;
        self.check_bounds(viewport)?;
        let bytes_per_sample = sample_bytes(self.header.color_mode, self.header.depth)?;
        options.check()?;
        let mut region = self.decode_channels(
            source,
            &self.merged.channels,
            viewport,
            usize::try_from(step).unwrap_or(usize::MAX),
            bytes_per_sample,
            options,
        )?;
        region.viewport = out;
        Ok(region)
    }

    /// Decode one layer's channels over `viewport`.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the layer index is out of range, the
    /// viewport is invalid or exceeds the cap, the color mode is unsupported,
    /// the deadline passed, or the bytes are truncated.
    pub(crate) fn read_layer_region_from(
        &self,
        source: &Source,
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
        self.decode_channels(
            source,
            &layer.channels,
            viewport,
            1,
            bytes_per_sample,
            options,
        )
    }

    /// Decode the merged image over `viewport` from bytes in memory.
    ///
    /// This copies `data` on every call. A caller that reads more than once
    /// should open a [`crate::PsdDocument`] instead.
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
        self.read_merged_region_from(&Source::memory(data.to_vec()), viewport, options)
    }

    /// Decode the merged image over `viewport`, keeping every `step`th pixel,
    /// from bytes in memory. See [`PsdFile::read_merged_region`] for the copy.
    ///
    /// # Errors
    ///
    /// A step of zero is treated as one. Returns a typed error when the
    /// viewport is invalid, the output exceeds the cap, the color mode is
    /// unsupported, the deadline passed, or the bytes are truncated.
    pub fn read_merged_strided(
        &self,
        data: &[u8],
        viewport: Viewport,
        step: u32,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        self.read_merged_strided_from(&Source::memory(data.to_vec()), viewport, step, options)
    }

    /// Decode one layer's channels over `viewport` from bytes in memory. See
    /// [`PsdFile::read_merged_region`] for the copy.
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
        self.read_layer_region_from(&Source::memory(data.to_vec()), index, viewport, options)
    }

    /// Validate a viewport against the canvas and the full-tier cap.
    pub(crate) fn check_region(&self, viewport: Viewport) -> Result<(), PsdError> {
        // The cap guard runs first so a huge request never reaches an alloc.
        Self::check_cap(viewport.width().max(viewport.height()))?;
        self.check_bounds(viewport)
    }

    /// Refuse an output edge over the full-tier cap.
    fn check_cap(edge: u32) -> Result<(), PsdError> {
        if edge > crate::limits::FULL_TIER_MAX_EDGE {
            return Err(PsdError::CapExceeded {
                edge,
                cap: crate::limits::FULL_TIER_MAX_EDGE,
            });
        }
        Ok(())
    }

    /// Refuse an empty or inverted viewport, or one outside the canvas.
    fn check_bounds(&self, viewport: Viewport) -> Result<(), PsdError> {
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

    /// Decode the channels the compositor reads over the viewport.
    ///
    /// A channel the compositor never reads (spot, extra alpha, the real mask)
    /// is skipped, and so is a repeated channel ID, because each decode costs a
    /// full viewport of samples.
    fn decode_channels(
        &self,
        source: &Source,
        channels: &[ChannelInfo],
        viewport: Viewport,
        step: usize,
        bytes_per_sample: usize,
        options: &ParseOptions,
    ) -> Result<ImageRegion, PsdError> {
        let color_channels = self.header.color_mode.color_channels().unwrap_or(0);
        let mut decoded: Vec<ChannelRegion> = Vec::new();
        for channel in channels {
            let wanted = channel.id == -1
                || channel.id == -2
                || usize::try_from(channel.id).is_ok_and(|id| id < color_channels);
            if !wanted || decoded.iter().any(|done| done.id == channel.id) {
                continue;
            }
            decoded.push(decode_channel(
                source,
                channel,
                bytes_per_sample,
                self.header.version,
                viewport,
                step,
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
