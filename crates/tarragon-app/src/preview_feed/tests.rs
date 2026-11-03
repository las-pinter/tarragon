use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use image::{Rgba, RgbaImage};

use super::PreviewFeed;
use crate::pipeline::Repaint;
use crate::preview_loader::{Loaded, PreviewLoader};
use crate::preview_source::{Source, SourceItem};

const WAIT: Duration = Duration::from_secs(10);

/// A stand-in for a texture handle.
struct Tex;

/// A loader whose repaint sends one signal per finished item. The tier files
/// do not exist, so each item ends as a failed result, which is enough to count.
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

fn source(items: &[(&str, (u32, u32))]) -> Source {
    Source {
        meta: None,
        total: items.len(),
        items: items
            .iter()
            .map(|(name, size)| SourceItem {
                key: PathBuf::from(name),
                tier: PathBuf::from(format!("/nonexistent/{name}.png")),
                size: *size,
            })
            .collect(),
    }
}

/// Wait for `count` results and return them.
fn collect(loader: &PreviewLoader, signals: &Receiver<()>, count: usize) -> Vec<Loaded> {
    for _ in 0..count {
        signals.recv_timeout(WAIT).expect("a repaint signal");
    }
    loader.poll()
}

/// Hand the feed the texture of a pending picture, as the loader result would.
fn give(feed: &mut PreviewFeed<Tex>, name: &str, size: (u32, u32)) {
    assert!(feed.accept(Path::new(name), size, Tex));
}

/// Collect results until `count` of them have `revision`. Each wait is bounded.
fn collect_revision(
    loader: &PreviewLoader,
    signals: &Receiver<()>,
    revision: u64,
    count: usize,
) -> Vec<Loaded> {
    let mut found = Vec::new();
    while found.len() < count {
        signals.recv_timeout(WAIT).expect("a repaint signal");
        found.extend(loader.poll().into_iter().filter(|l| l.revision == revision));
    }
    found
}

fn holds(feed: &PreviewFeed<Tex>, name: &str) -> bool {
    feed.held.contains_key(&PathBuf::from(name))
}

#[test]
fn two_new_items_make_one_request_at_their_planned_sizes() {
    let (loader, signals) = start();
    // Two pictures share 80_000 bytes, so the longest side is at most 100.
    let mut feed = PreviewFeed::<Tex>::with_capacity(80_000);
    let dir = std::env::temp_dir().join(format!("tarragon-preview-feed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    let mut items = Vec::new();
    for (name, size) in [("a", (400, 200)), ("b", (50, 60))] {
        let tier = dir.join(format!("{name}.png"));
        RgbaImage::from_pixel(size.0, size.1, Rgba([1, 2, 3, 255]))
            .save(&tier)
            .expect("write the png");
        items.push(SourceItem {
            key: PathBuf::from(name),
            tier,
            size,
        });
    }
    let selection = Source {
        meta: None,
        total: 2,
        items,
    };
    feed.select(&selection, &loader);
    let mut results = collect(&loader, &signals, 2);
    let _ = std::fs::remove_dir_all(&dir);
    results.sort_by(|x, y| x.key.cmp(&y.key));
    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .all(|loaded| loaded.revision == feed.revision)
    );
    let dims = |loaded: &Loaded| {
        let image = loaded.image.as_ref().expect("decoded");
        (image.width(), image.height())
    };
    assert_eq!(dims(&results[0]), (100, 50));
    assert_eq!(dims(&results[1]), (50, 60));
    assert_eq!(feed.wanted, [PathBuf::from("a"), PathBuf::from("b")]);
}

#[test]
fn the_same_item_again_is_cached_and_keeps_its_handle() {
    let (loader, signals) = start();
    let mut feed = PreviewFeed::<Tex>::new();
    let one = source(&[("a", (100, 100))]);
    feed.select(&one, &loader);
    assert_eq!(collect(&loader, &signals, 1).len(), 1);
    give(&mut feed, "a", (100, 100));
    feed.select(&one, &loader);
    assert!(holds(&feed, "a"));
    assert_eq!(feed.revision, 1);
}

#[test]
fn an_item_still_loading_is_requested_again_with_the_new_one() {
    let (loader, signals) = start();
    let mut feed = PreviewFeed::<Tex>::new();
    feed.select(&source(&[("a", (100, 100))]), &loader);
    feed.select(&source(&[("a", (100, 100)), ("b", (30, 30))]), &loader);
    assert_eq!(feed.revision, 2);
    let mut keys: Vec<_> = collect_revision(&loader, &signals, 2, 2)
        .into_iter()
        .map(|loaded| loaded.key)
        .collect();
    keys.sort();
    assert_eq!(keys, [PathBuf::from("a"), PathBuf::from("b")]);
}

#[test]
fn a_handle_for_a_key_that_left_the_selection_is_refused() {
    let (loader, signals) = start();
    let mut feed = PreviewFeed::<Tex>::new();
    feed.select(&source(&[("a", (100, 100))]), &loader);
    feed.select(&source(&[("b", (30, 30))]), &loader);
    assert!(!feed.accept(Path::new("a"), (100, 100), Tex));
    assert!(!feed.accept(Path::new("b"), (31, 30), Tex));
    assert!(!holds(&feed, "a"));
    assert!(!holds(&feed, "b"));
    assert_eq!(collect_revision(&loader, &signals, 2, 1).len(), 1);
}

#[test]
fn an_item_that_left_while_loading_is_requested_again_when_it_returns() {
    let (loader, signals) = start();
    let mut feed = PreviewFeed::<Tex>::new();
    feed.select(&source(&[("a", (100, 100))]), &loader);
    feed.select(&source(&[("b", (30, 30))]), &loader);
    feed.select(&source(&[("a", (100, 100))]), &loader);
    assert_eq!(feed.revision, 3);
    assert!(feed.pending.contains_key(Path::new("a")));
    let results = collect_revision(&loader, &signals, 3, 1);
    assert_eq!(results[0].key, PathBuf::from("a"));
}

#[test]
fn a_changed_planned_size_requests_again_and_drops_the_old_handle() {
    let (loader, signals) = start();
    let mut feed = PreviewFeed::<Tex>::new();
    feed.select(&source(&[("a", (100, 100))]), &loader);
    assert_eq!(collect(&loader, &signals, 1).len(), 1);
    give(&mut feed, "a", (100, 100));
    feed.select(&source(&[("a", (200, 100))]), &loader);
    assert!(!holds(&feed, "a"));
    let results = collect(&loader, &signals, 1);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].revision, 2);
}

