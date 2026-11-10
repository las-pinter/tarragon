//! The one thread that writes thumbnail rows.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender};
use tarragon_db::repository::{ThumbnailWrite, WriteBatch, WriteReport};
use tarragon_db::{Database, DbError};

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
    Flush(Sender<()>),
}

/// Store batches until every sender is gone. Workers never touch the
/// database, so this loop is the only writer. The UI thread reads under the
/// same lock, so a pass must hold it for a short time only.
///
/// Each pass stores its whole batch in one transaction. After a pass that
/// stored data, `epoch` goes up by one and `repaint` runs. A failed pass is
/// dropped, and those files render again on the next open.
pub(super) fn run(
    db: &Arc<Database>,
    messages: &Receiver<Message>,
    epoch: &AtomicU64,
    repaint: &Repaint,
) {
    run_with(&|batch| db.apply_writes(batch), messages, epoch, repaint);
}

/// The writer loop with the storage call passed in, so a test can record it.
fn run_with(
    store: &dyn Fn(&WriteBatch) -> Result<WriteReport, DbError>,
    messages: &Receiver<Message>,
    epoch: &AtomicU64,
    repaint: &Repaint,
) {
    while let Ok(first) = messages.recv() {
        let mut batch = WriteBatch::default();
        let mut acks = Vec::new();
        // A pass is one drain of the channel, up to BATCH messages.
        let mut count = 1;
        add(&mut batch, &mut acks, first);
        while count < BATCH {
            let Ok(next) = messages.try_recv() else { break };
            add(&mut batch, &mut acks, next);
            count += 1;
        }
        batch.touched_at = now();
        let stored = !batch.is_empty()
            && match store(&batch) {
                Ok(report) => report.stored,
                Err(error) => {
                    tracing::warn!(
                        "cannot store the writer batch of {} rows, {} touches and {} colours: {error}",
                        batch.rows.len(),
                        batch.touched.len(),
                        batch.colors.len(),
                    );
                    false
                }
            };
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

/// Put a message in the list for its kind.
fn add(batch: &mut WriteBatch, acks: &mut Vec<Sender<()>>, message: Message) {
    match message {
        Message::Row(row) => batch.rows.push(row),
        Message::Touch(path) => batch.touched.push(path),
        Message::Colors { path, names } => batch.colors.push((path, names)),
        Message::Flush(ack) => acks.push(ack),
    }
}

/// The time now in epoch seconds.
pub(super) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |age| i64::try_from(age.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
