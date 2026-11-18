//! An in-memory store of the newest log lines, filled by a tracing layer.

use std::collections::VecDeque;
use std::fmt::{self, Write as _};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use time::{OffsetDateTime, UtcOffset};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_log::NormalizeEvent;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use super::stamp;

/// The most lines the ring keeps.
pub const RING_CAPACITY: usize = 5000;

/// One stored log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// The entry number. The first entry is 1; numbers never repeat, even
    /// after [`LogRing::clear`].
    pub seq: u64,
    /// The level, so a view can color the line.
    pub level: Level,
    /// The finished text: `[HH:MM:SS] LEVEL    target: message`. The time is
    /// local, or UTC when the system gives no offset.
    pub text: String,
}

/// The ring's shared state.
#[derive(Debug)]
struct Inner {
    /// The newest entries, oldest first.
    entries: VecDeque<LogEntry>,
    /// The number given to the last entry.
    last_seq: u64,
    /// The most entries to keep.
    capacity: usize,
}

/// A thread-safe store of the newest log lines.
///
/// Clones share one store.
#[derive(Debug, Clone)]
pub struct LogRing {
    /// The shared state.
    inner: Arc<Mutex<Inner>>,
}

impl Default for LogRing {
    fn default() -> Self {
        Self::with_capacity(RING_CAPACITY)
    }
}

impl LogRing {
    /// Make an empty ring with the standard capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Make an empty ring with a chosen capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                entries: VecDeque::new(),
                last_seq: 0,
                capacity: capacity.max(1),
            })),
        }
    }

    /// Lock the state. A poisoned lock still gives its data, so logging keeps
    /// working after a panic.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Add one line and drop the oldest when the ring is full.
    pub fn push(&self, level: Level, text: String) {
        let mut inner = self.lock();
        inner.last_seq += 1;
        let seq = inner.last_seq;
        if inner.entries.len() >= inner.capacity {
            inner.entries.pop_front();
        }
        inner.entries.push_back(LogEntry { seq, level, text });
    }

    /// All stored entries, oldest first.
    #[must_use]
    #[cfg(test)]
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.lock().entries.iter().cloned().collect()
    }

    /// The entries newer than `counter`, oldest first. Pass 0 to get all of
    /// them, then pass the last `seq` seen to get only the new ones.
    #[must_use]
    pub fn since(&self, counter: u64) -> Vec<LogEntry> {
        self.lock()
            .entries
            .iter()
            .filter(|entry| entry.seq > counter)
            .cloned()
            .collect()
    }

    /// The number of the newest entry ever added, or 0 when none was added.
    #[must_use]
    #[cfg(test)]
    pub fn counter(&self) -> u64 {
        self.lock().last_seq
    }

    /// The number of stored entries.
    #[must_use]
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// Whether no entry is stored.
    #[must_use]
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.lock().entries.is_empty()
    }

    /// Drop every stored entry. Entry numbers keep counting up.
    pub fn clear(&self) {
        self.lock().entries.clear();
    }
}

/// The level name in logs. Warnings read `WARNING`.
pub(super) fn level_name(level: Level) -> &'static str {
    match level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARNING",
        Level::INFO => "INFO",
        Level::DEBUG => "DEBUG",
        Level::TRACE => "TRACE",
    }
}

/// Build a ring line from its parts.
pub(super) fn format_line(
    time: OffsetDateTime,
    level: Level,
    target: &str,
    message: &str,
) -> String {
    format!(
        "[{}] {:<8} {target}: {message}",
        stamp::clock(time),
        level_name(level)
    )
}

/// Collects the message first, then the other fields as `name=value`.
#[derive(Default)]
pub(super) struct MessageVisitor {
    /// The event message.
    message: String,
    /// The other fields, each with a leading space.
    extra: String,
}

impl MessageVisitor {
    /// The message with the extra fields behind it.
    pub(super) fn finish(self) -> String {
        self.message + &self.extra
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else if !field.name().starts_with("log.") {
            // Fields named `log.*` only repeat the metadata of a bridged
            // `log` record.
            let _ = write!(self.extra, " {}={value:?}", field.name());
        }
    }
}

/// A tracing layer that copies every event into a [`LogRing`].
#[derive(Debug, Clone)]
pub struct RingLayer {
    /// The target store.
    ring: LogRing,
    /// The offset used for the time of each line.
    offset: UtcOffset,
}

impl RingLayer {
    /// Make a layer that fills `ring`, with line times in `offset`.
    #[must_use]
    pub const fn new(ring: LogRing, offset: UtcOffset) -> Self {
        Self { ring, offset }
    }
}

