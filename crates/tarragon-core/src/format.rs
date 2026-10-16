//! The ten image extensions shared by the scanner and the render registry.
//!
//! Q26 fixes the set. The scanner indexes a file only when its extension is in
//! the set. The render registry dispatches only the same set. One definition
//! here keeps the two from drifting.

use std::path::Path;

/// The ten indexed image extensions, lower case, each with its leading dot
/// (Q26).
pub const SUPPORTED_EXTENSIONS: [&str; 10] = [
    ".jpg", ".jpeg", ".png", ".webp", ".tiff", ".tif", ".psd", ".psb", ".clip", ".kra",
];

/// Return true when `path` has one of the ten extensions, ignoring case.
///
/// A hidden file is not special: `.hidden.png` keeps the `.png` extension, so
/// it is supported. A name with no extension, or a leading-dot name like
/// `.jpg`, is not supported. A name whose extension is not UTF-8 is not
/// supported.
#[must_use]
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(is_supported_extension)
}

/// Return true when `extension` (without the dot) is one of the ten.
///
/// The compare ignores case.
#[must_use]
pub fn is_supported_extension(extension: &str) -> bool {
    SUPPORTED_EXTENSIONS.iter().any(|known| {
        known
            .strip_prefix('.')
            .is_some_and(|tail| tail.eq_ignore_ascii_case(extension))
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::format::{SUPPORTED_EXTENSIONS, is_supported, is_supported_extension};

    #[test]
    fn the_set_holds_the_ten_q26_extensions() {
        let expected = [
            ".jpg", ".jpeg", ".png", ".webp", ".tiff", ".tif", ".psd", ".psb", ".clip", ".kra",
        ];
        assert_eq!(SUPPORTED_EXTENSIONS.len(), expected.len());
        for extension in expected {
            assert!(
                SUPPORTED_EXTENSIONS.contains(&extension),
                "missing extension: {extension}"
            );
        }
    }

    #[test]
    fn an_extension_matches_in_both_cases() {
        for extension in SUPPORTED_EXTENSIONS {
            let tail = extension.trim_start_matches('.');
            assert!(is_supported_extension(tail), "lower case refused: {tail}");
            let upper = tail.to_uppercase();
            assert!(
                is_supported_extension(&upper),
                "upper case refused: {upper}"
            );
        }
    }

    #[test]
    fn unknown_and_bare_names_are_refused() {
        for name in ["notes.txt", "readme.md", "jpg", "", ".jpg"] {
            assert!(!is_supported(Path::new(name)), "name passed: {name}");
        }
    }

    #[test]
    fn every_supported_extension_is_supported_by_path() {
        for extension in SUPPORTED_EXTENSIONS {
            assert!(
                is_supported(Path::new(&format!("image{extension}"))),
                "refused a supported extension: {extension}"
            );
        }
    }
}
