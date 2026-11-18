use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use image::{Rgba, RgbaImage};

use super::{LoadItem, Loaded, PreviewLoader, Request, newest};
use crate::pipeline::Repaint;

const WAIT: Duration = Duration::from_secs(10);

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "tarragon-preview-loader-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        Self(dir)
    }

    fn png(&self, name: &str, width: u32, height: u32) -> PathBuf {
        let path = self.0.join(name);
        RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, 255]))
            .save(&path)
            .expect("write the png");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A loader whose repaint sends one signal per call.
fn start() -> (PreviewLoader, Receiver<()>) {
    let (signal, signals) = crossbeam_channel::unbounded();
    let repaint: Repaint = Arc::new(move || {
        let _ = signal.send(());
    });
    (
        PreviewLoader::new(repaint).expect("start the loader"),
        signals,
    )
}

fn item(key: &str, tier: &Path, size: (u32, u32)) -> LoadItem {
    LoadItem {
        key: PathBuf::from(key),
        tier: tier.to_path_buf(),
        size,
    }
}

/// Wait for `count` repaint signals, then take the results.
fn collect(loader: &PreviewLoader, signals: &Receiver<()>, count: usize) -> Vec<Loaded> {
    for _ in 0..count {
        signals.recv_timeout(WAIT).expect("a repaint signal");
    }
    loader.poll()
}

#[test]
fn shrinks_a_large_picture_and_never_enlarges() {
    let dir = TempDir::new("size");
    let png = dir.png("a.png", 200, 100);
    let (loader, signals) = start();
    loader.request(1, vec![item("small", &png, (100, 50))]);
    let small = collect(&loader, &signals, 1);
    let image = small[0].image.as_ref().expect("decoded");
    assert_eq!((image.width(), image.height()), (100, 50));
    loader.request(2, vec![item("big", &png, (400, 200))]);
    let big = collect(&loader, &signals, 1);
    let image = big[0].image.as_ref().expect("decoded");
    assert_eq!((image.width(), image.height()), (200, 100));
}

#[test]
fn a_missing_file_fails_only_its_item() {
    let dir = TempDir::new("missing");
    let png = dir.png("a.png", 20, 20);
    let (loader, signals) = start();
    let gone = dir.0.join("gone.png");
    loader.request(
        7,
        vec![item("gone", &gone, (20, 20)), item("ok", &png, (20, 20))],
    );
    let done = collect(&loader, &signals, 2);
    assert_eq!(done.len(), 2);
    assert!(done.iter().all(|one| one.revision == 7));
    let gone = done.iter().find(|one| one.key.ends_with("gone"));
    let ok = done.iter().find(|one| one.key.ends_with("ok"));
    assert!(gone.is_some_and(|one| one.image.is_err()));
    assert!(ok.is_some_and(|one| one.image.is_ok()));
}

#[test]
fn a_zero_size_gives_a_one_pixel_picture() {
    let dir = TempDir::new("zero");
    let png = dir.png("a.png", 20, 20);
    let (loader, signals) = start();
    loader.request(1, vec![item("zero", &png, (0, 0))]);
    let done = collect(&loader, &signals, 1);
    let image = done[0].image.as_ref().expect("decoded");
    assert_eq!((image.width(), image.height()), (1, 1));
}

#[test]
fn only_the_newest_queued_request_is_kept() {
    let dir = TempDir::new("newest");
    let png = dir.png("a.png", 8, 8);
    let (sender, queue) = crossbeam_channel::unbounded();
    for revision in 1..=3 {
        let items = vec![item("k", &png, (8, 8))];
        sender.send(Request { revision, items }).expect("queue");
    }
    let first = queue.recv().expect("first request");
    assert_eq!(newest(first, &queue).revision, 3);
    assert!(queue.try_recv().is_err());
}

#[test]
fn a_newer_request_ends_the_running_one_between_items() {
    let dir = TempDir::new("between");
    let png = dir.png("a.png", 8, 8);
    let (signal, signals) = crossbeam_channel::unbounded();
    let (gate, gated) = crossbeam_channel::bounded::<()>(0);
    // The first repaint waits for the gate, so the request is mid-way.
    let repaint: Repaint = Arc::new(move || {
        let _ = signal.send(());
        let _ = gated.recv_timeout(WAIT);
    });
    let loader = PreviewLoader::new(repaint).expect("start the loader");
    let items = (0..3).map(|n| item(&format!("a{n}"), &png, (8, 8)));
    loader.request(1, items.collect());
    signals.recv_timeout(WAIT).expect("the first item is done");
    loader.request(2, vec![item("b", &png, (8, 8))]);
    // A closed gate lets every later repaint through at once.
    drop(gate);
    let mut revisions = Vec::new();
    while !revisions.contains(&2) {
        signals.recv_timeout(WAIT).expect("a repaint signal");
        revisions.extend(loader.poll().iter().map(|one| one.revision));
    }
    assert_eq!(revisions.iter().filter(|&&one| one == 1).count(), 1);
}

#[test]
fn a_width_only_overflow_is_shrunk() {
    let dir = TempDir::new("width");
    let png = dir.png("a.png", 200, 10);
    let (loader, signals) = start();
    loader.request(1, vec![item("wide", &png, (100, 10))]);
    let done = collect(&loader, &signals, 1);
    let image = done[0].image.as_ref().expect("decoded");
    assert_eq!((image.width(), image.height()), (100, 10));
}

#[test]
fn repaint_runs_once_per_finished_item() {
    let dir = TempDir::new("repaint");
    let png = dir.png("a.png", 8, 8);
    let (loader, signals) = start();
    let items = (0..3)
        .map(|n| item(&format!("k{n}"), &png, (8, 8)))
        .collect();
    loader.request(1, items);
    let done = collect(&loader, &signals, 3);
    assert_eq!(done.len(), 3);
    drop(loader);
    assert!(signals.try_recv().is_err());
}

#[test]
fn dropping_the_loader_joins_the_thread() {
    let counter = Arc::new(());
    let held = Arc::clone(&counter);
    let repaint: Repaint = Arc::new(move || {
        let _ = &held;
    });
    let loader = PreviewLoader::new(repaint).expect("start the loader");
    let (done, finished) = crossbeam_channel::bounded(1);
    std::thread::spawn(move || {
        drop(loader);
        let _ = done.send(());
    });
    finished.recv_timeout(WAIT).expect("the drop returned");
    // The thread owned the only other clone. It is gone once it has ended.
    assert_eq!(Arc::strong_count(&counter), 1);
}