impl<S: Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        // Events from the `log` bridge carry their real target here.
        let normalized = event.normalized_metadata();
        let meta = normalized.as_ref().unwrap_or_else(|| event.metadata());
        let line = format_line(
            stamp::now(self.offset),
            *meta.level(),
            meta.target(),
            &visitor.finish(),
        );
        self.ring.push(*meta.level(), line);
    }
}

#[cfg(test)]
mod tests {
    use time::{OffsetDateTime, UtcOffset};
    use tracing::Level;
    use tracing_subscriber::layer::SubscriberExt;

    use super::{LogRing, RING_CAPACITY, RingLayer, format_line};

    #[test]
    fn ring_caps_at_5000_and_drops_the_oldest() {
        let ring = LogRing::new();
        for n in 0..(RING_CAPACITY + 10) {
            ring.push(Level::INFO, format!("line {n}"));
        }
        let lines = ring.snapshot();
        assert_eq!(lines.len(), 5000);
        assert_eq!(lines[0].text, "line 10");
        assert_eq!(lines[4999].text, "line 5009");
    }

    #[test]
    fn clear_empties_the_ring_but_keeps_counting() {
        let ring = LogRing::new();
        ring.push(Level::INFO, "a".into());
        ring.push(Level::INFO, "b".into());
        ring.clear();
        assert!(ring.is_empty());
        ring.push(Level::INFO, "c".into());
        assert_eq!(ring.snapshot()[0].seq, 3);
        assert_eq!(ring.counter(), 3);
    }

    #[test]
    fn since_returns_only_newer_entries() {
        let ring = LogRing::new();
        ring.push(Level::INFO, "a".into());
        ring.push(Level::WARN, "b".into());
        ring.push(Level::ERROR, "c".into());
        let newer = ring.since(1);
        assert_eq!(newer.len(), 2);
        assert_eq!(newer[0].text, "b");
        assert_eq!(newer[1].level, Level::ERROR);
        assert_eq!(ring.since(ring.counter()).len(), 0);
    }

    #[test]
    fn since_skips_entries_the_ring_already_dropped() {
        let ring = LogRing::with_capacity(2);
        for text in ["a", "b", "c"] {
            ring.push(Level::INFO, text.into());
        }
        let all = ring.since(0);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].seq, 2);
    }

    #[test]
    fn clones_share_one_store() {
        let ring = LogRing::new();
        let other = ring.clone();
        other.push(Level::INFO, "x".into());
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn line_format_pads_the_level_to_eight_characters() {
        let time = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("time");
        assert_eq!(
            format_line(time, Level::INFO, "tarragon::scan", "done"),
            "[22:13:20] INFO     tarragon::scan: done"
        );
        assert_eq!(
            format_line(time, Level::WARN, "t", "m"),
            "[22:13:20] WARNING  t: m"
        );
        assert_eq!(
            format_line(time, Level::ERROR, "t", "m"),
            "[22:13:20] ERROR    t: m"
        );
    }

    #[test]
    fn layer_stores_events_with_level_target_and_fields() {
        let ring = LogRing::new();
        let subscriber =
            tracing_subscriber::registry().with(RingLayer::new(ring.clone(), UtcOffset::UTC));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "demo", count = 3, "slow");
        });
        let entries = ring.snapshot();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, Level::WARN);
        assert!(entries[0].text.ends_with("WARNING  demo: slow count=3"));
        assert!(entries[0].text.starts_with('['));
    }

    #[test]
    fn ring_keeps_working_after_a_thread_panics_holding_the_lock() {
        let _serial = super::super::PANIC_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ring = LogRing::new();
        let held = ring.clone();
        let result = std::thread::spawn(move || {
            let _guard = held.lock();
            panic!("poison the lock");
        })
        .join();
        assert!(result.is_err());
        ring.push(Level::INFO, "after".into());
        assert_eq!(ring.snapshot()[0].text, "after");
        ring.clear();
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn bridged_log_records_carry_no_log_field_text() {
        let ring = LogRing::new();
        let subscriber =
            tracing_subscriber::registry().with(RingLayer::new(ring.clone(), UtcOffset::UTC));
        tracing::subscriber::with_default(subscriber, || {
            let record = log::Record::builder()
                .args(format_args!("bridged"))
                .level(log::Level::Info)
                .target("bridge_demo")
                .file(Some("x.rs"))
                .line(Some(7))
                .build();
            tracing_log::format_trace(&record).expect("bridge");
        });
        let entries = ring.snapshot();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].text.ends_with("INFO     bridge_demo: bridged"));
        assert!(!entries[0].text.contains("log."), "{}", entries[0].text);
    }
}
