//! Rules for the names of user tags.

use crate::color::{ColorBucket, TRANSPARENT_TAG};

/// The most characters a tag name may have.
pub const MAX_TAG_NAME_CHARS: usize = 64;

/// A reason a tag name is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TagNameError {
    /// The name has no characters after trimming.
    #[error("Enter a tag name")]
    Empty,
    /// The name has more than [`MAX_TAG_NAME_CHARS`] characters.
    #[error("Name is longer than 64 characters")]
    TooLong,
    /// The name is a color bucket name or `transparent`.
    #[error("Name is reserved")]
    Reserved,
}

/// Trim a raw tag name and check it.
///
/// Length counts `char`s, not bytes. The case stays as typed, so `Cat` and
/// `cat` are two names.
///
/// # Errors
///
/// Returns [`TagNameError`] when the trimmed name is empty, too long, or
/// reserved.
pub fn validate_tag_name(raw: &str) -> Result<String, TagNameError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(TagNameError::Empty);
    }
    if name.chars().count() > MAX_TAG_NAME_CHARS {
        return Err(TagNameError::TooLong);
    }
    if is_reserved_tag_name(name) {
        return Err(TagNameError::Reserved);
    }
    Ok(name.to_owned())
}

/// Tell whether a name belongs to the app: a color bucket or `transparent`.
///
/// The match ignores ASCII case, so `RED` is reserved too. The name is not
/// trimmed.
#[must_use]
pub fn is_reserved_tag_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(TRANSPARENT_TAG)
        || ColorBucket::ALL
            .iter()
            .any(|bucket| name.eq_ignore_ascii_case(bucket.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_rejects_empty() {
        assert_eq!(validate_tag_name("  cat  "), Ok("cat".to_owned()));
        assert_eq!(validate_tag_name(""), Err(TagNameError::Empty));
        assert_eq!(validate_tag_name("   "), Err(TagNameError::Empty));
    }

    #[test]
    fn length_counts_chars_not_bytes() {
        assert!(validate_tag_name(&"a".repeat(64)).is_ok());
        assert_eq!(
            validate_tag_name(&"a".repeat(65)),
            Err(TagNameError::TooLong)
        );
        assert!(validate_tag_name(&"é".repeat(64)).is_ok());
        assert_eq!(
            validate_tag_name(&"é".repeat(65)),
            Err(TagNameError::TooLong)
        );
    }

    #[test]
    fn reserved_names_are_refused_in_any_case() {
        let mut names: Vec<&str> = ColorBucket::ALL.iter().map(|b| b.as_str()).collect();
        names.push(TRANSPARENT_TAG);
        assert_eq!(names.len(), 12);
        for name in names {
            let upper = name.to_ascii_uppercase();
            let padded = format!(" {name} ");
            for variant in [name, upper.as_str(), padded.as_str()] {
                assert_eq!(
                    validate_tag_name(variant),
                    Err(TagNameError::Reserved),
                    "{variant:?}"
                );
            }
        }
        assert_eq!(validate_tag_name("Red"), Err(TagNameError::Reserved));
    }

    #[test]
    fn case_is_kept() {
        assert_eq!(validate_tag_name("Cat"), Ok("Cat".to_owned()));
        assert_eq!(validate_tag_name("cat"), Ok("cat".to_owned()));
    }

    #[test]
    fn only_the_exact_name_is_reserved() {
        assert!(validate_tag_name("reddish").is_ok());
        assert!(validate_tag_name("red tag").is_ok());
        assert!(!is_reserved_tag_name("reddish"));
    }
}
