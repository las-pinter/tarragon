//! The rules that turn pipeline results into tile state. It has no egui types,
//! so tests run it without a window.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use image::RgbaImage;
use tarragon_ui::grid::TileError;
use tarragon_ui::texture_budget::TextureBudget;

use crate::pipeline::{ErrorKind, ThumbResult};

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
    /// Tiles that show an error mark.
    failed: HashSet<PathBuf>,
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
            failed: HashSet::new(),
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
        self.failed.clear();
    }

    /// The number of tiles, and how many of them show an error mark.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        (self.total, self.failed.len())
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
        match result.outcome {
            Ok(image) => {
                let bytes =
                    usize::try_from(u64::from(image.width()) * u64::from(image.height()) * 4)
                        .unwrap_or(usize::MAX);
                // A full budget would drop a tile for one that is off screen,
                // and the dropped one may be in view. Keep the image back.
                let over = self.budget.total_bytes().saturating_add(bytes) > self.budget.capacity();
                if over && !self.shown.contains(&key) && !self.budget.contains(&key) {
                    self.waiting.remove(&key);
                    self.failed.remove(&key);
                    self.dropped.insert(key);
                    return Applied::Deferred;
                }
                if !tiles.set_texture(&key, upload(&image)) {
                    return Applied::Unknown;
                }
                self.waiting.remove(&key);
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
                self.waiting.remove(&key);
                self.dropped.remove(&key);
                self.budget.remove(&key);
                self.failed.insert(key);
                Applied::Failed
            }
        }
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
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::pipeline::ThumbError;

    /// A fake grid that records what the feed did.
    #[derive(Default)]
    struct Fake {
        known: HashSet<PathBuf>,
        textures: HashMap<PathBuf, u32>,
        failed: HashMap<PathBuf, TileError>,
    }

    impl Fake {
        fn with(keys: &[&str]) -> Self {
            Self {
                known: keys.iter().map(PathBuf::from).collect(),
                ..Self::default()
            }
        }
    }

    impl Tiles for Fake {
        type Texture = u32;

        fn contains(&self, key: &Path) -> bool {
            self.known.contains(key)
        }

        fn set_texture(&mut self, key: &Path, texture: u32) -> bool {
            if !self.known.contains(key) {
                return false;
            }
            self.failed.remove(key);
            self.textures.insert(key.to_path_buf(), texture);
            true
        }

        fn clear_texture(&mut self, key: &Path) {
            self.textures.remove(key);
        }

        fn mark_failed(&mut self, key: &Path, error: TileError) -> bool {
            if !self.known.contains(key) {
                return false;
            }
            self.textures.remove(key);
            self.failed.insert(key.to_path_buf(), error);
            true
        }
    }

    /// One 2x2 thumbnail is 16 bytes.
    const TILE_BYTES: usize = 16;

    fn ok(generation: u64, key: &str) -> ThumbResult {
        ThumbResult {
            generation,
            key: PathBuf::from(key),
            outcome: Ok(RgbaImage::new(2, 2)),
        }
    }

    fn err(generation: u64, key: &str, kind: ErrorKind) -> ThumbResult {
        ThumbResult {
            generation,
            key: PathBuf::from(key),
            outcome: Err(ThumbError {
                kind,
                message: "failed".to_owned(),
            }),
        }
    }

    fn feed_for(keys: &[&str], capacity: usize) -> Feed {
        let mut feed = Feed::new(capacity);
        feed.reset(3, keys.iter().map(PathBuf::from));
        feed
    }

    #[test]
    fn every_error_kind_maps_to_a_tile_error() {
        let cases = [
            (ErrorKind::Io, TileError::Read),
            (ErrorKind::Decode, TileError::Decode),
            (ErrorKind::UnsupportedFormat, TileError::Unsupported),
            (ErrorKind::TooLarge, TileError::TooLarge),
            (ErrorKind::Timeout, TileError::Timeout),
            (ErrorKind::Panicked, TileError::Other),
            (ErrorKind::InvalidName, TileError::Other),
            (ErrorKind::Cancelled, TileError::Other),
        ];
        for (kind, expected) in cases {
            assert_eq!(TileError::from(kind), expected, "{kind:?}");
        }
    }

    #[test]
    fn a_stale_generation_changes_nothing() {
        let mut feed = feed_for(&["a"], 1000);
        let mut tiles = Fake::with(&["a"]);
        let mut uploads = 0;
        let applied = feed.apply(&mut tiles, ok(2, "a"), |_| {
            uploads += 1;
            1
        });
        assert_eq!(applied, Applied::Stale);
        assert_eq!(uploads, 0, "a stale image is never uploaded");
        assert!(tiles.textures.is_empty() && tiles.failed.is_empty());
        let applied = feed.apply(&mut tiles, err(2, "a", ErrorKind::Decode), |_| 0);
        assert_eq!(applied, Applied::Stale);
        assert!(tiles.failed.is_empty());
        assert!(!feed.all_settled());
    }

    #[test]
    fn an_ok_result_sets_the_texture_and_settles_the_tile() {
        let mut feed = feed_for(&["a"], 1000);
        let mut tiles = Fake::with(&["a"]);
        assert_eq!(feed.apply(&mut tiles, ok(3, "a"), |_| 7), Applied::Textured);
        assert_eq!(tiles.textures.get(Path::new("a")), Some(&7));
        assert!(feed.all_settled());
    }

    #[test]
    fn an_error_marks_the_tile_with_the_mapped_kind() {
        let mut feed = feed_for(&["a"], 1000);
        let mut tiles = Fake::with(&["a"]);
        let applied = feed.apply(&mut tiles, err(3, "a", ErrorKind::Io), |_| 0);
        assert_eq!(applied, Applied::Failed);
        assert_eq!(tiles.failed.get(Path::new("a")), Some(&TileError::Read));
        assert!(feed.all_settled());
        assert_eq!(feed.counts(), (1, 1));
    }

    #[test]
    fn a_cancelled_result_does_not_mark_the_tile() {
        let mut feed = feed_for(&["a"], 1000);
        let mut tiles = Fake::with(&["a"]);
        let applied = feed.apply(&mut tiles, err(3, "a", ErrorKind::Cancelled), |_| 0);
        assert_eq!(applied, Applied::Cancelled);
        assert!(tiles.failed.is_empty());
        assert!(!feed.all_settled());
    }

    #[test]
    fn an_unknown_key_is_not_uploaded_into_the_budget() {
        let mut feed = feed_for(&["a"], TILE_BYTES);
        let mut tiles = Fake::with(&["a"]);
        assert_eq!(feed.apply(&mut tiles, ok(3, "a"), |_| 1), Applied::Textured);
        assert_eq!(
            feed.apply(&mut tiles, ok(3, "gone"), |_| 2),
            Applied::Unknown
        );
        assert_eq!(tiles.textures.len(), 1, "the unknown key evicted nothing");
    }

    #[test]
    fn a_shown_result_over_budget_clears_the_oldest_texture() {
        let mut feed = feed_for(&["a", "b", "c"], 2 * TILE_BYTES);
        let mut tiles = Fake::with(&["a", "b", "c"]);
        feed.apply(&mut tiles, ok(3, "a"), |_| 0);
        feed.apply(&mut tiles, ok(3, "b"), |_| 0);
        let _ = feed.show([Path::new("b"), Path::new("c")].into_iter());
        feed.apply(&mut tiles, ok(3, "c"), |_| 0);
        assert!(!tiles.textures.contains_key(Path::new("a")));
        assert!(tiles.textures.contains_key(Path::new("b")));
        assert!(tiles.textures.contains_key(Path::new("c")));
        assert!(feed.all_settled(), "a dropped tile stays settled");
    }

    #[test]
    fn a_dropped_tile_is_requested_once_when_it_is_shown() {
        let mut feed = feed_for(&["a", "b", "c"], 2 * TILE_BYTES);
        let mut tiles = Fake::with(&["a", "b", "c"]);
        feed.apply(&mut tiles, ok(3, "a"), |_| 0);
        feed.apply(&mut tiles, ok(3, "b"), |_| 0);
        let _ = feed.show([Path::new("b"), Path::new("c")].into_iter());
        feed.apply(&mut tiles, ok(3, "c"), |_| 0);
        let shown = [Path::new("b"), Path::new("a")];
        let again = feed.show(shown.into_iter());
        assert_eq!(again, HashSet::from([PathBuf::from("a")]));
        let once = feed.show(shown.into_iter());
        assert!(once.is_empty(), "the request is not repeated");
    }

    #[test]
    fn losing_a_shown_tile_asks_for_another_show() {
        let mut feed = feed_for(&["a", "b", "c"], 2 * TILE_BYTES);
        let mut tiles = Fake::with(&["a", "b", "c"]);
        feed.apply(&mut tiles, ok(3, "a"), |_| 0);
        feed.apply(&mut tiles, ok(3, "b"), |_| 0);
        // The view holds more tiles than the budget, so one must give way.
        let _ = feed.show([Path::new("a"), Path::new("b"), Path::new("c")].into_iter());
        assert!(!feed.take_resync());
        feed.apply(&mut tiles, ok(3, "c"), |_| 0);
        assert!(feed.take_resync());
        assert!(!feed.take_resync(), "the flag is taken once");
    }

    #[test]
    fn a_static_view_keeps_its_tiles_while_more_results_arrive() {
        let names: Vec<String> = (0..10).map(|n| format!("t{n}")).collect();
        let keys: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut feed = feed_for(&keys, 3 * TILE_BYTES);
        let mut tiles = Fake::with(&keys);
        for key in &keys[..2] {
            feed.apply(&mut tiles, ok(3, key), |_| 0);
        }
        let shown = [Path::new("t0"), Path::new("t1")];
        let _ = feed.show(shown.into_iter());
        let mut uploads = 0;
        for key in &keys[2..] {
            feed.keep_shown();
            feed.apply(&mut tiles, ok(3, key), |_| {
                uploads += 1;
                0
            });
            assert!(tiles.textures.contains_key(Path::new("t0")));
            assert!(tiles.textures.contains_key(Path::new("t1")));
        }
        assert!(!feed.take_resync(), "no shown tile was cleared");
        assert!(uploads == 1, "deferred keys are not uploaded: {uploads}");
        assert!(feed.all_settled(), "a deferred tile counts as settled");
        // A deferred tile that scrolls into view is asked for again.
        let again = feed.show([Path::new("t9")].into_iter());
        assert_eq!(again, HashSet::from([PathBuf::from("t9")]));
    }

    #[test]
    fn a_deferred_result_reports_deferred_and_skips_the_upload() {
        let mut feed = feed_for(&["a", "b"], TILE_BYTES);
        let mut tiles = Fake::with(&["a", "b"]);
        feed.apply(&mut tiles, ok(3, "a"), |_| 0);
        let mut uploaded = false;
        let applied = feed.apply(&mut tiles, ok(3, "b"), |_| {
            uploaded = true;
            0
        });
        assert_eq!(applied, Applied::Deferred);
        assert!(!uploaded);
        assert!(!tiles.textures.contains_key(Path::new("b")));
    }

    #[test]
    fn an_unknown_key_costs_no_upload() {
        let mut feed = feed_for(&["a"], 1000);
        let mut tiles = Fake::with(&["a"]);
        let mut uploaded = false;
        let applied = feed.apply(&mut tiles, ok(3, "gone"), |_| {
            uploaded = true;
            0
        });
        assert_eq!(applied, Applied::Unknown);
        assert!(!uploaded);
    }

    #[test]
    fn a_new_folder_forgets_the_old_state() {
        let mut feed = feed_for(&["a", "b"], 1000);
        let mut tiles = Fake::with(&["a"]);
        feed.apply(&mut tiles, ok(3, "a"), |_| 0);
        feed.reset(4, [PathBuf::from("x")]);
        assert!(!feed.all_settled());
        assert_eq!(feed.apply(&mut tiles, ok(3, "a"), |_| 0), Applied::Stale);
    }
}
