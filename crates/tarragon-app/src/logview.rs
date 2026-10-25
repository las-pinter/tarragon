//! The Log panel's UI-side copy of the log lines.

use std::collections::VecDeque;

use tarragon_ui::shell::{LogLevel, LogLine, LogSource};
use tracing::Level;

use crate::logging::ring::{LogEntry, LogRing, RING_CAPACITY};

/// Map a tracing level to the panel's level. Trace reads as debug.
pub fn panel_level(level: Level) -> LogLevel {
    match level {
        Level::ERROR => LogLevel::Error,
        Level::WARN => LogLevel::Warn,
        Level::INFO => LogLevel::Info,
        Level::DEBUG | Level::TRACE => LogLevel::Debug,
    }
}

/// The newest lines, copied out of the ring.
///
/// The frame reads this copy, so it never holds the ring lock. A line logged
/// during the frame cannot deadlock on it.
#[derive(Debug, Default)]
pub struct LogCache {
    /// The lines, oldest first.
    entries: VecDeque<LogEntry>,
    /// The number of the newest line copied so far.
    last_seq: u64,
}

impl LogCache {
    /// Copy the lines added to `ring` since the last pull. The ring lock is
    /// held only inside this call.
    pub fn pull(&mut self, ring: &LogRing) {
        self.append(ring.since(self.last_seq));
    }

    /// Add new lines and drop the oldest ones over the cap. The cap equals the
    /// ring size, so a panel that was hidden for a while still gets the
    /// last full ring when it opens.
    pub fn append(&mut self, new: Vec<LogEntry>) {
        if let Some(last) = new.last() {
            self.last_seq = self.last_seq.max(last.seq);
        }
        self.entries.extend(new);
        let extra = self.entries.len().saturating_sub(RING_CAPACITY);
        self.entries.drain(..extra);
    }

    /// Drop every line. The next pull gets only lines that come after.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// The cached lines as a panel source.
    pub fn source(&self) -> CacheSource<'_> {
        CacheSource(&self.entries)
    }
}

/// A [`LogSource`] over the cached lines.
pub struct CacheSource<'a>(&'a VecDeque<LogEntry>);

impl LogSource for CacheSource<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn line(&self, index: usize) -> LogLine<'_> {
        let entry = &self.0[index];
        LogLine {
            level: panel_level(entry.level),
            text: &entry.text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(seq: u64) -> LogEntry {
        LogEntry {
            seq,
            level: Level::INFO,
            text: format!("line {seq}"),
        }
    }

    fn texts(cache: &LogCache) -> Vec<String> {
        let source = cache.source();
        (0..source.len())
            .map(|index| source.line(index).text.to_owned())
            .collect()
    }

    #[test]
    fn levels_map_to_panel_levels() {
        assert_eq!(panel_level(Level::TRACE), LogLevel::Debug);
        assert_eq!(panel_level(Level::DEBUG), LogLevel::Debug);
        assert_eq!(panel_level(Level::INFO), LogLevel::Info);
        assert_eq!(panel_level(Level::WARN), LogLevel::Warn);
        assert_eq!(panel_level(Level::ERROR), LogLevel::Error);
    }

    #[test]
    fn pull_copies_only_lines_after_the_last_seq() {
        let ring = LogRing::new();
        let mut cache = LogCache::default();
        ring.push(Level::INFO, "a".to_owned());
        ring.push(Level::WARN, "b".to_owned());
        cache.pull(&ring);
        cache.pull(&ring);
        ring.push(Level::ERROR, "c".to_owned());
        cache.pull(&ring);
        assert_eq!(texts(&cache), ["a", "b", "c"]);
        assert_eq!(cache.source().line(2).level, LogLevel::Error);
    }

    #[test]
    fn append_keeps_only_the_newest_lines_over_the_cap() {
        let mut cache = LogCache::default();
        let total = RING_CAPACITY as u64 + 10;
        cache.append((1..=total).map(entry).collect());
        let source = cache.source();
        assert_eq!(source.len(), RING_CAPACITY);
        assert_eq!(source.line(0).text, "line 11");
        assert_eq!(source.line(RING_CAPACITY - 1).text, format!("line {total}"));
    }

    #[test]
    fn a_long_hidden_panel_gets_the_last_full_ring_on_open() {
        let ring = LogRing::new();
        let mut cache = LogCache::default();
        ring.push(Level::INFO, "old".to_owned());
        cache.pull(&ring);
        for index in 0..RING_CAPACITY {
            ring.push(Level::INFO, format!("new {index}"));
        }
        cache.pull(&ring);
        let source = cache.source();
        assert_eq!(source.len(), RING_CAPACITY);
        assert_eq!(source.line(0).text, "new 0");
    }

    #[test]
    fn clear_empties_the_cache_and_later_pulls_skip_old_lines() {
        let ring = LogRing::new();
        let mut cache = LogCache::default();
        ring.push(Level::INFO, "a".to_owned());
        cache.pull(&ring);
        cache.clear();
        cache.pull(&ring);
        assert!(cache.source().is_empty());
        ring.push(Level::INFO, "b".to_owned());
        cache.pull(&ring);
        assert_eq!(texts(&cache), ["b"]);
    }
}
