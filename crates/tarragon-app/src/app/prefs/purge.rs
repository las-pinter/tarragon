//! The Cache Size row and the Purge Cache button of the Preferences window.
//!
//! The size is measured on its own thread and the purge runs on the work
//! thread, so the window stays responsive. The state sits in the egui context,
//! like the settle counter, so the app struct does not grow.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crossbeam_channel::Receiver;
use eframe::egui::{Context, Id};
use tarragon_cache::store::CacheStore;
use tarragon_ui::prefs::PrefsValues;
use tarragon_ui::preview::size_text;

use super::{CACHE_SIZE_UNKNOWN, SETTLE_FRAMES, SETTLE_ID};
use crate::app::App;
use crate::prefs;
use crate::thumbs::Work;

/// The context key of the shared state.
const STATE_ID: &str = "prefs_cache_state";

/// What the dialog says when the purge refuses an unsafe cache folder.
const UNSAFE_TEXT: &str = "The cache folder is not safe to purge.";

/// The text for the dialog. The work result is plain text, so the unsafe
/// folder is known by its message; the work thread has logged the raw text.
fn dialog_text(error: String) -> String {
    if error.contains("unsafe cache root") {
        UNSAFE_TEXT.to_owned()
    } else {
        error
    }
}

/// What the Preferences window knows about the cache between frames.
pub(super) struct State {
    /// The Cache Size text as last measured.
    pub(super) size: String,
    /// The answer of the measure thread, until it arrives.
    pub(super) measuring: Option<Receiver<Result<u64, String>>>,
    /// A purge was asked for and its result has not come yet.
    pub(super) purging: bool,
}

pub(super) type Shared = Arc<Mutex<State>>;

/// The shared state of this context.
pub(super) fn shared(ctx: &Context) -> Shared {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_insert_with(Id::new(STATE_ID), || {
            Arc::new(Mutex::new(State {
                size: CACHE_SIZE_UNKNOWN.to_owned(),
                measuring: None,
                purging: false,
            }))
        })
        .clone()
    })
}

/// Lock the state. A poisoned lock still holds usable text and flags.
pub(super) fn lock(state: &Shared) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl App {
    /// Open the window from the menu. The cache size is measured again, and
    /// reads as unknown until the answer comes.
    pub(super) fn open_prefs(&mut self, ctx: &Context) {
        lock(&shared(ctx)).size = String::from(CACHE_SIZE_UNKNOWN);
        self.show_prefs(ctx, prefs::load(&self.db));
        self.measure_cache(ctx);
    }

    /// Show the window with `values`, the last measured size, and the purge
    /// state. The form needs a few frames to learn its size.
    pub(super) fn show_prefs(&mut self, ctx: &Context, values: PrefsValues) {
        let (size, purging) = {
            let state = shared(ctx);
            let state = lock(&state);
            (state.size.clone(), state.purging)
        };
        let window = self.shell.prefs_mut();
        window.open(values, size);
        // A purge may still run from an earlier opening of the window.
        window.set_purging(purging);
        ctx.data_mut(|data| data.insert_temp(Id::new(SETTLE_ID), SETTLE_FRAMES));
    }

    /// Start a purge, unless other work runs now. It acts at once and does not
    /// wait for OK.
    pub(super) fn purge_cache(&mut self, ctx: &Context) {
        if self.thumbs.is_working().is_some() {
            return;
        }
        self.thumbs.purge(self.data_root.clone());
        lock(&shared(ctx)).purging = true;
        let window = self.shell.prefs_mut();
        window.set_purging(true);
        window.set_error(None);
    }

    /// Take what the threads have finished: the measured size and the purge
    /// result. Runs every frame, so no result waits unread.
    pub(super) fn poll_cache(&mut self, ctx: &Context) {
        let state = shared(ctx);
        let answer = lock(&state)
            .measuring
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(answer) = answer {
            let text = match answer {
                Ok(bytes) => size_text(bytes),
                Err(error) => {
                    tracing::warn!("cannot measure the cache size: {error}");
                    CACHE_SIZE_UNKNOWN.to_owned()
                }
            };
            let mut locked = lock(&state);
            locked.measuring = None;
            locked.size.clone_from(&text);
            self.shell.prefs_mut().set_cache_size(text);
        }
        // An eviction result is only taken: its failure is already logged.
        if let Some((Work::Purge, result)) = self.thumbs.take_work_result() {
            lock(&state).purging = false;
            let window = self.shell.prefs_mut();
            window.set_purging(false);
            window.set_error(result.err().map(dialog_text));
            self.measure_cache(ctx);
        }
        if lock(&state).purging {
            ctx.request_repaint();
        }
    }

    /// Measure the cache on a thread. A newer measure replaces a pending one.
    fn measure_cache(&self, ctx: &Context) {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let root = self.cache_root.clone();
        let wake = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("tarragon-cache-size".to_owned())
            .spawn(move || {
                let size = CacheStore::new(root)
                    .total_size()
                    .map_err(|error| error.to_string());
                // A newer measure dropped the receiver, so the answer is not needed.
                let _ = sender.send(size);
                wake.request_repaint();
            });
        let state = shared(ctx);
        let mut state = lock(&state);
        match spawned {
            Ok(_) => state.measuring = Some(receiver),
            Err(error) => {
                tracing::warn!("cannot start the cache size thread: {error}");
                state.measuring = None;
            }
        }
    }
}
