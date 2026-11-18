use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::Receiver;
use eframe::egui::{Context, TextureHandle, TextureId};
use image::{Rgba, RgbaImage};
use tarragon_ui::preview::{Preview, PreviewImage};

use super::{PreviewFeed, route};
use crate::pipeline::Repaint;
use crate::preview_loader::{Loaded, PreviewLoader};
use crate::preview_source::{Source, SourceItem};

fn loader() -> PreviewLoader {
    let repaint: Repaint = Arc::new(|| {});
    PreviewLoader::new(repaint).expect("start the loader")
}

fn source(names: &[&str]) -> Source {
    Source {
        meta: None,
        total: names.len(),
        items: names
            .iter()
            .map(|name| SourceItem {
                key: PathBuf::from(name),
                tier: PathBuf::from(format!("/nonexistent/{name}.png")),
                size: (100, 100),
            })
            .collect(),
    }
}

fn result(revision: u64, name: &str, image: Result<RgbaImage, String>) -> Loaded {
    Loaded {
        revision,
        key: PathBuf::from(name),
        image,
    }
}

/// Wait for the loader result of `revision` and take it. Each wait is bounded.
fn revision_result(loader: &PreviewLoader, signals: &Receiver<()>, revision: u64) -> Loaded {
    loop {
        signals
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("a repaint signal");
        if let Some(found) = loader.poll().into_iter().find(|l| l.revision == revision) {
            return found;
        }
    }
}

fn picture(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_pixel(width, height, Rgba([9, 9, 9, 255]))
}

/// How many textures the context holds beyond the ones it makes itself.
fn allocated(ctx: &Context) -> usize {
    let own = Context::default().tex_manager().read().num_allocated();
    ctx.tex_manager().read().num_allocated() - own
}

fn image(id: u64) -> PreviewImage {
    PreviewImage {
        texture: TextureId::Managed(id),
        size: (1, 1),
    }
}

#[test]
fn route_sends_zero_selected_to_nothing() {
    assert_eq!(route(vec![image(1)], 0), (None, vec![], 0));
}

#[test]
fn route_sends_one_selected_to_the_single_picture() {
    assert_eq!(route(vec![image(1)], 1), (Some(image(1)), vec![], 1));
    assert_eq!(route(vec![], 1), (None, vec![], 1));
}

#[test]
fn route_sends_a_partial_mosaic_with_the_full_total() {
    let held = vec![image(1), image(2), image(3)];
    assert_eq!(route(held.clone(), 5), (None, held, 5));
}

