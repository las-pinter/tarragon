//! Registry that maps file extensions to renderers.
//!
//! The registry wraps the core extension list, re-exported here so the two
//! cannot drift. A specialized format adds an entry to [`FORMAT_DISPATCH`];
//! every other supported format uses [`DEFAULT_RENDERER`], the plain renderer.
//!
//! The registry refuses an unknown extension with
//! [`RenderError::UnsupportedFormat`]. The refusal is deliberate. An unknown
//! type takes the error-state path.

use std::ffi::OsStr;
use std::path::Path;

use tarragon_core::error::RenderError;
pub use tarragon_core::format::SUPPORTED_EXTENSIONS;
use tarragon_core::format::is_supported_extension;

use crate::clip::render_clip_image;
use crate::guard::run_guarded;
use crate::krita::render_kra_image;
use crate::plain::render_plain_image;
use crate::psd::render_psd_image;
use crate::renderer::{RenderedImage, Renderer};

/// The specialized format dispatch. Keys are lower case with a leading dot.
///
/// The table holds the formats that decode their own container. Every other
/// supported format uses the plain renderer.
pub const FORMAT_DISPATCH: &[(&str, Renderer)] = &[
    (".clip", render_clip_image),
    (".kra", render_kra_image),
    (".psd", render_psd_image),
    (".psb", render_psd_image),
];

/// The renderer for a format with no specialized entry: the plain renderer.
///
/// The plain renderer decodes JPEG, PNG, WebP, and TIFF.
pub const DEFAULT_RENDERER: Renderer = render_plain_image;

/// Return the renderer for `extension`, a dotted key such as `.png`.
///
/// The lookup ignores case. A key with no dispatch entry returns
/// [`DEFAULT_RENDERER`].
#[must_use]
pub fn renderer_for(extension: &str) -> Renderer {
    let key = extension.to_ascii_lowercase();
    FORMAT_DISPATCH
        .iter()
        .find_map(|(known, renderer)| (*known == key).then_some(*renderer))
        .unwrap_or(DEFAULT_RENDERER)
}

/// Render `path` to an oriented image the cache store and the UI consume.
///
/// The function validates the extension against [`SUPPORTED_EXTENSIONS`], then
/// dispatches to a specialized renderer or the plain default.
///
/// # Errors
///
/// Returns [`RenderError::UnsupportedFormat`] for an unknown extension. The
/// payload is the lower-case dotted extension. When the path has no extension,
/// the payload is the file name instead, so the card shows a clear token.
/// Returns [`RenderError::Io`] when the file cannot be read. Returns
/// [`RenderError::Decode`] when the file cannot be decoded. Returns
/// [`RenderError::Panicked`] when the renderer panics; every format passes the
/// same panic guard here.
pub fn render(path: &Path) -> Result<RenderedImage, RenderError> {
    let Some(extension) = path.extension().and_then(OsStr::to_str) else {
        // No extension means no dotted key. Report the file name, not a bare
        // dot, so the error-state card shows a clear token.
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_owned();
        return Err(RenderError::UnsupportedFormat(name));
    };
    let extension = extension.to_ascii_lowercase();
    if !is_supported_extension(&extension) {
        return Err(RenderError::UnsupportedFormat(format!(".{extension}")));
    }
    let renderer = renderer_for(&format!(".{extension}"));
    run_guarded(path, || renderer(path))
}

#[cfg(test)]
mod tests {
    use image::DynamicImage;

    use crate::clip::render_clip_image;
    use crate::krita::render_kra_image;
    use crate::plain::render_plain_image;
    use crate::psd::render_psd_image;
    use crate::registry::{
        DEFAULT_RENDERER, FORMAT_DISPATCH, SUPPORTED_EXTENSIONS, render, renderer_for,
    };
    use crate::renderer::Renderer;

    #[test]
    fn the_render_gate_accepts_a_supported_fixture() {
        let folder =
            std::env::temp_dir().join(format!("tarragon-render-gate-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("create the temp folder");
        let path = folder.join("photo.png");
        DynamicImage::new_rgba8(4, 3)
            .save(&path)
            .expect("write the fixture");
        let rendered = render(&path).expect("the gate accepted a supported extension");
        assert_eq!((rendered.width(), rendered.height()), (4, 3));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_dispatch_keys_are_supported_lower_case_extensions() {
        for (key, _renderer) in FORMAT_DISPATCH {
            assert!(
                SUPPORTED_EXTENSIONS.contains(key),
                "dispatch key is not supported: {key}"
            );
            assert!(key.starts_with('.'), "dispatch key has no dot: {key}");
            assert_eq!(
                *key,
                key.to_ascii_lowercase(),
                "dispatch key is not lower case"
            );
        }
    }

    #[test]
    fn the_default_renderer_is_the_plain_renderer() {
        assert!(std::ptr::fn_addr_eq(
            DEFAULT_RENDERER,
            render_plain_image as Renderer
        ));
    }

    #[test]
    fn the_clip_format_uses_the_clip_renderer() {
        assert!(std::ptr::fn_addr_eq(
            renderer_for(".clip"),
            render_clip_image as Renderer
        ));
    }

    #[test]
    fn the_kra_format_uses_the_krita_renderer() {
        assert!(std::ptr::fn_addr_eq(
            renderer_for(".kra"),
            render_kra_image as Renderer
        ));
    }

    #[test]
    fn the_photoshop_formats_use_the_psd_renderer() {
        for key in [".psd", ".psb", ".PSD", ".Psb"] {
            assert!(
                std::ptr::fn_addr_eq(renderer_for(key), render_psd_image as Renderer),
                "wrong renderer: {key}"
            );
        }
    }

    #[test]
    fn the_renderer_lookup_ignores_case_and_falls_back() {
        assert!(std::ptr::fn_addr_eq(renderer_for(".PNG"), DEFAULT_RENDERER));
        assert!(std::ptr::fn_addr_eq(
            renderer_for(".unknown"),
            DEFAULT_RENDERER
        ));
    }
}
