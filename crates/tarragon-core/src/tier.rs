//! Cache tier names and their pixel limits.

use core::fmt;
use core::str::FromStr;

use crate::error::CoreError;

/// One cache tier.
///
/// The cache key is also the `thumbnails.tier` column value and the cache
/// directory name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// The 256 px grid thumbnail.
    Thumb256,
    /// The 1024 px preview.
    Preview1024,
    /// The full-resolution render.
    Full,
}

impl Tier {
    /// Every tier, from smallest to largest.
    pub const ALL: [Self; 3] = [Self::Thumb256, Self::Preview1024, Self::Full];

    /// Return the cache key, for example `256` or `full`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thumb256 => "256",
            Self::Preview1024 => "1024",
            Self::Full => "full",
        }
    }

    /// Return the longest-edge limit in pixels for this tier.
    ///
    /// The full tier has no tier limit. The render pass applies the 8192 px
    /// cap. A renderer never upscales.
    #[must_use]
    pub const fn max_edge(self) -> Option<u32> {
        match self {
            Self::Thumb256 => Some(256),
            Self::Preview1024 => Some(1024),
            Self::Full => None,
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Tier {
    type Err = CoreError;

    /// Parse a cache key. The compare ignores case and outer space.
    ///
    /// The aliases `thumb256` and `preview1024` are accepted. The canonical
    /// keys stay `256`, `1024`, and `full`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownTier`] when the key is not a known tier.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "256" | "thumb256" => Ok(Self::Thumb256),
            "1024" | "preview1024" => Ok(Self::Preview1024),
            "full" => Ok(Self::Full),
            other => Err(CoreError::UnknownTier(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use crate::tier::Tier;

    #[test]
    fn tier_maps_to_its_cache_key_and_pixel_limit() {
        assert_eq!(Tier::Thumb256.as_str(), "256");
        assert_eq!(Tier::Thumb256.max_edge(), Some(256));
        assert_eq!(Tier::Preview1024.as_str(), "1024");
        assert_eq!(Tier::Preview1024.max_edge(), Some(1024));
        assert_eq!(Tier::Full.as_str(), "full");
        assert_eq!(Tier::Full.max_edge(), None);
    }

    #[test]
    fn tier_round_trips_through_its_cache_key() {
        for tier in Tier::ALL {
            let parsed = Tier::from_str(tier.as_str()).expect("cache key parses");
            assert_eq!(parsed, tier);
        }
    }

    #[test]
    fn full_tier_has_no_pixel_limit() {
        assert_eq!(Tier::Full.max_edge(), None);
    }

    #[test]
    fn unknown_tier_key_is_refused() {
        let result = Tier::from_str("512");
        assert!(result.is_err(), "unknown tier parsed: {result:?}");
    }

    #[test]
    fn tier_aliases_map_to_their_tier() {
        assert_eq!(
            Tier::from_str("thumb256").expect("thumb256 parses"),
            Tier::Thumb256
        );
        assert_eq!(
            Tier::from_str("preview1024").expect("preview1024 parses"),
            Tier::Preview1024
        );
    }

    #[test]
    fn tier_alias_compare_ignores_case_and_outer_space() {
        assert_eq!(
            Tier::from_str("  Preview1024 ").expect("mixed case parses"),
            Tier::Preview1024
        );
    }

    #[test]
    fn full_key_is_the_full_tier() {
        assert_eq!(Tier::from_str("full").expect("full parses"), Tier::Full);
    }
}
