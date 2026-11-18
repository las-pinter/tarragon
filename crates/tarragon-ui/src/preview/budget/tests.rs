//! Tests for the preview texture plan, with fake sizes.

use super::{MAX_SIDE, PreviewTextures, scaled_size, side_limit};
use crate::texture_budget::PREVIEW_BUDGET_BYTES;

const MIB: usize = 1024 * 1024;
const NONE: [&str; 0] = [];

fn wanted(names: &[&'static str], size: (u32, u32)) -> Vec<(&'static str, (u32, u32))> {
    names.iter().map(|n| (*n, size)).collect()
}

#[test]
fn a_lone_8192_image_is_scaled_down_to_fit() {
    let mut model = PreviewTextures::default();
    let plan = model.plan(&[("a", (8192, 8192))]);
    assert_eq!(plan.items[0].size, (MAX_SIDE, MAX_SIDE));
    assert!(!plan.items[0].cached);
    assert_eq!(plan.evict, NONE);
    // 4096 x 4096 RGBA is 64 MiB, half the budget.
    assert_eq!(model.budget_bytes(), 64 * MIB);
}

#[test]
fn nine_1024_images_all_fit_at_full_size() {
    let mut model = PreviewTextures::default();
    let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
    let plan = model.plan(&wanted(&names, (1024, 1024)));
    assert_eq!(plan.items.len(), 9);
    assert!(plan.items.iter().all(|i| i.size == (1024, 1024)));
    assert_eq!(plan.evict, NONE);
}

#[test]
fn nine_huge_images_share_the_budget() {
    let limit = side_limit(9, PREVIEW_BUDGET_BYTES);
    assert_eq!(limit, 1930);
    assert!(9 * limit as usize * limit as usize * 4 <= PREVIEW_BUDGET_BYTES);
    let mut model = PreviewTextures::default();
    let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
    let plan = model.plan(&wanted(&names, (8000, 6000)));
    assert!(plan.items.iter().all(|i| i.size == (1930, 1448)));
    assert_eq!(plan.evict, NONE);
}

#[test]
fn a_selection_change_evicts_the_least_recently_used_first() {
    // Three 1000 x 1000 images fill 12 MB exactly.
    let mut model = PreviewTextures::new(12_000_000);
    let img = (1000, 1000);
    assert_eq!(model.plan(&wanted(&["a", "b", "c"], img)).evict, NONE);

    let plan = model.plan(&wanted(&["d"], img));
    assert_eq!(plan.evict, ["a"]);

    // Using b again makes c the oldest.
    let plan = model.plan(&wanted(&["b", "e"], img));
    assert!(plan.items[0].cached);
    assert!(!plan.items[1].cached);
    assert_eq!(plan.evict, ["c"]);
}

#[test]
fn a_selection_that_was_held_is_not_loaded_again() {
    let mut model = PreviewTextures::new(12_000_000);
    model.plan(&wanted(&["a", "b", "c"], (1000, 1000)));
    let plan = model.plan(&wanted(&["c", "a"], (1000, 1000)));
    assert!(plan.items.iter().all(|i| i.cached));
    assert_eq!(plan.evict, NONE);
}

#[test]
fn an_image_larger_than_the_whole_budget_still_gets_a_smaller_plan() {
    let mut model = PreviewTextures::new(1000);
    let plan = model.plan(&[("a", (8192, 4096))]);
    // 15 x 15 x 4 = 900 bytes is the biggest square that fits 1000 bytes.
    assert_eq!(plan.items[0].size, (15, 8));
    let mut empty = PreviewTextures::new(0);
    assert_eq!(empty.plan(&[("a", (8192, 4096))]).items[0].size, (1, 1));
}

#[test]
fn a_plan_that_follows_an_oversize_one_evicts_it() {
    let mut model = PreviewTextures::new(1000);
    model.plan(&[("a", (8192, 8192))]);
    let plan = model.plan(&[("b", (10, 10))]);
    assert_eq!(plan.evict, ["a"]);
}

#[test]
fn scaling_keeps_the_aspect_and_never_grows_or_hits_zero() {
    assert_eq!(scaled_size((100, 50), 200), (100, 50));
    assert_eq!(scaled_size((200, 100), 200), (200, 100));
    assert_eq!(scaled_size((8192, 4096), 4096), (4096, 2048));
    assert_eq!(scaled_size((4096, 8192), 4096), (2048, 4096));
    assert_eq!(scaled_size((10_000, 1), 100), (100, 1));
    assert_eq!(scaled_size((0, 0), 10), (0, 0));
}

#[test]
fn the_side_limit_is_capped_and_at_least_one() {
    assert_eq!(side_limit(1, PREVIEW_BUDGET_BYTES), MAX_SIDE);
    assert_eq!(side_limit(0, 4 * MIB), 1024);
    assert_eq!(side_limit(100, 0), 1);
}

#[test]
fn a_removed_texture_frees_its_bytes() {
    let mut model = PreviewTextures::new(8_000_000);
    model.plan(&wanted(&["a", "b"], (1000, 1000)));
    model.remove(&"a");
    let plan = model.plan(&wanted(&["c"], (1000, 1000)));
    assert_eq!(plan.evict, NONE);
}

#[test]
fn a_held_image_at_a_larger_size_is_planned_again_and_not_evicted() {
    // 'a' is held at 1000 x 1000. Three images now share 4 MB, so the limit
    // drops to 577 and 'a' must be uploaded again at the smaller size.
    let mut model = PreviewTextures::new(4_000_000);
    model.plan(&wanted(&["a"], (1000, 1000)));
    let plan = model.plan(&wanted(&["a", "b", "c"], (1000, 1000)));
    let a = &plan.items[0];
    assert!(!a.cached);
    assert_eq!(a.size, (577, 577));
    assert_eq!(plan.evict, NONE);
    assert_eq!(model.budget_bytes(), 3 * 577 * 577 * 4);
}

#[test]
fn a_cached_item_has_the_held_size() {
    let mut model = PreviewTextures::new(12_000_000);
    model.plan(&wanted(&["a", "b"], (3000, 1000)));
    let plan = model.plan(&wanted(&["a", "b"], (3000, 1000)));
    assert!(plan.items.iter().all(|i| i.cached && i.size == (1224, 408)));
}

#[test]
fn a_repeated_key_is_planned_once_with_its_first_size() {
    let mut model = PreviewTextures::default();
    let plan = model.plan(&[("a", (100, 100)), ("b", (50, 50)), ("a", (900, 900))]);
    let keys: Vec<_> = plan.items.iter().map(|i| i.key).collect();
    assert_eq!(keys, ["a", "b"]);
    assert_eq!(plan.items[0].size, (100, 100));
    assert_eq!(model.budget_bytes(), (100 * 100 + 50 * 50) * 4);
}

#[test]
fn the_plan_keeps_every_requested_image_held_when_it_can_fit() {
    let img = (4000, 3000);
    for capacity in [0, 3, 4, 100, 1000, 12_000_000, PREVIEW_BUDGET_BYTES] {
        let mut model = PreviewTextures::new(capacity);
        let rounds: [&[&str]; 4] = [&["a", "b", "c"], &["c", "d"], &["a", "a", "e", "f"], &["f"]];
        for names in rounds {
            let plan = model.plan(&wanted(names, img));
            for key in &plan.evict {
                assert!(
                    plan.items.iter().all(|i| i.key != *key),
                    "capacity {capacity}: {key} is requested and evicted"
                );
            }
            // Below 4 bytes per image the selection cannot fit at all.
            if capacity < 4 * plan.items.len() {
                continue;
            }
            for item in &plan.items {
                let held = model.held_size(&item.key);
                assert!(held.is_some(), "capacity {capacity}: {} lost", item.key);
                assert!(
                    !item.cached || held == Some(item.size),
                    "capacity {capacity}: {} is cached at another size",
                    item.key
                );
            }
            assert!(model.budget_bytes() <= capacity, "capacity {capacity}");
        }
    }
}

#[test]
fn a_same_key_with_another_shape_is_planned_again_even_at_equal_bytes() {
    let mut model = PreviewTextures::default();
    let first = model.plan(&[("a", (3000, 1000))]);
    assert!(!first.items[0].cached);
    let second = model.plan(&[("a", (1000, 3000))]);
    assert!(!second.items[0].cached);
    assert_eq!(second.items[0].size, (1000, 3000));
    assert_eq!(model.held_size(&"a"), Some((1000, 3000)));
    assert_eq!(second.evict, NONE);
}

#[test]
fn a_selection_that_cannot_fit_marks_the_dropped_item_not_cached() {
    let mut model = PreviewTextures::new(0);
    let plan = model.plan(&wanted(&["a", "b"], (100, 100)));
    assert_eq!(plan.evict, NONE);
    assert!(plan.items.iter().all(|i| i.size == (1, 1)));
    // The first plan keeps only "b". This round "a" is inserted and pushes "b"
    // out again, so the guard clears the cached flag of "b".
    let again = model.plan(&wanted(&["a", "b"], (100, 100)));
    assert_eq!(again.items.iter().filter(|i| i.cached).count(), 0);
}
