use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crossbeam_channel::unbounded;
use tarragon_db::DbError;
use tarragon_db::repository::{ThumbnailWrite, WriteBatch, WriteReport};

use super::{Message, run_with};
use crate::pipeline::Repaint;

/// Build a rendered row for `path`.
fn row(path: &str) -> ThumbnailWrite {
    ThumbnailWrite {
        path: path.to_owned(),
        mtime: 1,
        size: 1,
        width: 1,
        height: 1,
        cache_uuid: "uuid".to_owned(),
        thumbnail_cache_path: None,
        preview_cache_path: None,
        full_cache_path: None,
        last_accessed: 1,
    }
}

/// What a run left behind: the epoch and the repaint count.
struct Outcome {
    epoch: u64,
    repaints: usize,
}

/// Queue `messages`, close the channel, and run the writer on this thread.
fn drive(
    messages: Vec<Message>,
    store: &dyn Fn(&WriteBatch) -> Result<WriteReport, DbError>,
) -> Outcome {
    let (sender, receiver) = unbounded();
    for message in messages {
        sender.send(message).expect("queue the message");
    }
    drop(sender);
    let epoch = AtomicU64::new(0);
    let repaints = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&repaints);
    let repaint: Repaint = Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    run_with(store, &receiver, &epoch, &repaint);
    Outcome {
        epoch: epoch.load(Ordering::SeqCst),
        repaints: repaints.load(Ordering::SeqCst),
    }
}

/// A store that reports `stored` and records the size of each batch.
fn recorder(
    calls: &RefCell<Vec<WriteBatch>>,
    stored: bool,
) -> impl Fn(&WriteBatch) -> Result<WriteReport, DbError> + '_ {
    move |batch| {
        calls.borrow_mut().push(batch.clone());
        Ok(WriteReport { stored, failed: 0 })
    }
}

/// Queue `count` row messages.
fn rows(count: usize) -> Vec<Message> {
    (0..count)
        .map(|index| Message::Row(row(&format!("/pics/{index}"))))
        .collect()
}

/// One pass makes one store call with every kind of item.
#[test]
fn one_pass_makes_one_store_call() {
    let mut messages = rows(10);
    messages.extend((0..5).map(|index| Message::Touch(format!("/pics/{index}"))));
    messages.extend((0..4).map(|index| Message::Colors {
        path: format!("/pics/{index}"),
        names: vec!["red".to_owned()],
    }));
    let calls = RefCell::new(Vec::new());

    drive(messages, &recorder(&calls, true));

    let calls = calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].rows.len(), 10);
    assert_eq!(calls[0].touched.len(), 5);
    assert_eq!(calls[0].colors.len(), 4);
    assert!(calls[0].touched_at > 0);
}

/// A full queue splits into passes of at most 64 messages.
#[test]
fn a_long_queue_splits_into_passes_of_64() {
    let calls = RefCell::new(Vec::new());

    drive(rows(100), &recorder(&calls, true));

    let sizes: Vec<usize> = calls
        .borrow()
        .iter()
        .map(|batch| batch.rows.len())
        .collect();
    assert_eq!(sizes, [64, 36]);
}

/// A pass that stores nothing leaves the epoch alone and does not repaint.
#[test]
fn a_pass_that_stored_nothing_does_not_bump_or_repaint() {
    let calls = RefCell::new(Vec::new());

    let outcome = drive(rows(100), &recorder(&calls, false));

    assert_eq!(calls.borrow().len(), 2);
    assert_eq!(outcome.epoch, 0);
    assert_eq!(outcome.repaints, 0);
}

/// Each pass that stored data bumps the epoch once and repaints once.
#[test]
fn each_stored_pass_bumps_the_epoch_and_repaints_once() {
    let calls = RefCell::new(Vec::new());

    let outcome = drive(rows(100), &recorder(&calls, true));

    assert_eq!(outcome.epoch, 2);
    assert_eq!(outcome.repaints, 2);
}

/// A flush ack arrives after the store call.
#[test]
fn a_flush_ack_arrives_after_the_store_call() {
    let (ack_tx, ack_rx) = unbounded();
    let acked_early = RefCell::new(None);
    let store = |_: &WriteBatch| {
        *acked_early.borrow_mut() = Some(ack_rx.try_recv().is_ok());
        Ok(WriteReport {
            stored: true,
            failed: 0,
        })
    };

    drive(
        vec![Message::Row(row("/a")), Message::Flush(ack_tx)],
        &store,
    );

    assert_eq!(*acked_early.borrow(), Some(false));
    assert!(ack_rx.try_recv().is_ok());
}

/// A flush ack also arrives when the store call fails, and the pass counts
/// as stored nothing.
#[test]
fn a_flush_ack_arrives_when_the_store_fails() {
    let (ack_tx, ack_rx) = unbounded();
    let store = |_: &WriteBatch| Err(DbError::LockPoisoned);

    let outcome = drive(
        vec![Message::Row(row("/a")), Message::Flush(ack_tx)],
        &store,
    );

    assert!(ack_rx.try_recv().is_ok());
    assert_eq!(outcome.epoch, 0);
    assert_eq!(outcome.repaints, 0);
}

/// A queue of only a flush calls no store and still acks.
#[test]
fn a_lone_flush_calls_no_store_and_acks() {
    let (ack_tx, ack_rx) = unbounded();
    let calls = RefCell::new(Vec::new());

    drive(vec![Message::Flush(ack_tx)], &recorder(&calls, true));

    assert!(calls.borrow().is_empty());
    assert!(ack_rx.try_recv().is_ok());
}

/// The loop goes on after a failed pass and counts only the pass that stored.
#[test]
fn the_loop_goes_on_after_a_failed_pass() {
    let call = std::cell::Cell::new(0);
    let store = |_: &WriteBatch| {
        call.set(call.get() + 1);
        if call.get() == 1 {
            Err(DbError::LockPoisoned)
        } else {
            Ok(WriteReport {
                stored: true,
                failed: 0,
            })
        }
    };

    let outcome = drive(rows(70), &store);

    assert_eq!(call.get(), 2);
    assert_eq!(outcome.epoch, 1);
    assert_eq!(outcome.repaints, 1);
}