#[test]
fn an_item_pushed_out_of_the_budget_loses_its_handle() {
    let (loader, signals) = start();
    // Room for two 100 x 100 pictures.
    let mut feed = PreviewFeed::<Tex>::with_capacity(80_000);
    for name in ["a", "b"] {
        feed.select(&source(&[(name, (100, 100))]), &loader);
        assert_eq!(collect(&loader, &signals, 1).len(), 1);
        give(&mut feed, name, (100, 100));
    }
    feed.select(&source(&[("c", (100, 100))]), &loader);
    assert!(!holds(&feed, "a"));
    assert!(holds(&feed, "b"));
    assert_eq!(collect(&loader, &signals, 1).len(), 1);
}

#[test]
fn a_one_byte_budget_drops_the_handles_the_model_forgot() {
    let (loader, signals) = start();
    let mut feed = PreviewFeed::<Tex>::with_capacity(1);
    feed.select(&source(&[("a", (100, 100))]), &loader);
    assert_eq!(collect(&loader, &signals, 1).len(), 1);
    // One pixel is the smallest plan, so the planned size is (1, 1).
    give(&mut feed, "a", (1, 1));
    assert!(holds(&feed, "a"));
    feed.select(&source(&[("a", (100, 100)), ("b", (30, 30))]), &loader);
    assert!(!holds(&feed, "a"));
    assert!(!holds(&feed, "b"));
    assert_eq!(collect_revision(&loader, &signals, 2, 2).len(), 2);
}

#[test]
fn forget_all_drops_the_handles_and_the_plan() {
    let (loader, signals) = start();
    let mut feed = PreviewFeed::<Tex>::new();
    feed.select(&source(&[("a", (100, 100))]), &loader);
    assert_eq!(collect(&loader, &signals, 1).len(), 1);
    give(&mut feed, "a", (100, 100));
    feed.select(&source(&[("a", (100, 100)), ("b", (30, 30))]), &loader);
    feed.forget_all();
    assert!(feed.held.is_empty());
    assert!(feed.pending.is_empty());
    assert_eq!(feed.wanted.len(), 0);
    assert!(!feed.accept(Path::new("b"), (30, 30), Tex));
    // The model forgot "a", so the same item loads again.
    feed.select(&source(&[("a", (100, 100))]), &loader);
    assert_eq!(collect_revision(&loader, &signals, 3, 1).len(), 1);
}
