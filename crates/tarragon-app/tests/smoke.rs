//! Opens a folder of fixtures in the real binary on a virtual display.
//!
//! The test is ignored by default because it needs `xvfb-run`. It always
//! starts the binary through `xvfb-run`, so no window can reach a real
//! screen. Run it with `cargo nextest run --run-ignored only smoke`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use image::{Rgba, RgbaImage};

/// The most time the app gets to load the grid and close.
const LIMIT: Duration = Duration::from_secs(120);

/// A temp folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("tarragon-smoke-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("make the temp folder");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(relative)
}

/// A small PNG whose pixels all have `alpha`.
fn write_png(path: &Path, alpha: u8) {
    // The largest value is 39 * 6, so every channel fits in a byte.
    let channel = |value: u32| u8::try_from(value).unwrap_or(u8::MAX);
    RgbaImage::from_fn(40, 30, |x, y| {
        Rgba([channel(x * 6), channel(y * 8), 90, alpha])
    })
    .save(path)
    .expect("write a png");
}

/// The files below `root` with this name.
fn find(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                todo.push(path);
            } else if path.file_name().is_some_and(|n| n == name) {
                found.push(path);
            }
        }
    }
    found
}

#[test]
#[ignore = "needs xvfb-run"]
fn every_tile_settles_and_the_alpha_source_caches_png_tiers() {
    let dir = TempDir::new();
    let folder = dir.0.join("pics");
    std::fs::create_dir_all(&folder).expect("make the folder");
    write_png(&folder.join("plain.png"), 255);
    write_png(&folder.join("alpha.png"), 128);
    for (from, to) in [
        ("psd/rgb_8bit.psd", "doc.psd"),
        ("psd/psb_8bit.psb", "big.psb"),
        ("clip/preview.clip", "art.clip"),
        ("kra/merged.kra", "paint.kra"),
        ("psd/truncated.psd", "broken.psd"),
    ] {
        std::fs::copy(fixture(from), folder.join(to)).expect("copy a fixture");
    }
    let data = dir.0.join("data");
    std::fs::create_dir_all(&data).expect("make the data folder");

    let started = Instant::now();
    let mut child = Command::new("xvfb-run")
        .arg("-a")
        .arg(env!("CARGO_BIN_EXE_tarragon"))
        .arg(&folder)
        .env("TARRAGON_DB", data.join("tarragon.db"))
        .env("TARRAGON_CLOSE_WHEN_LOADED", "1")
        .stdin(Stdio::null())
        .spawn()
        .expect("start xvfb-run");
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll the app") {
            break status;
        }
        if started.elapsed() > LIMIT {
            let _ = child.kill();
            panic!("the app did not close within {LIMIT:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let took = started.elapsed();
    let log = std::fs::read_to_string(data.join("tarragon.log")).unwrap_or_default();
    eprintln!("smoke: exit {status}, {took:?} in all");
    eprintln!("smoke: {}", log.lines().last().unwrap_or("no log"));

    assert!(status.success(), "exit status {status}\n{log}");
    assert!(
        log.contains("7 tiles settled, 1 failed"),
        "six textures and the truncated file as an error\n{log}"
    );
    let cache = data.join("cache");
    let tiers = find(&cache, "alpha.png.png");
    assert_eq!(tiers.len(), 3, "png tiers for the alpha source: {tiers:?}");
    assert!(
        find(&cache, "plain.png.jpg").len() == 3,
        "jpeg tiers for the opaque png"
    );
    // The scan stores a row for every file; only a render stores cache paths.
    // The shutdown join runs after the window closes, so the six good renders
    // must have their paths in the database by now.
    let db = tarragon_db::Database::open(data.join("tarragon.db")).expect("open the db");
    let rows = db
        .thumbnails_for_folder(&folder.to_string_lossy())
        .expect("read rows");
    let cached = rows
        .iter()
        .filter(|row| row.thumbnail_cache_path.is_some())
        .count();
    assert_eq!((rows.len(), cached), (7, 6));
}
