//! Parse limits: the full-tier cap, the deadline, and the parse options.
//!
//! The full-tier render caps the long edge at [`FULL_TIER_MAX_EDGE`] pixels.
//! The whole render has a [`PARSE_DEADLINE_SECS`] second deadline. A deadline
//! that is already expired returns [`PsdError::Timeout`] before any pixel
//! work. A cancel flag stops the work early with [`PsdError::Cancelled`].

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::error::PsdError;

/// The full-tier long-edge cap in pixels.
pub const FULL_TIER_MAX_EDGE: u32 = 8192;

/// The whole-parse budget in seconds.
pub const PARSE_DEADLINE_SECS: u64 = 120;

/// The Adobe PSD and PSB dimension limit in pixels.
pub const MAX_DIMENSION: u32 = 300_000;

/// The deepest group nesting the compositor accepts.
///
/// The tree build and the compositor walk recurse once per level, so the cap
/// keeps the stack use fixed whatever the file declares.
pub const MAX_GROUP_DEPTH: usize = 32;

/// The memory budget of one composite call, in bytes.
///
/// The estimate is [`COMPOSITE_BASE_BYTES_PER_PIXEL`] plus
/// [`GROUP_BYTES_PER_PIXEL_LEVEL`] for each group level, plus the effects
/// terms when any layer or group has effects, times the working pixels. Nested
/// nodes with effects pad the working size again. A call above the budget returns [`PsdError::OverBudget`] before it
/// allocates. The budget keeps one render from exhausting the machine.
pub const MAX_COMPOSITE_BYTES: u64 = 1 << 30;

/// The bytes per pixel that one group nesting level holds.
///
/// A level holds up to ten `f32` planes: the backdrop color (three) and alpha,
/// the running color (three) and alpha, the coverage, and the group alpha. A
/// group with effects builds those as one fresh state, which is the largest
/// case.
pub const GROUP_BYTES_PER_PIXEL_LEVEL: u64 = 40;

/// The bytes per pixel that every render holds, with or without groups.
///
/// One painted layer holds six `f32` planes (24 bytes) and the output region
/// holds four (16 bytes). The remaining 16 bytes allow for temporary copies.
pub const COMPOSITE_BASE_BYTES_PER_PIXEL: u64 = 56;

/// The bytes per padded pixel that the effects path holds beyond the base.
///
/// One layer with effects holds, at the padded size: the layer read (12), the
/// layer paint (24), the effect shape (4), up to sixteen transient `f32`
/// planes while one effect map is built (64), and a copy of the running state
/// with the stack planes (48). That is 152 bytes, rounded up to 160.
pub const EFFECTS_BYTES_PER_PIXEL: u64 = 160;

/// The bytes per padded pixel that each held paint takes.
///
/// Every exterior paint stays alive until the layer composites: three `f32`
/// color planes and one coverage plane. A node holds the paints of its drop
/// shadows, outer glows, outside strokes, and bevels (two each); interior
/// effects hold none.
pub const EFFECT_PAINT_BYTES_PER_PIXEL: u64 = 16;

/// The most channels a layer record may declare.
pub const MAX_LAYER_CHANNELS: usize = 64;

/// The most channels the format allows in the file header.
pub const MAX_HEADER_CHANNELS: usize = 56;

/// A cooperative parse deadline.
///
/// The reader checks the deadline per row, per channel, and per effect pass. A
/// live wall clock is intentional, because the work runs on a worker thread.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    /// The instant the parse must stop, or `None` when the limit is too large
    /// to represent and so never expires.
    end: Option<Instant>,
}

impl Deadline {
    /// Build a deadline that expires after `limit`.
    #[must_use]
    pub fn after(limit: Duration) -> Self {
        Self {
            end: Instant::now().checked_add(limit),
        }
    }

    /// Build a deadline that expires at `end`.
    #[must_use]
    pub const fn at(end: Instant) -> Self {
        Self { end: Some(end) }
    }

    /// Return true when the deadline has passed.
    #[must_use]
    pub fn expired(&self) -> bool {
        self.end.is_some_and(|end| Instant::now() >= end)
    }
}

/// The options for a parse or a region read.
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// The cooperative deadline. `None` means no deadline.
    pub deadline: Option<Deadline>,
    /// The cooperative cancel flag. `None` means the work cannot be cancelled.
    pub cancel: Option<Arc<AtomicBool>>,
}

impl ParseOptions {
    /// Options with no deadline.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            deadline: None,
            cancel: None,
        }
    }

    /// Options with a deadline `seconds` from now.
    #[must_use]
    pub fn in_seconds(seconds: u64) -> Self {
        Self {
            deadline: Some(Deadline::after(Duration::from_secs(seconds))),
            cancel: None,
        }
    }

    /// Options with an explicit deadline.
    #[must_use]
    pub const fn with_deadline(deadline: Deadline) -> Self {
        Self {
            deadline: Some(deadline),
            cancel: None,
        }
    }

    /// Return these options with a cancel flag. Setting the flag stops the work
    /// at the next check.
    #[must_use]
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Return an error when the work was cancelled or the deadline has passed.
    ///
    /// # Errors
    ///
    /// Returns [`PsdError::Cancelled`] when the cancel flag is set. Returns
    /// [`PsdError::Timeout`] when the deadline has passed.
    pub fn check(&self) -> Result<(), PsdError> {
        if self
            .cancel
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(PsdError::Cancelled);
        }
        match self.deadline {
            Some(deadline) if deadline.expired() => Err(PsdError::Timeout),
            _ => Ok(()),
        }
    }
}

impl Default for ParseOptions {
    /// The default options use the 120 s deadline.
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::{Deadline, FULL_TIER_MAX_EDGE, ParseOptions, full_tier_size};
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

    #[test]
    fn a_set_cancel_flag_is_a_typed_cancel() {
        let flag = Arc::new(AtomicBool::new(false));
        let options = ParseOptions::none().with_cancel(Arc::clone(&flag));
        assert!(options.check().is_ok());
        flag.store(true, Ordering::Relaxed);
        assert!(matches!(options.check(), Err(PsdError::Cancelled)));
    }

    #[test]
    fn a_deadline_too_far_away_never_expires() {
        let deadline = Deadline::after(Duration::from_secs(u64::MAX));
        assert!(!deadline.expired());
        assert!(ParseOptions::in_seconds(u64::MAX).check().is_ok());
    }
}
