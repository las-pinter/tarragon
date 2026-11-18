//! Tests for the texture budget.

use super::*;

/// No key to evict.
const NONE: [&str; 0] = [];

#[test]
fn the_budgets_are_256_and_128_mib() {
    assert_eq!(GRID_BUDGET_BYTES, 268_435_456);
    assert_eq!(PREVIEW_BUDGET_BYTES, 134_217_728);
}

#[test]
fn inserts_under_the_cap_evict_nothing() {
    let mut budget = TextureBudget::new(100);
    assert_eq!(budget.insert("a", 40), NONE);
    assert_eq!(budget.insert("b", 60), NONE);
    assert_eq!(budget.total_bytes(), 100);
    assert_eq!(budget.len(), 2);
}

#[test]
fn eviction_follows_least_recent_use() {
    let mut budget = TextureBudget::new(100);
    budget.insert("a", 40);
    budget.insert("b", 40);
    budget.insert("c", 20);
    assert_eq!(budget.insert("d", 70), ["a", "b"]);
    assert!(!budget.contains(&"a"));
    assert_eq!(budget.total_bytes(), 90);
}

#[test]
fn touch_moves_a_key_to_most_recent() {
    let mut budget = TextureBudget::new(100);
    budget.insert("a", 40);
    budget.insert("b", 40);
    budget.touch(&"a");
    assert_eq!(budget.insert("c", 40), ["b"]);
    assert!(budget.contains(&"a"));
}

#[test]
fn touching_an_unknown_key_changes_nothing() {
    let mut budget = TextureBudget::new(100);
    budget.insert("a", 40);
    budget.touch(&"zzz");
    assert_eq!(budget.len(), 1);
    assert_eq!(budget.total_bytes(), 40);
}

#[test]
fn remove_frees_the_bytes() {
    let mut budget = TextureBudget::new(100);
    budget.insert("a", 40);
    budget.insert("b", 50);
    assert_eq!(budget.remove(&"a"), Some(40));
    assert_eq!(budget.remove(&"a"), None);
    assert_eq!(budget.total_bytes(), 50);
    assert_eq!(budget.insert("c", 50), NONE, "the freed bytes fit c");
}

#[test]
fn reinserting_a_key_replaces_its_size() {
    let mut budget = TextureBudget::new(100);
    budget.insert("a", 40);
    budget.insert("b", 40);
    assert_eq!(budget.insert("a", 50), NONE);
    assert_eq!(budget.total_bytes(), 90);
    assert_eq!(budget.len(), 2);
    assert_eq!(budget.insert("c", 30), ["b"], "a is now the newer key");
}

/// Check the books: the total is the sum of sizes and both maps agree.
fn assert_consistent(budget: &TextureBudget<usize>) {
    let sum: usize = budget.entries.values().map(|&(bytes, _)| bytes).sum();
    assert_eq!(budget.total_bytes(), sum, "total differs from the sizes");
    assert_eq!(budget.by_use.len(), budget.entries.len(), "maps differ");
}

#[test]
fn the_total_never_exceeds_the_cap_after_an_insert() {
    let mut budget = TextureBudget::new(1000);
    for step in 0..500_usize {
        let size = 1 + (step * 37) % 400;
        let evicted = budget.insert(step % 23, size);
        assert!(!evicted.contains(&(step % 23)), "evicted the new key");
        assert!(budget.total_bytes() <= 1000, "over the cap at step {step}");
        assert_consistent(&budget);
        if step % 5 == 0 {
            budget.touch(&(step % 11));
        }
    }
}

#[test]
fn an_item_larger_than_the_cap_stays_alone() {
    let mut budget = TextureBudget::new(100);
    budget.insert("a", 30);
    budget.insert("b", 30);
    assert_eq!(budget.insert("huge", 500), ["a", "b"]);
    assert_eq!(budget.len(), 1);
    assert_eq!(budget.total_bytes(), 500);
    assert_eq!(budget.insert("c", 10), ["huge"], "the next insert drops it");
    assert_eq!(budget.total_bytes(), 10);
}

#[test]
fn an_oversize_insert_among_others_keeps_the_books_straight() {
    let mut budget = TextureBudget::new(100);
    budget.insert(1, 30);
    budget.insert(2, 30);
    assert_eq!(budget.insert(3, 400), [1, 2]);
    assert_consistent(&budget);
    budget.touch(&3);
    assert_consistent(&budget);
    assert_eq!(
        budget.insert(3, 20),
        Vec::<usize>::new(),
        "re-insert shrinks it"
    );
    assert_eq!(budget.total_bytes(), 20);
    assert_eq!(budget.insert(4, 80), Vec::<usize>::new());
    assert_eq!(budget.insert(5, 10), [3]);
    assert_consistent(&budget);
    assert!(budget.total_bytes() <= 100);
}
