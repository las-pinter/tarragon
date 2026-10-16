//! The renderer function type shared by all format back ends.
//!
//! A renderer returns a `DynamicImage` with its EXIF orientation applied. The
//! render crate does not depend on egui (Q16). The UI calls `to_rgba8()` at
//! texture upload (task 34).

#![allow(clippy::module_name_repetitions)]

use std::path::Path;

use image::DynamicImage;
use tarragon_core::error::RenderError;

/// A decoded image with the EXIF orientation applied.
///
/// The render boundary hands off one type: `CacheStore::write_all` takes
/// `&DynamicImage` directly, and the UI converts to RGBA8 with `to_rgba8()` at
/// texture upload (task 34).
pub type RenderedImage = DynamicImage;

/// A renderer: read `path` and return an oriented image, or a typed error.
///
/// The signature is small on purpose. A caller that needs a smaller size
/// scales the full-resolution result; the cache store owns the tier scaling
/// (task 10). The renderer never resizes, so there is one resize path.
pub type Renderer = fn(&Path) -> Result<RenderedImage, RenderError>;
