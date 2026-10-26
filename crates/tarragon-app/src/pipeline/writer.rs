//! The one thread that writes thumbnail rows.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender};
use tarragon_db::Database;
use tarragon_db::repository::ThumbnailWrite;

/// The most rows written in one pass.
const BATCH: usize = 64;

/// A message for the writer thread.
pub(super) enum Message {
    /// A row to store.
    Row(ThumbnailWrite),
    /// A cache hit: set `last_accessed` for this path.
    Touch(String),
    /// Reply on the sender once every earlier row is stored.
    #[cfg_attr(not(test), expect(dead_code, reason = "only tests send it"))]
    Flush(Sender<()>),
}

/// Store rows until every sender is gone. Workers never touch the database,
/// so this loop is the only writer and the lock is rarely contended.
pub(super) fn run(db: &Arc<Database>, messages: &Receiver<Message>) {
    while let Ok(first) = messages.recv() {
        let mut rows = Vec::new();
        let mut acks = Vec::new();
        let mut touched = Vec::new();
        // A batch is one drain of the channel per pass, up to BATCH messages.
        // Each row is still one upsert; touches share one statement.
        let mut batch = Batch {
            rows: &mut rows,
            acks: &mut acks,
            touched: &mut touched,
        };
        batch.add(first);
        while batch.len() < BATCH {
            let Ok(next) = messages.try_recv() else { break };
            batch.add(next);
        }
        for row in &rows {
            if let Err(error) = db.upsert_thumbnail(row) {
                tracing::warn!("cannot store the thumbnail row for {}: {error}", row.path);
            }
        }
        if !touched.is_empty()
            && let Err(error) = db.touch_last_accessed(&touched, now())
        {
            tracing::warn!("cannot refresh last_accessed: {error}");
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
}

impl Batch<'_> {
    fn len(&self) -> usize {
        self.rows.len() + self.acks.len() + self.touched.len()
    }

    /// Put a message in the list for its kind.
    fn add(&mut self, message: Message) {
        match message {
            Message::Row(row) => self.rows.push(row),
            Message::Touch(path) => self.touched.push(path),
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
