//! Runs the pipeline for the open folder and keeps the gallery tiles in step.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};
use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use image::RgbaImage;
use tarragon_ui::gallery::Gallery;
use tarragon_ui::grid::{GridItem, TileError};
use tarragon_ui::texture_budget::GRID_BUDGET_BYTES;

use crate::feed::{Feed, Tiles};
use crate::pipeline::{Opened, Pipeline, PipelineError, Repaint, Results};

/// Results applied per frame. Every result is a 256 px image, so each costs
/// about the same, and a fixed count keeps the frame time steady and testable.
const RESULTS_PER_FRAME: usize = 16;

/// How often a running open checks for its answer when no repaint arrives.
const OPEN_POLL: Duration = Duration::from_millis(100);

impl Tiles for Gallery {
    type Texture = TextureHandle;

    fn contains(&self, key: &Path) -> bool {
        self.has_item(key)
    }

    fn set_texture(&mut self, key: &Path, texture: TextureHandle) -> bool {
        Self::set_texture(self, key, texture)
    }

    fn clear_texture(&mut self, key: &Path) {
        Self::clear_texture(self, key);
    }

    fn mark_failed(&mut self, key: &Path, error: TileError) -> bool {
        Self::mark_failed(self, key, error)
    }
}

/// What the open thread hands back. The pipeline comes back with it, because
/// `open_folder` needs `&mut` and only one open may run at a time.
struct Done {
    pipeline: Pipeline,
    result: Result<Opened, PipelineError>,
}

/// Where the pipeline is.
enum Slot {
    /// Ready for a call.
    Idle(Pipeline),
    /// Inside the open thread.
    Opening {
        folder: PathBuf,
        done: Receiver<Done>,
    },
    /// Lost to a failed thread start, or taken at exit.
    Gone,
}

/// Lets `main` wait for the pipeline to finish after the window is gone.
#[derive(Clone, Default)]
pub struct ExitWait(Arc<Mutex<Option<JoinHandle<()>>>>);

impl ExitWait {
    /// Block until the pipeline has stopped and stored its rows.
    pub fn wait(&self) {
        let handle = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

/// The thumbnail side of the app: one pipeline, retargeted to one folder at a time.
pub struct Thumbs {
    slot: Slot,
    results: Results,
    feed: Feed,
    /// The folder to open when the pipeline is idle. A newer request replaces it.
    pending: Option<PathBuf>,
    /// The folder the gallery shows. A failed open leaves it as it was.
    current: Option<PathBuf>,
    /// True when an open ended and the shell must learn `current`.
    changed: bool,
    /// The visible revision last sent to the pipeline.
    synced: Option<u64>,
    /// True once the open is over, whether it worked or not.
    opened: bool,
    /// True once `loaded_once` has reported.
    announced: bool,
    started: Instant,
    repaint: Repaint,
}

impl Thumbs {
    /// Hold `pipeline` and open `folder` on the first frame, if there is one.
    /// The open waits for a frame so a window that never starts does no work.
    /// Without a folder the pipeline idles.
    #[must_use]
    pub fn new(
        pipeline: Pipeline,
        results: Results,
        folder: Option<PathBuf>,
        repaint: Repaint,
    ) -> Self {
        Self {
            slot: Slot::Idle(pipeline),
            results,
            feed: Feed::new(GRID_BUDGET_BYTES),
            pending: folder,
            current: None,
            changed: false,
            synced: None,
            opened: false,
            announced: false,
            started: Instant::now(),
            repaint,
        }
    }

    /// Run before the gallery draws: start or finish the open, then apply a
    /// bounded batch of results.
    pub fn before_draw(&mut self, ctx: &Context, gallery: &mut Gallery) {
        self.finish_open(gallery);
        if matches!(self.slot, Slot::Idle(_))
            && let Some(folder) = self.pending.take()
        {
            self.start_open(folder);
        }
        if matches!(self.slot, Slot::Opening { .. }) {
            ctx.request_repaint_after(OPEN_POLL);
            return;
        }
        let batch = self.results.drain(RESULTS_PER_FRAME);
        if !batch.is_empty() {
            self.feed.keep_shown();
        }
        let full = batch.len() == RESULTS_PER_FRAME;
        for result in batch {
            let name = result.key.to_string_lossy().into_owned();
            self.feed.apply(gallery, result, |image| {
                ctx.load_texture(name, color_image(image), TextureOptions::LINEAR)
            });
        }
        if full {
            ctx.request_repaint();
        }
    }

    /// Ask for another folder. The newest request wins: it waits until the
    /// running open is over, then opens. A request for the folder that is
    /// opening, or for the shown folder while nothing opens, cancels any
    /// waiting request and does nothing else. A request equal to the waiting
    /// one does nothing. The caller passes a normalized path.
    pub fn navigate(&mut self, folder: PathBuf) {
        if matches!(self.slot, Slot::Gone) {
            tracing::warn!("the pipeline is gone; {} cannot open", folder.display());
        }
        let settled = match &self.slot {
            Slot::Opening {
                folder: opening, ..
            } => opening == &folder,
            _ => self.current.as_ref() == Some(&folder),
        };
        if settled {
            self.pending = None;
            return;
        }
        if self.pending.as_ref() == Some(&folder) {
            return;
        }
        self.pending = Some(folder);
        (self.repaint)();
    }

    /// True when no open runs and none waits.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn is_idle(&self) -> bool {
        self.pending.is_none() && !matches!(self.slot, Slot::Opening { .. })
    }

    /// After an open ended, return the folder the gallery shows, once, so the
    /// caller can set the scope and the sidebar highlight. A failed open
    /// returns the old folder, which puts the highlight back.
    pub fn take_folder_change(&mut self) -> Option<PathBuf> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        self.current.clone()
    }

