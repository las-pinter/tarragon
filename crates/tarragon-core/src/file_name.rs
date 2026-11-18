//! Rules for the names of image files the app writes.
//!
//! The functions are pure. They do not touch the disk. The caller passes a
//! closure that says whether a name is already used.

/// The most bytes a whole file name may have.
pub const MAX_FILE_NAME_BYTES: usize = 255;

/// A reason a new file stem is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FileNameError {
    /// The stem has no characters.
    #[error("Enter a file name")]
    Empty,
    /// The stem has a slash, a backslash, or a NUL character.
    #[error("Name cannot contain / \\ or NUL")]
    ForbiddenChar,
    /// The stem is `.` or `..`.
    #[error("Name cannot be . or ..")]
    DotName,
    /// The stem starts or ends with a space or a dot.
    #[error("Name cannot start or end with a space or a dot")]
    EdgeSpaceOrDot,
    /// The whole name has more than [`MAX_FILE_NAME_BYTES`] bytes.
    #[error("Name is longer than 255 bytes")]
    TooLong,
    /// Another file already has the whole name.
    #[error("Name already used")]
    Taken,
}

/// Split a name into its stem and extension, without the dot.
///
/// The extension starts at the last dot. A dot at the start of the name does
/// not start an extension, so `.hidden` is all stem. A name that ends in a dot
/// has an empty extension, so `a.` gives the stem `a` and the extension `""`.
#[must_use]
pub fn split_file_name(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(index) if index > 0 => (&name[..index], Some(&name[index + 1..])),
        _ => (name, None),
    }
}

/// Join a stem and an optional extension into a whole name.
fn join_name(stem: &str, extension: Option<&str>) -> String {
    match extension {
        Some(extension) => format!("{stem}.{extension}"),
        None => stem.to_owned(),
    }
}

/// Give the first free copy name for a file name.
///
/// The names run `stem copy.ext`, `stem copy 2.ext`, `stem copy 3.ext`, and
/// so on. `taken` says whether a name is already used. A name with no
/// extension gives `stem copy`. A name that starts with a dot and has no other
/// dot keeps the whole name as the stem.
pub fn copy_name(name: &str, mut taken: impl FnMut(&str) -> bool) -> String {
    let (stem, extension) = split_file_name(name);
    let mut number: u64 = 1;
    loop {
        let candidate = if number == 1 {
            join_name(&format!("{stem} copy"), extension)
        } else {
            join_name(&format!("{stem} copy {number}"), extension)
        };
        if !taken(&candidate) {
            return candidate;
        }
        number += 1;
    }
}

