//! Parse limits: the full-tier cap, the deadline, and the parse options.
//!
//! The two limits come from the Python renderer (Q9). The full-tier render
//! caps the long edge at [`FULL_TIER_MAX_EDGE`] pixels. The whole render has a
//! [`PARSE_DEADLINE_SECS`] second deadline. A deadline that is already expired
//! returns [`PsdError::Timeout`] before any pixel work.

use std::time::{Duration, Instant};

use crate::error::PsdError;

/// The full-tier long-edge cap in pixels (Q9).
pub const FULL_TIER_MAX_EDGE: u32 = 8192;

/// The whole-parse budget in seconds, matching the Python renderer (Q9).
pub const PARSE_DEADLINE_SECS: u64 = 120;

/// The Adobe PSD and PSB dimension limit in pixels.
pub const MAX_DIMENSION: u32 = 300_000;

/// A cooperative parse deadline.
///
/// The reader checks the deadline per row and per channel. A live wall clock
/// is intentional, because the work runs on a worker thread.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    /// The instant the parse must stop.
    end: Instant,
}

impl Deadline {
    /// Build a deadline that expires after `limit`.
    #[must_use]
    pub fn after(limit: Duration) -> Self {
        let end = Instant::now()
            .checked_add(limit)
            .unwrap_or_else(Instant::now);
        Self { end }
    }

    /// Build a deadline that expires at `end`.
    #[must_use]
    pub const fn at(end: Instant) -> Self {
        Self { end }
    }

    /// Return true when the deadline has passed.
    #[must_use]
    pub fn expired(&self) -> bool {
        Instant::now() >= self.end
    }
}

/// The options for a parse or a region read.
#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    /// The cooperative deadline. `None` means no deadline.
    pub deadline: Option<Deadline>,
}

impl ParseOptions {
    /// Options with no deadline.
    #[must_use]
    pub const fn none() -> Self {
        Self { deadline: None }
    }

    /// Options with a deadline `seconds` from now.
    #[must_use]
    pub fn in_seconds(seconds: u64) -> Self {
        Self {
            deadline: Some(Deadline::after(Duration::from_secs(seconds))),
        }
    }

    /// Options with an explicit deadline.
    #[must_use]
    pub const fn with_deadline(deadline: Deadline) -> Self {
        Self {
            deadline: Some(deadline),
        }
    }

    /// Return [`PsdError::Timeout`] when the deadline has passed.
    ///
    /// # Errors
    ///
    /// Returns [`PsdError::Timeout`] when the deadline has passed.
    pub fn check(&self) -> Result<(), PsdError> {
        match self.deadline {
            Some(deadline) if deadline.expired() => Err(PsdError::Timeout),
            _ => Ok(()),
        }
    }
}

impl Default for ParseOptions {
    /// The default options use the 120 s deadline (Q9).
    fn default() -> Self {
        Self::in_seconds(PARSE_DEADLINE_SECS)
    }
}

/// Clamp a canvas to the full-tier cap. The long edge never exceeds the cap.
#[must_use]
pub fn full_tier_size(width: u32, height: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= FULL_TIER_MAX_EDGE {
        return (width, height);
    }
    let cap = u64::from(FULL_TIER_MAX_EDGE);
    let scale = |value: u32| {
        let scaled = u64::from(value) * cap / u64::from(long);
        u32::try_from(scaled).unwrap_or(FULL_TIER_MAX_EDGE).max(1)
    };
    (scale(width), scale(height))
}

#[cfg(test)]
mod tests {
    use super::{FULL_TIER_MAX_EDGE, ParseOptions, full_tier_size};
    use crate::error::PsdError;

    #[test]
    fn a_small_canvas_keeps_its_size() {
        assert_eq!(full_tier_size(640, 480), (640, 480));
        assert_eq!(full_tier_size(8192, 100), (8192, 100));
    }

    #[test]
    fn a_large_canvas_is_capped_on_the_long_edge() {
        assert_eq!(full_tier_size(9000, 9000), (8192, 8192));
        assert_eq!(full_tier_size(16000, 8000), (8192, 4096));
        assert_eq!(full_tier_size(8000, 16000), (4096, 8192));
    }

    #[test]
    fn the_default_options_use_the_cap_constant() {
        assert_eq!(FULL_TIER_MAX_EDGE, 8192);
        let options = ParseOptions::default();
        assert!(options.deadline.is_some());
    }

    #[test]
    fn an_expired_deadline_is_a_typed_timeout() {
        let expired =
            ParseOptions::with_deadline(super::Deadline::after(std::time::Duration::from_secs(0)));
        assert!(matches!(expired.check(), Err(PsdError::Timeout)));
        assert!(ParseOptions::none().check().is_ok());
    }
}
