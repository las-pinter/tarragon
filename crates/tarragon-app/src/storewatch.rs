//! Throttles the reaction to the writer's store epoch.

use std::time::{Duration, Instant};

/// The least time between two reactions to a changed epoch.
pub const INTERVAL: Duration = Duration::from_millis(250);

/// Remembers the last epoch acted on and when.
#[derive(Debug, Default)]
pub struct StoreWatch {
    /// The epoch the last reaction was for.
    seen: u64,
    /// When the last reaction happened. None before the first.
    fired: Option<Instant>,
}

impl StoreWatch {
    /// Whether the caller should react now to `epoch`.
    ///
    /// True when the epoch moved since the last true and [`INTERVAL`] has
    /// passed since it. The first change fires at once. A change inside the
    /// window stays pending and fires on the first poll after the window.
    pub fn poll(&mut self, epoch: u64, now: Instant) -> bool {
        if !self.pending(epoch) {
            return false;
        }
        if let Some(fired) = self.fired
            && now.saturating_duration_since(fired) < INTERVAL
        {
            return false;
        }
        self.seen = epoch;
        self.fired = Some(now);
        true
    }

    /// The time left in the window while a change to `epoch` waits, so the
    /// caller can ask for a repaint when the window ends. None when no change
    /// waits.
    pub fn wait(&self, epoch: u64, now: Instant) -> Option<Duration> {
        let fired = self.fired.filter(|_| self.pending(epoch))?;
        Some(INTERVAL.saturating_sub(now.saturating_duration_since(fired)))
    }

    /// Whether `epoch` differs from the one the last reaction was for.
    fn pending(&self, epoch: u64) -> bool {
        epoch != self.seen
    }
}

#[cfg(test)]
mod tests;
