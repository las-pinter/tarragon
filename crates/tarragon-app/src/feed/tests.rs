use std::collections::{HashMap, HashSet};

use super::*;
use crate::pipeline::ThumbError;

/// A fake grid that records what the feed did.
#[derive(Default)]
struct Fake {
    known: HashSet<PathBuf>,
    textures: HashMap<PathBuf, u32>,
    failed: HashMap<PathBuf, TileError>,
    dimensions: HashMap<PathBuf, (u32, u32)>,
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

    fn set_dimensions(&mut self, key: &Path, dimensions: (u32, u32)) {
        self.dimensions.insert(key.to_path_buf(), dimensions);
    }
}

/// One 2x2 thumbnail is 16 bytes.
const TILE_BYTES: usize = 16;

fn ok(generation: u64, key: &str) -> ThumbResult {
    ThumbResult {
        generation,
        key: PathBuf::from(key),
        outcome: Ok(RgbaImage::new(2, 2)),
        dimensions: None,
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
        dimensions: None,
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
fn a_fresh_render_gives_the_tile_its_pixel_size() {
    let mut feed = feed_for(&["a"], 1000);
    let mut tiles = Fake::with(&["a"]);
    let fresh = ThumbResult {
        dimensions: Some((640, 480)),
        ..ok(3, "a")
    };
    feed.apply(&mut tiles, fresh, |_| 7);
    let unknown = ThumbResult {
        dimensions: Some((1, 1)),
        ..ok(3, "gone")
    };
    feed.apply(&mut tiles, unknown, |_| 8);
    assert_eq!(tiles.dimensions.len(), 1, "an unknown key gets no size");
    assert_eq!(tiles.dimensions.get(Path::new("a")), Some(&(640, 480)));
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

#[test]
fn progress_counts_settled_tiles_and_the_ones_that_need_colors() {
    let mut feed = feed_for(&["a", "b", "c", "d"], 1000);
    // "x" is not a tile, so it does not count.
    feed.set_color_keys(["b", "c", "x"].map(PathBuf::from));
    let mut tiles = Fake::with(&["a", "b", "c", "d"]);
    assert_eq!(
        feed.progress(),
        Progress {
            thumbs: (0, 4),
            colors: (0, 2)
        }
    );

    feed.apply(&mut tiles, ok(3, "a"), |_| 1);
    feed.apply(&mut tiles, err(3, "b", ErrorKind::Decode), |_| 0);
    // A stopped render settles nothing.
    feed.apply(&mut tiles, err(3, "c", ErrorKind::Cancelled), |_| 0);

    assert_eq!(
        feed.progress(),
        Progress {
            thumbs: (2, 4),
            colors: (1, 2)
        }
    );
}

#[test]
fn a_kept_back_texture_settles_its_colors_and_a_reset_clears_them() {
    let mut feed = feed_for(&["a", "b"], TILE_BYTES);
    feed.set_color_keys(["a", "b"].map(PathBuf::from));
    let mut tiles = Fake::with(&["a", "b"]);
    feed.apply(&mut tiles, ok(3, "a"), |_| 1);
    // The budget is full and "b" is off screen, so its texture waits.
    assert_eq!(feed.apply(&mut tiles, ok(3, "b"), |_| 2), Applied::Deferred);
    assert_eq!(feed.progress().colors, (2, 2));

    feed.reset(4, [PathBuf::from("a")]);

    assert_eq!(
        feed.progress(),
        Progress {
            thumbs: (0, 1),
            colors: (0, 0)
        }
    );
}