#[test]
fn poll_uploads_a_decoded_file_and_gives_it_to_the_preview() {
    let (signal, signals) = crossbeam_channel::unbounded();
    let repaint: Repaint = Arc::new(move || {
        let _ = signal.send(());
    });
    let loader = PreviewLoader::new(repaint).expect("start the loader");
    let dir = std::env::temp_dir().join(format!("tarragon-feed-poll-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    let tier = dir.join("a.png");
    picture(40, 30).save(&tier).expect("write the png");
    let mut selection = source(&["a"]);
    selection.items[0].tier = tier;
    let (ctx, mut preview) = (Context::default(), Preview::default());
    let mut feed = PreviewFeed::<TextureHandle>::new();
    feed.select(&selection, &loader);
    // Each wait is bounded; the item ends as one signal.
    signals
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("a repaint signal");
    let showed = feed.poll(&ctx, &loader, &mut preview, 1);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(showed);
    assert_eq!(feed.held[&PathBuf::from("a")].size(), [40, 30]);
    assert!(!feed.poll(&ctx, &loader, &mut preview, 1));
}

#[test]
fn a_result_of_an_older_revision_uploads_nothing() {
    let (ctx, loader) = (Context::default(), loader());
    let mut feed = PreviewFeed::<TextureHandle>::new();
    feed.select(&source(&["a"]), &loader);
    let mut bigger = source(&["a"]);
    bigger.items[0].size = (200, 100);
    feed.select(&bigger, &loader);
    assert_eq!(feed.revision, 2);
    assert!(!feed.apply(&ctx, result(1, "a", Ok(picture(10, 10)))));
    assert!(feed.held.is_empty());
    assert!(feed.pending.contains_key(&PathBuf::from("a")));
    assert_eq!(allocated(&ctx), 0);
}

#[test]
fn an_older_failed_result_leaves_the_pending_picture_and_the_model() {
    let (ctx, loader) = (Context::default(), loader());
    let mut feed = PreviewFeed::<TextureHandle>::new();
    let mut bigger = source(&["a"]);
    bigger.items[0].size = (200, 100);
    feed.select(&source(&["a"]), &loader);
    feed.select(&bigger, &loader);
    feed.dirty = false;
    assert!(!feed.apply(&ctx, result(1, "a", Err("broken".to_owned()))));
    assert!(feed.pending.contains_key(&PathBuf::from("a")));
    assert!(!feed.dirty);
    let plan = feed.textures.plan(&[(PathBuf::from("a"), (200, 100))]);
    assert!(plan.items[0].cached);
}

#[test]
fn a_failed_decode_leaves_the_others_and_is_tried_again_on_select() {
    let (ctx, loader) = (Context::default(), loader());
    let mut feed = PreviewFeed::<TextureHandle>::new();
    let both = source(&["a", "b"]);
    feed.select(&both, &loader);
    assert!(!feed.apply(&ctx, result(1, "a", Err("broken".to_owned()))));
    assert!(feed.apply(&ctx, result(1, "b", Ok(picture(20, 20)))));
    assert!(!feed.pending.contains_key(&PathBuf::from("a")));
    assert_eq!(feed.held.len(), 1);
    // The model forgot "a" too, so it counts as new and is requested again.
    feed.select(&both, &loader);
    assert_eq!(feed.revision, 2);
    assert!(feed.pending.contains_key(&PathBuf::from("a")));
    assert_eq!(allocated(&ctx), 1);
}

#[test]
fn the_only_image_failing_still_refreshes_the_preview() {
    let (signal, signals) = crossbeam_channel::unbounded();
    let repaint: Repaint = Arc::new(move || {
        let _ = signal.send(());
    });
    let loader = PreviewLoader::new(repaint).expect("start the loader");
    let ctx = Context::default();
    let mut feed = PreviewFeed::<TextureHandle>::new();
    let mut preview = Preview::default();
    feed.select(&source(&["a"]), &loader);
    feed.apply(&ctx, result(1, "a", Ok(picture(20, 20))));
    assert!(feed.poll(&ctx, &loader, &mut preview, 1));
    feed.select(&source(&["b"]), &loader);
    // The loader fails "b" on its own thread (the tier file is missing). Take
    // that result out of the loader first, so no `poll` below can meet it early.
    let failed = revision_result(&loader, &signals, 2);
    assert!(feed.poll(&ctx, &loader, &mut preview, 1));
    assert!(!feed.poll(&ctx, &loader, &mut preview, 1));
    assert!(failed.image.is_err());
    feed.apply(&ctx, failed);
    assert!(feed.poll(&ctx, &loader, &mut preview, 1));
    assert!(!feed.poll(&ctx, &loader, &mut preview, 1));
}

#[test]
fn a_cached_reselect_with_no_request_refreshes_the_preview() {
    let (ctx, loader) = (Context::default(), loader());
    let mut feed = PreviewFeed::<TextureHandle>::new();
    let mut preview = Preview::default();
    feed.select(&source(&["a"]), &loader);
    feed.apply(&ctx, result(1, "a", Ok(picture(20, 20))));
    feed.select(&source(&["b"]), &loader);
    feed.apply(&ctx, result(2, "b", Ok(picture(20, 20))));
    assert!(feed.poll(&ctx, &loader, &mut preview, 1));
    feed.select(&source(&["a"]), &loader);
    assert_eq!(feed.revision, 2);
    assert!(feed.poll(&ctx, &loader, &mut preview, 1));
}

#[test]
fn held_handles_match_the_model_after_a_run_of_selections() {
    let (ctx, loader) = (Context::default(), loader());
    // Room for two 100 x 100 pictures.
    let mut feed = PreviewFeed::<TextureHandle>::with_capacity(80_000);
    for (revision, name) in (1..).zip(["a", "b", "c"]) {
        feed.select(&source(&[name]), &loader);
        assert!(feed.apply(&ctx, result(revision, name, Ok(picture(100, 100)))));
        assert_eq!(allocated(&ctx), feed.held.len());
    }
    assert_eq!(feed.held.len(), 2);
    assert!(!feed.held.contains_key(&PathBuf::from("a")));
    feed.forget_all();
    assert_eq!(allocated(&ctx), 0);
}
