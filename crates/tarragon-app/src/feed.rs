//! The rules that turn pipeline results into tile state. It has no egui types,
//! so tests run it without a window.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use image::RgbaImage;
use tarragon_ui::grid::TileError;
use tarragon_ui::texture_budget::TextureBudget;

use crate::pipeline::{ErrorKind, ThumbError, ThumbResult};

impl From<ErrorKind> for TileError {
    fn from(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::Io => Self::Read,
            ErrorKind::Decode => Self::Decode,
            ErrorKind::UnsupportedFormat => Self::Unsupported,
            ErrorKind::TooLarge => Self::TooLarge,
            ErrorKind::Timeout => Self::Timeout,
            // A cancelled render never reaches a tile; `Feed::apply` drops it.
            ErrorKind::Panicked | ErrorKind::InvalidName | ErrorKind::Cancelled => Self::Other,
        }
    }
}

/// The tile calls the feed makes. The gallery implements it; tests use a fake.
pub trait Tiles {
    /// The texture type the tiles hold.
    type Texture;

    /// True when the grid has a tile for `key`.
    fn contains(&self, key: &Path) -> bool;
    /// Show `texture` on the tile. Return false for an unknown key.
    fn set_texture(&mut self, key: &Path, texture: Self::Texture) -> bool;
    /// Show the placeholder on the tile.
    fn clear_texture(&mut self, key: &Path);
    /// Show the error mark on the tile. Return false for an unknown key.
    fn mark_failed(&mut self, key: &Path, error: TileError) -> bool;
    /// Record the pixel size of the original on the tile.
    fn set_dimensions(&mut self, key: &Path, dimensions: (u32, u32));
}

/// What `apply` did with one result.
#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    /// The result belongs to an older folder.
    Stale,
    /// The render was stopped, so the tile stays as it was.
    Cancelled,
    /// The tile has a texture now.
    Textured,
    /// The budget is full and the tile is off screen, so the image was not
    /// uploaded. `show` asks for it again when the tile comes into view.
    Deferred,
    /// The tile shows an error mark.
    Failed,
    /// The key is not in the grid.
    Unknown,
}

/// Settled and total tiles of the open folder, and of the tiles that need
/// colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Settled tiles and all tiles.
    pub thumbs: (usize, usize),
    /// Settled tiles and all tiles whose job extracts colors.
    pub colors: (usize, usize),
}

/// The state behind the tiles of the open folder.
pub struct Feed {
    /// The generation of the open folder.
    generation: u64,
    /// Sizes and use order of the uploaded textures.
    budget: TextureBudget<PathBuf>,
    /// Tiles whose texture was dropped for room and not asked for again.
    dropped: HashSet<PathBuf>,
    /// Tiles that have neither a texture nor an error yet.
    waiting: HashSet<PathBuf>,
    /// How many tiles the folder has.
    total: usize,
    /// Tiles whose job extracts colors and has not settled yet.
    color_waiting: HashSet<PathBuf>,
    /// How many tiles of the folder need colors.
    color_total: usize,
    /// Tiles that show an error mark, with the error behind each mark.
    failed: HashMap<PathBuf, ThumbError>,
    /// The tiles of the last `show` call.
    shown: HashSet<PathBuf>,
    /// A shown tile lost its texture, so `show` must run again.
    resync: bool,
    /// The budget cap, kept to build a fresh budget for the next folder.
    capacity: usize,
}

