//! Scanned-file and tag entities.

use std::path::PathBuf;

use core::fmt;
use core::str::FromStr;

use crate::error::CoreError;

/// File metadata collected at scan time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// Absolute path of the file.
    pub path: PathBuf,
    /// File size in bytes.
    pub size: u64,
    /// Last-modified time in epoch seconds.
    pub mtime: i64,
    /// Pixel width. Zero means unknown.
    pub width: u32,
    /// Pixel height. Zero means unknown.
    pub height: u32,
}

impl FileInfo {
    /// Return the file name as text.
    ///
    /// A non-UTF-8 name gives `None`.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.path.file_name().and_then(|name| name.to_str())
    }
}

/// Where a tag came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TagSource {
    /// A tag the user made.
    User,
    /// A tag the color-tag pipeline made. The name is a bare color bucket.
    AutoColor,
}

impl TagSource {
    /// Every source.
    pub const ALL: [Self; 2] = [Self::User, Self::AutoColor];

    /// Return the storage name, for example `user` or `auto_color`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::AutoColor => "auto_color",
        }
    }
}

impl fmt::Display for TagSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for TagSource {
    type Err = CoreError;

    /// Parse a tag source name. The compare ignores case and outer space.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownTagSource`] when the name is unknown.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "user" => Ok(Self::User),
            "auto_color" => Ok(Self::AutoColor),
            other => Err(CoreError::UnknownTagSource(other.to_owned())),
        }
    }
}

/// A tag row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// Database row id.
    pub id: i64,
    /// Tag name. A color tag uses the bare bucket name.
    pub name: String,
    /// Where the tag came from.
    pub source: TagSource,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::entity::{FileInfo, Tag, TagSource};

    #[test]
    fn file_info_name_reads_the_last_path_part() {
        let info = FileInfo {
            path: PathBuf::from("/refs/night/sky.png"),
            size: 10,
            mtime: 5,
            width: 4,
            height: 3,
        };
        assert_eq!(info.name(), Some("sky.png"));
    }

    #[test]
    fn tag_source_round_trips_through_its_name() {
        for source in TagSource::ALL {
            let parsed = source.as_str().parse::<TagSource>().expect("name parses");
            assert_eq!(parsed, source);
        }
    }

    #[test]
    fn unknown_tag_source_name_is_refused() {
        let result = "auto".parse::<TagSource>();
        assert!(result.is_err(), "unknown source parsed: {result:?}");
    }

    #[test]
    fn tag_carries_id_name_and_source() {
        let tag = Tag {
            id: 7,
            name: "red".to_owned(),
            source: TagSource::AutoColor,
        };
        assert_eq!(tag.id, 7);
        assert_eq!(tag.name, "red");
        assert_eq!(tag.source, TagSource::AutoColor);
    }
}
