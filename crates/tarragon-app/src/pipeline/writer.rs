//! The one thread that writes thumbnail rows.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender};
use tarragon_db::Database;
use tarragon_db::repository::ThumbnailWrite;

use super::Repaint;

/// The most rows written in one pass.
const BATCH: usize = 64;

/// A message for the writer thread.
pub(super) enum Message {
    /// A row to store.
    Row(ThumbnailWrite),
    /// A cache hit: set `last_accessed` for this path.
    Touch(String),
    /// Replace the `auto_color` tags of `path` with `names`.
    Colors { path: String, names: Vec<String> },
    /// Reply on the sender once every earlier row is stored.
    #[cfg_attr(not(test), expect(dead_code, reason = "only tests send it"))]
    Flush(Sender<()>),
}

/// Store rows until every sender is gone. Workers never touch the database,
/// so this loop is the only writer and the lock is rarely contended.
///
/// After each pass that stored data, `epoch` goes up by one and `repaint` runs.
pub(super) fn run(
    db: &Arc<Database>,
    messages: &Receiver<Message>,
    epoch: &AtomicU64,
    repaint: &Repaint,
) {
    while let Ok(first) = messages.recv() {
        let mut rows = Vec::new();
        let mut acks = Vec::new();
        let mut touched = Vec::new();
        let mut colors = Vec::new();
        // A batch is one drain of the channel per pass, up to BATCH messages.
        // Each row is still one upsert; touches share one statement.
        let mut batch = Batch {
            rows: &mut rows,
            acks: &mut acks,
            touched: &mut touched,
            colors: &mut colors,
        };
        batch.add(first);
        while batch.len() < BATCH {
            let Ok(next) = messages.try_recv() else { break };
            batch.add(next);
        }
        let mut stored = false;
        for row in &rows {
            match db.upsert_thumbnail(row) {
                Ok(()) => stored = true,
                Err(error) => {
                    tracing::warn!("cannot store the thumbnail row for {}: {error}", row.path);
                }
            }
        }
        if !touched.is_empty() {
            match db.touch_last_accessed(&touched, now()) {
                Ok(_) => stored = true,
                Err(error) => tracing::warn!("cannot refresh last_accessed: {error}"),
            }
        }
        for (path, names) in &colors {
            match db.replace_auto_color_tags(path, names) {
                Ok(_) => stored = true,
                Err(error) => tracing::warn!("cannot store the colour tags for {path}: {error}"),
            }
        }
        if stored {
            // Bump before the callback, so a woken UI sees the new value.
            epoch.fetch_add(1, Ordering::AcqRel);
            let _ = catch_unwind(AssertUnwindSafe(|| repaint()));
        }
        for ack in acks {
            // The waiter may have given up. That is fine.
            let _ = ack.send(());
        }
    }
}

/// The lists one pass fills.
struct Batch<'a> {
    rows: &'a mut Vec<ThumbnailWrite>,
    acks: &'a mut Vec<Sender<()>>,
    touched: &'a mut Vec<String>,
    colors: &'a mut Vec<(String, Vec<String>)>,
}

impl Batch<'_> {
    fn len(&self) -> usize {
        self.rows.len() + self.acks.len() + self.touched.len() + self.colors.len()
    }

    /// Put a message in the list for its kind.
    fn add(&mut self, message: Message) {
        match message {
            Message::Row(row) => self.rows.push(row),
            Message::Touch(path) => self.touched.push(path),
            Message::Colors { path, names } => self.colors.push((path, names)),
            Message::Flush(ack) => self.acks.push(ack),
        }
    }
}

/// The time now in epoch seconds.
pub(super) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |age| i64::try_from(age.as_secs()).unwrap_or(i64::MAX))
}
