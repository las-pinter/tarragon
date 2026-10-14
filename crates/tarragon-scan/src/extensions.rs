//! The ten indexed image extensions from Q26.
//!
//! Extension matching ignores case (Q26). A hidden file is not special: the
//! name `.hidden.png` keeps the `.png` extension.

use std::path::Path;

/// The indexed image extensions, lower case, each with its leading dot (Q26).
pub const SUPPORTED_EXTENSIONS: [&str; 10] = [
    ".jpg", ".jpeg", ".png", ".webp", ".tiff", ".tif", ".psd", ".psb", ".clip", ".kra",
];

/// Return true when `path` has one of the ten extensions, ignoring case.
///
/// A name with no extension, or a leading-dot name like `.jpg`, is not
/// supported. A name whose extension is not UTF-8 is not supported.
#[must_use]
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_EXTENSIONS.iter().any(|known| {
                known
                    .strip_prefix('.')
                    .is_some_and(|tail| tail.eq_ignore_ascii_case(extension))
            })
        })
}
