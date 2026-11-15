//! Frame-time recording and the percentile report for the smoke run.
//!
//! The recorder reads no clock: the caller passes each frame's `Instant`, so
//! a test can feed exact gaps.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// A report needs at least this many recorded frame times; fewer make the
/// percentiles meaningless.
const MIN_FRAMES: usize = 100;
/// A p99 at or over this many printed milliseconds fails the run.
const P99_LIMIT_MS: f64 = 33.0;
/// A frame time over this many printed milliseconds fails the run, whatever
/// the p99 is.
const MAX_LIMIT_MS: f64 = 100.0;

/// Where a recorder is in its run.
#[cfg_attr(not(test), expect(dead_code, reason = "no caller yet"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Frames still inside the warm-up; none is recorded.
    Warmup,
    /// Frame times are being recorded.
    Recording,
    /// The recorded span reached the run length.
    Done,
}

/// Turns frame timestamps into a list of frame times.
#[cfg_attr(not(test), expect(dead_code, reason = "no caller yet"))]
#[derive(Debug)]
pub struct FrameRecorder {
    /// Frames before this offset from the first frame are not recorded.
    warmup: Duration,
    /// The recorded span at which the run ends.
    run: Duration,
    /// The first frame, which is time zero.
    start: Option<Instant>,
    /// The previous frame, once the warm-up is over.
    last: Option<Instant>,
    /// Recorded frame times, in arrival order.
    gaps: Vec<Duration>,
    /// The sum of `gaps`.
    span: Duration,
}

#[cfg_attr(not(test), expect(dead_code, reason = "no caller yet"))]
impl FrameRecorder {
    /// Make a recorder that skips `warmup` and then records for `run`.
    pub fn new(warmup: Duration, run: Duration) -> Self {
        Self {
            warmup,
            run,
            start: None,
            last: None,
            gaps: Vec::new(),
            span: Duration::ZERO,
        }
    }

    /// Take one frame at `now`. Frames after `Done` are not recorded.
    pub fn frame(&mut self, now: Instant) -> Phase {
        if self.span >= self.run && !self.gaps.is_empty() {
            return Phase::Done;
        }
        let start = *self.start.get_or_insert(now);
        if now.saturating_duration_since(start) < self.warmup {
            return Phase::Warmup;
        }
        // The first frame after the warm-up only sets the baseline: its gap
        // would reach back into the warm-up.
        let Some(previous) = self.last.replace(now) else {
            return Phase::Recording;
        };
        let gap = now.saturating_duration_since(previous);
        self.gaps.push(gap);
        self.span += gap;
        if self.span >= self.run {
            Phase::Done
        } else {
            Phase::Recording
        }
    }

    /// Summarise the recorded frame times, or `None` when there are too few.
    pub fn report(&self) -> Option<FrameReport> {
        if self.gaps.len() < MIN_FRAMES {
            return None;
        }
        let mut sorted = self.gaps.clone();
        sorted.sort_unstable();
        Some(FrameReport {
            count: sorted.len(),
            p50: nearest_rank(&sorted, 50),
            p95: nearest_rank(&sorted, 95),
            p99: nearest_rank(&sorted, 99),
            max: sorted[sorted.len() - 1],
            span: self.span,
        })
    }
}

/// The value at percentile `p` of non-empty sorted values, by nearest rank.
/// Integer math keeps the rank exact.
fn nearest_rank(sorted: &[Duration], p: usize) -> Duration {
    let rank = (p * sorted.len()).div_ceil(100);
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// The pass or fail result of a run.
#[cfg_attr(not(test), expect(dead_code, reason = "no caller yet"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Both limits hold.
    Pass,
    /// The p99 or the max is over its limit.
    Fail,
}

/// The statistics of one run.
#[cfg_attr(not(test), expect(dead_code, reason = "no caller yet"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameReport {
    /// The number of recorded frame times.
    pub count: usize,
    /// The median frame time.
    pub p50: Duration,
    /// The 95th percentile frame time.
    pub p95: Duration,
    /// The 99th percentile frame time.
    pub p99: Duration,
    /// The longest frame time.
    pub max: Duration,
    /// The sum of the recorded frame times.
    pub span: Duration,
}

#[cfg_attr(not(test), expect(dead_code, reason = "no caller yet"))]
impl FrameReport {
    /// Fail when the p99 is 33 ms or more, or the max is over 100 ms. The
    /// max check catches one long stall that the p99 hides. It judges the
    /// printed two-decimal values, so the verdict never contradicts the text.
    pub fn verdict(&self) -> Verdict {
        if ms(self.p99) >= P99_LIMIT_MS || ms(self.max) > MAX_LIMIT_MS {
            Verdict::Fail
        } else {
            Verdict::Pass
        }
    }

    /// One `key: value` per line, for a log a script can read.
    pub fn lines(&self) -> String {
        let word = match self.verdict() {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
        };
        let mut text = String::new();
        let _ = writeln!(text, "frames: {}", self.count);
        let _ = writeln!(text, "span_s: {:.2}", floor2(self.span.as_secs_f64()));
        let _ = writeln!(text, "p50_ms: {:.2}", ms(self.p50));
        let _ = writeln!(text, "p95_ms: {:.2}", ms(self.p95));
        let _ = writeln!(text, "p99_ms: {:.2}", ms(self.p99));
        let _ = writeln!(text, "max_ms: {:.2}", ms(self.max));
        let _ = writeln!(text, "frame_verdict: {word}");
        text
    }
}

/// A duration in milliseconds, cut to two decimals. Cutting instead of
/// rounding keeps a value under a limit from printing as the limit.
fn ms(value: Duration) -> f64 {
    floor2(value.as_secs_f64() * 1000.0)
}

/// Cut a value to two decimals; the small term absorbs float error.
fn floor2(value: f64) -> f64 {
    (value * 100.0 + 1e-6).floor() / 100.0
}

#[cfg(test)]
mod tests;