impl Feed {
    /// Build a feed with a texture cap of `capacity` bytes.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            generation: 0,
            budget: TextureBudget::new(capacity),
            dropped: HashSet::new(),
            waiting: HashSet::new(),
            total: 0,
            color_waiting: HashSet::new(),
            color_total: 0,
            failed: HashMap::new(),
            shown: HashSet::new(),
            resync: false,
            capacity,
        }
    }

    /// Start a folder: results of older generations are ignored from now on.
    pub fn reset(&mut self, generation: u64, keys: impl IntoIterator<Item = PathBuf>) {
        self.generation = generation;
        self.budget = TextureBudget::new(self.capacity);
        self.dropped.clear();
        self.shown.clear();
        self.resync = false;
        self.waiting = keys.into_iter().collect();
        self.total = self.waiting.len();
        self.color_waiting.clear();
        self.color_total = 0;
        self.failed.clear();
    }

    /// Name the tiles of the folder whose job extracts colors. Call it after
    /// `reset`; keys that are not tiles are left out.
    pub fn set_color_keys(&mut self, keys: impl IntoIterator<Item = PathBuf>) {
        self.color_waiting = keys
            .into_iter()
            .filter(|key| self.waiting.contains(key))
            .collect();
        self.color_total = self.color_waiting.len();
    }

    /// How far the folder has come: settled tiles of all tiles, and settled
    /// tiles of the ones that need colors. A tile settles with a texture, an
    /// error, or a texture kept back for room.
    #[must_use]
    pub fn progress(&self) -> Progress {
        Progress {
            thumbs: (self.total - self.waiting.len(), self.total),
            colors: (
                self.color_total - self.color_waiting.len(),
                self.color_total,
            ),
        }
    }

    /// The number of tiles, and how many of them show an error mark.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        (self.total, self.failed.len())
    }

    /// The error behind the error mark of `key`, if the tile shows one.
    #[must_use]
    pub fn failure(&self, key: &Path) -> Option<&ThumbError> {
        self.failed.get(key)
    }

    /// The number of textures the budget holds now.
    #[must_use]
    pub fn live_textures(&self) -> usize {
        self.budget.len()
    }

    /// True when every tile has a texture or an error.
    #[must_use]
    pub fn all_settled(&self) -> bool {
        self.waiting.is_empty()
    }

    /// Apply one result. `upload` makes the texture and runs only for an ok
    /// result of the open folder.
    pub fn apply<T: Tiles>(
        &mut self,
        tiles: &mut T,
        result: ThumbResult,
        upload: impl FnOnce(&RgbaImage) -> T::Texture,
    ) -> Applied {
        if result.generation != self.generation {
            return Applied::Stale;
        }
        let key = result.key;
        if !tiles.contains(&key) {
            return Applied::Unknown;
        }
        // A fresh render knows the pixel size even when the texture waits.
        if let Some(dimensions) = result.dimensions {
            tiles.set_dimensions(&key, dimensions);
        }
        match result.outcome {
            Ok(image) => {
                let bytes =
                    usize::try_from(u64::from(image.width()) * u64::from(image.height()) * 4)
                        .unwrap_or(usize::MAX);
                // A full budget would drop a tile for one that is off screen,
                // and the dropped one may be in view. Keep the image back.
                let over = self.budget.total_bytes().saturating_add(bytes) > self.budget.capacity();
                if over && !self.shown.contains(&key) && !self.budget.contains(&key) {
                    self.settle(&key);
                    self.failed.remove(&key);
                    self.dropped.insert(key);
                    return Applied::Deferred;
                }
                if !tiles.set_texture(&key, upload(&image)) {
                    return Applied::Unknown;
                }
                self.settle(&key);
                self.dropped.remove(&key);
                self.failed.remove(&key);
                for old in self.budget.insert(key, bytes) {
                    tiles.clear_texture(&old);
                    self.resync |= self.shown.contains(&old);
                    self.dropped.insert(old);
                }
                Applied::Textured
            }
            Err(error) if error.kind == ErrorKind::Cancelled => Applied::Cancelled,
            Err(error) => {
                if !tiles.mark_failed(&key, error.kind.into()) {
                    return Applied::Unknown;
                }
                self.settle(&key);
                self.dropped.remove(&key);
                self.budget.remove(&key);
                self.failed.insert(key, error);
                Applied::Failed
            }
        }
    }

    /// The tile of `key` has its texture or its error, and its colors were
    /// extracted before the result was sent.
    fn settle(&mut self, key: &Path) {
        self.waiting.remove(key);
        self.color_waiting.remove(key);
    }

    /// The visible tiles changed. Mark them as just used, so the budget drops
    /// other tiles first, and return the ones that lost their texture and
    /// need it again.
    pub fn show<'a>(&mut self, visible: impl Iterator<Item = &'a Path>) -> HashSet<PathBuf> {
        self.shown.clear();
        self.resync = false;
        let mut again = HashSet::new();
        for key in visible {
            let key = key.to_path_buf();
            self.budget.touch(&key);
            if self.dropped.remove(&key) {
                again.insert(key.clone());
            }
            self.shown.insert(key);
        }
        again
    }

    /// Mark every shown tile as just used. Call it before a batch of results,
    /// so the inserts drop tiles that are off screen first.
    pub fn keep_shown(&mut self) {
        for key in &self.shown {
            self.budget.touch(key);
        }
    }

    /// True once after a shown tile lost its texture. The view has not
    /// changed then, so the caller runs `show` again by itself.
    pub fn take_resync(&mut self) -> bool {
        std::mem::take(&mut self.resync)
    }
}

#[cfg(test)]
mod tests;
