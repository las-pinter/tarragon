//! The thread that decodes Preview pictures off the UI thread.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use image::RgbaImage;
use image::imageops::{FilterType, resize};

use crate::pipeline::Repaint;

/// One picture to decode.
pub(crate) struct LoadItem {
    /// The identity of the picture, handed back in [`Loaded`].
    pub(crate) key: PathBuf,
    /// The cache tier file to decode.
    pub(crate) tier: PathBuf,
    /// The most width and height the result may have.
    pub(crate) size: (u32, u32),
}

/// The result for one [`LoadItem`].
pub(crate) struct Loaded {
    /// The revision of the request that asked for it.
    pub(crate) revision: u64,
    /// The `key` of the item.
    pub(crate) key: PathBuf,
    /// The decoded picture, or why it failed.
    pub(crate) image: Result<RgbaImage, String>,
}

/// A request for the thread.
struct Request {
    revision: u64,
    items: Vec<LoadItem>,
}

/// Owns the decode thread. Dropping it stops and joins the thread.
pub(crate) struct PreviewLoader {
    requests: Option<Sender<Request>>,
    results: Receiver<Loaded>,
    thread: Option<JoinHandle<()>>,
}

impl PreviewLoader {
    /// Start the thread. `repaint` runs after each finished item.
    pub(crate) fn new(repaint: Repaint) -> io::Result<Self> {
        let (requests, queue) = crossbeam_channel::unbounded();
        let (sender, results) = crossbeam_channel::unbounded();
        let thread = std::thread::Builder::new()
            .name("tarragon-preview".to_owned())
            .spawn(move || run(&queue, &sender, &repaint))?;
        Ok(Self {
            requests: Some(requests),
            results,
            thread: Some(thread),
        })
    }

    /// Ask for `items`. It replaces a queued request and ends a running one before its next item.
    pub(crate) fn request(&self, revision: u64, items: Vec<LoadItem>) {
        if let Some(requests) = &self.requests {
            // The send fails only after the thread ended, which needs a panic
            // outside the decode guard. Nothing is left to ask then.
            let _ = requests.send(Request { revision, items });
        }
    }

    /// Take every result that is ready.
    pub(crate) fn poll(&self) -> Vec<Loaded> {
        self.results.try_iter().collect()
    }
}

impl Drop for PreviewLoader {
    fn drop(&mut self) {
        // With the sender gone the thread leaves its loop.
        self.requests = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Serve requests until the sender is gone. Between items it looks at the
/// queue: a newer request ends the current one, and a closed queue ends the thread.
fn run(queue: &Receiver<Request>, results: &Sender<Loaded>, repaint: &Repaint) {
    let mut waiting = None;
    loop {
        let first = match waiting.take() {
            Some(request) => request,
            None => match queue.recv() {
                Ok(request) => request,
                Err(_) => return,
            },
        };
        let request = newest(first, queue);
        for item in request.items {
            match queue.try_recv() {
                Ok(newer) => {
                    waiting = Some(newer);
                    break;
                }
                Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => {}
            }
            let image = catch_unwind(AssertUnwindSafe(|| decode(&item)))
                .unwrap_or_else(|_| Err("the decoder panicked".to_owned()));
            let loaded = Loaded {
                revision: request.revision,
                key: item.key,
                image,
            };
            if results.send(loaded).is_err() {
                return;
            }
            let _ = catch_unwind(AssertUnwindSafe(|| repaint()));
        }
    }
}

/// The last request in the queue, or `first` when it is alone.
fn newest(first: Request, queue: &Receiver<Request>) -> Request {
    queue.try_iter().last().unwrap_or(first)
}

/// Decode one tier file and shrink it to fit `item.size`. Never enlarges.
fn decode(item: &LoadItem) -> Result<RgbaImage, String> {
    let decoded = image::open(&item.tier)
        .map_err(|error| format!("cannot decode {}: {error}", item.tier.display()))?
        .to_rgba8();
    let (width, height) = item.size;
    if decoded.width() > width || decoded.height() > height {
        return Ok(resize(
            &decoded,
            width.max(1),
            height.max(1),
            FilterType::Triangle,
        ));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests;