/// Check a new file stem for a file with the given extension.
///
/// `extension` has no dot. An empty `extension` means the file has none. The
/// length rule counts the whole name in bytes. `taken` gets the whole name and
/// says whether another file has it. The match is exact.
///
/// # Errors
///
/// Returns [`FileNameError`] for the first rule the stem breaks, in this
/// order: empty, forbidden character, `.` or `..`, space or dot at an edge,
/// too long, taken.
pub fn validate_file_stem(
    stem: &str,
    extension: &str,
    mut taken: impl FnMut(&str) -> bool,
) -> Result<(), FileNameError> {
    if stem.is_empty() {
        return Err(FileNameError::Empty);
    }
    if stem.contains(['/', '\\', '\0']) {
        return Err(FileNameError::ForbiddenChar);
    }
    if stem == "." || stem == ".." {
        return Err(FileNameError::DotName);
    }
    if stem.starts_with([' ', '.']) || stem.ends_with([' ', '.']) {
        return Err(FileNameError::EdgeSpaceOrDot);
    }
    let extension = (!extension.is_empty()).then_some(extension);
    let name = join_name(stem, extension);
    if name.len() > MAX_FILE_NAME_BYTES {
        return Err(FileNameError::TooLong);
    }
    if taken(&name) {
        return Err(FileNameError::Taken);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none_taken(_: &str) -> bool {
        false
    }

    #[test]
    fn split_file_name_splits_at_the_last_dot_after_the_first_byte() {
        assert_eq!(split_file_name(".hidden"), (".hidden", None));
        assert_eq!(split_file_name("a."), ("a", Some("")));
        assert_eq!(split_file_name("a.tar.gz"), ("a.tar", Some("gz")));
        assert_eq!(split_file_name("a.png"), ("a", Some("png")));
    }

    #[test]
    fn copy_name_counts_up_past_taken_names() {
        assert_eq!(copy_name("a.png", none_taken), "a copy.png");
        assert_eq!(copy_name("a.png", |n| n == "a copy.png"), "a copy 2.png");
        assert_eq!(
            copy_name("a.png", |n| n == "a copy.png" || n == "a copy 2.png"),
            "a copy 3.png"
        );
    }

    #[test]
    fn copy_name_without_extension() {
        assert_eq!(copy_name("notes", none_taken), "notes copy");
        assert_eq!(copy_name("notes", |n| n == "notes copy"), "notes copy 2");
    }

    #[test]
    fn copy_name_keeps_a_dot_leading_name_as_stem() {
        assert_eq!(copy_name(".hidden", none_taken), ".hidden copy");
        assert_eq!(
            copy_name(".hidden", |n| n == ".hidden copy"),
            ".hidden copy 2"
        );
        assert_eq!(copy_name(".hidden.png", none_taken), ".hidden copy.png");
    }

    #[test]
    fn copy_name_splits_at_the_last_dot() {
        assert_eq!(copy_name("a.b.png", none_taken), "a.b copy.png");
    }

    #[test]
    fn empty_stem_is_refused() {
        assert_eq!(
            validate_file_stem("", "png", none_taken),
            Err(FileNameError::Empty)
        );
        assert_eq!(validate_file_stem("a", "png", none_taken), Ok(()));
    }

    #[test]
    fn slash_backslash_and_nul_are_refused() {
        for stem in ["a/b", "a\\b", "a\0b"] {
            assert_eq!(
                validate_file_stem(stem, "png", none_taken),
                Err(FileNameError::ForbiddenChar),
                "{stem:?}"
            );
        }
        assert_eq!(validate_file_stem("a-b_c", "png", none_taken), Ok(()));
    }

    #[test]
    fn dot_and_dot_dot_are_refused() {
        for stem in [".", ".."] {
            assert_eq!(
                validate_file_stem(stem, "", none_taken),
                Err(FileNameError::DotName),
                "{stem:?}"
            );
        }
        assert_eq!(validate_file_stem("a.b", "", none_taken), Ok(()));
    }

    #[test]
    fn leading_and_trailing_space_or_dot_are_refused() {
        for stem in [" a", "a ", ".a", "a."] {
            assert_eq!(
                validate_file_stem(stem, "png", none_taken),
                Err(FileNameError::EdgeSpaceOrDot),
                "{stem:?}"
            );
        }
        assert_eq!(validate_file_stem("a b.c", "png", none_taken), Ok(()));
    }

    #[test]
    fn length_counts_bytes_of_the_whole_name() {
        // "é" has two bytes. 126 of them and ".png" make 256 bytes.
        let long = "é".repeat(126);
        assert_eq!(
            validate_file_stem(&long, "png", none_taken),
            Err(FileNameError::TooLong)
        );
        // 125 of them and ".png" make 254 bytes. 125 plus "a" make 255.
        let fits = "é".repeat(125);
        assert_eq!(validate_file_stem(&fits, "png", none_taken), Ok(()));
        assert_eq!(
            validate_file_stem(&format!("{fits}a"), "png", none_taken),
            Ok(())
        );
        assert_eq!(
            validate_file_stem(&format!("{fits}aa"), "png", none_taken),
            Err(FileNameError::TooLong)
        );
        // 127 chars of two bytes are 254 bytes with no extension.
        assert_eq!(validate_file_stem(&"é".repeat(127), "", none_taken), Ok(()));
    }

    #[test]
    fn taken_whole_name_is_refused() {
        assert_eq!(
            validate_file_stem("a", "png", |n| n == "a.png"),
            Err(FileNameError::Taken)
        );
        assert_eq!(
            validate_file_stem("a", "", |n| n == "a"),
            Err(FileNameError::Taken)
        );
        assert_eq!(validate_file_stem("a", "png", |n| n == "A.png"), Ok(()));
    }

    #[test]
    fn error_text_is_shown_to_the_user() {
        assert_eq!(FileNameError::Empty.to_string(), "Enter a file name");
        assert_eq!(
            FileNameError::ForbiddenChar.to_string(),
            "Name cannot contain / \\ or NUL"
        );
        assert_eq!(FileNameError::DotName.to_string(), "Name cannot be . or ..");
        assert_eq!(
            FileNameError::EdgeSpaceOrDot.to_string(),
            "Name cannot start or end with a space or a dot"
        );
        assert_eq!(
            FileNameError::TooLong.to_string(),
            "Name is longer than 255 bytes"
        );
        assert_eq!(FileNameError::Taken.to_string(), "Name already used");
    }
}
