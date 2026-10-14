//! Dominant-color buckets used by the color filter and the color tags.

use core::fmt;
use core::str::FromStr;

use crate::error::CoreError;

/// A dominant-color bucket.
///
/// The name is bare, for example `red`. The app never writes `color:red`
/// (Q20). The set matches the Python app: red, orange, yellow, green, teal,
/// cyan, blue, purple, magenta, and neutral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColorBucket {
    /// Red hues.
    Red,
    /// Orange hues.
    Orange,
    /// Yellow hues.
    Yellow,
    /// Green hues.
    Green,
    /// Teal hues.
    Teal,
    /// Cyan hues.
    Cyan,
    /// Blue hues.
    Blue,
    /// Purple hues.
    Purple,
    /// Magenta hues.
    Magenta,
    /// Low-saturation colors.
    Neutral,
}

impl ColorBucket {
    /// Every bucket, in the canonical order.
    pub const ALL: [Self; 10] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Teal,
        Self::Cyan,
        Self::Blue,
        Self::Purple,
        Self::Magenta,
        Self::Neutral,
    ];

    /// Return the bare bucket name, for example `red`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Teal => "teal",
            Self::Cyan => "cyan",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Magenta => "magenta",
            Self::Neutral => "neutral",
        }
    }
}

impl fmt::Display for ColorBucket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ColorBucket {
    type Err = CoreError;

    /// Parse a bare bucket name. The compare ignores case and outer space.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownColorBucket`] when the name is not one of
    /// the ten buckets.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "red" => Ok(Self::Red),
            "orange" => Ok(Self::Orange),
            "yellow" => Ok(Self::Yellow),
            "green" => Ok(Self::Green),
            "teal" => Ok(Self::Teal),
            "cyan" => Ok(Self::Cyan),
            "blue" => Ok(Self::Blue),
            "purple" => Ok(Self::Purple),
            "magenta" => Ok(Self::Magenta),
            "neutral" => Ok(Self::Neutral),
            other => Err(CoreError::UnknownColorBucket(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use crate::color::ColorBucket;

    #[test]
    fn bucket_round_trips_through_its_bare_name() {
        for bucket in ColorBucket::ALL {
            let text = bucket.to_string();
            assert_eq!(text, bucket.as_str());
            let parsed = ColorBucket::from_str(&text).expect("bare name parses");
            assert_eq!(parsed, bucket);
        }
    }

    #[test]
    fn bucket_names_are_bare_and_never_prefixed() {
        let text = ColorBucket::Red.to_string();
        assert_eq!(text, "red");
        assert!(
            !text.contains("color:"),
            "bucket name carries a prefix: {text}"
        );
    }

    #[test]
    fn bucket_parse_ignores_case_and_outer_space() {
        let parsed = ColorBucket::from_str("  BLUE ").expect("mixed case parses");
        assert_eq!(parsed, ColorBucket::Blue);
    }

    #[test]
    fn unknown_bucket_name_is_refused() {
        let result = ColorBucket::from_str("chartreuse");
        assert!(result.is_err(), "unknown bucket parsed: {result:?}");
    }

    #[test]
    fn prefixed_bucket_name_is_refused() {
        let result = ColorBucket::from_str("color:red");
        assert!(result.is_err(), "prefixed name parsed: {result:?}");
    }
}