    /// Run after the gallery draws: when the painted tiles changed, tell the
    /// pipeline which ones to render first.
    pub fn after_draw(&mut self, gallery: &Gallery) {
        // While an open runs the pipeline is away; the next frame tries again.
        let Slot::Idle(pipeline) = &self.slot else {
            return;
        };
        let revision = gallery.visible_revision();
        let resync = self.feed.take_resync();
        if self.synced == Some(revision) && !resync {
            return;
        }
        let visible: HashSet<PathBuf> = gallery.visible_keys().map(Path::to_path_buf).collect();
        let again = self.feed.show(gallery.visible_keys());
        pipeline.reprioritise(&visible);
        if !again.is_empty() {
            pipeline.request(&again);
        }
        self.synced = Some(revision);
    }

    /// Once the folder is open and every tile has a texture or an error,
    /// return a one-line summary. Later calls return none.
    pub fn loaded_once(&mut self) -> Option<String> {
        if self.announced || !self.opened || !self.feed.all_settled() {
            return None;
        }
        self.announced = true;
        let (total, failed) = self.feed.counts();
        Some(format!(
            "{total} tiles settled, {failed} failed, after {:?}",
            self.started.elapsed()
        ))
    }

    /// Hand the pipeline to a thread that stops it, and give `wait` the
    /// handle. The drop blocks until renders in flight end, and a PSD render
    /// can take seconds, so the window must not wait for it.
    pub fn shutdown(&mut self, wait: &ExitWait) {
        let slot = std::mem::replace(&mut self.slot, Slot::Gone);
        let spawned = std::thread::Builder::new()
            .name("tarragon-shutdown".to_owned())
            .spawn(move || match slot {
                Slot::Idle(pipeline) => drop(pipeline),
                // The open thread returns the pipeline; dropping it ends the work.
                Slot::Opening { done, .. } => drop(done.recv()),
                Slot::Gone => {}
            });
        match spawned {
            Ok(handle) => {
                *wait
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle);
            }
            Err(error) => tracing::warn!("cannot stop the pipeline in the background: {error}"),
        }
    }

    /// Move the pipeline into a short-lived thread that scans the folder.
    fn start_open(&mut self, folder: PathBuf) {
        let Slot::Idle(mut pipeline) = std::mem::replace(&mut self.slot, Slot::Gone) else {
            tracing::warn!("an open is already running; ignoring {}", folder.display());
            return;
        };
        self.announced = false;
        self.opened = false;
        self.started = Instant::now();
        let (sender, done) = crossbeam_channel::bounded(1);
        let repaint = Arc::clone(&self.repaint);
        let target = folder.clone();
        let spawned = std::thread::Builder::new()
            .name("tarragon-open".to_owned())
            .spawn(move || {
                // This stat runs here, not on the UI thread: a dead mount can stall it.
                let result = if target.is_dir() {
                    pipeline.open_folder(&target)
                } else {
                    Err(PipelineError::NotAFolder)
                };
                // A closed window drops the answer, and the pipeline with it.
                let _ = sender.send(Done { pipeline, result });
                repaint();
            });
        match spawned {
            Ok(_) => self.slot = Slot::Opening { folder, done },
            Err(error) => {
                tracing::error!("cannot start the open thread: {error}");
                self.opened = true;
                self.changed = true;
            }
        }
    }

    /// Take the answer of the open thread, when it is ready, and show the
    /// scanned files as placeholder tiles.
    fn finish_open(&mut self, gallery: &mut Gallery) {
        let Slot::Opening { done, .. } = &self.slot else {
            return;
        };
        let answer = match done.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                tracing::error!("the open thread ended without an answer");
                self.slot = Slot::Gone;
                self.opened = true;
                return;
            }
        };
        let Slot::Opening { folder, .. } = std::mem::replace(&mut self.slot, Slot::Gone) else {
            return;
        };
        self.slot = Slot::Idle(answer.pipeline);
        // A newer request waits: the highlight already shows it, so do not
        // snap it back to this folder.
        let report = self.pending.is_none();
        match answer.result {
            Ok(opened) => {
                let keys = opened.files.iter().map(|file| file.path.clone());
                self.feed.reset(opened.generation, keys);
                let items = opened
                    .files
                    .into_iter()
                    .map(|file| GridItem::new(file.path, file.mtime, None))
                    .collect();
                gallery.set_folder(Some(folder.clone()));
                gallery.set_items(items);
                self.current = Some(folder);
                self.synced = None;
                self.opened = true;
                self.changed |= report;
            }
            Err(error) => {
                tracing::error!("cannot open {}: {error}", folder.display());
                self.opened = true;
                self.changed |= report;
            }
        }
    }
}

/// Convert a thumbnail to an egui image. The pixels are not premultiplied.
fn color_image(image: &RgbaImage) -> ColorImage {
    let size = [image.width() as usize, image.height() as usize];
    ColorImage::from_rgba_unmultiplied(size, image.as_raw())
}

#[cfg(test)]
mod tests;
