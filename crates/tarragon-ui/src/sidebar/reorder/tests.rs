//! Tests for the favorite drag index math.

use super::*;

/// Three rows of height 10 from y = 0.
const ROWS: [(f32, f32); 3] = [(0.0, 10.0), (10.0, 20.0), (20.0, 30.0)];

#[test]
fn slot_at_the_top() {
    assert_eq!(drop_slot(&ROWS, 0.0), Some(0));
    assert_eq!(drop_slot(&ROWS, 4.9), Some(0));
}

#[test]
fn slot_in_the_middle() {
    assert_eq!(drop_slot(&ROWS, 5.0), Some(1));
    assert_eq!(drop_slot(&ROWS, 14.0), Some(1));
    assert_eq!(drop_slot(&ROWS, 15.0), Some(2));
}

#[test]
fn slot_at_the_bottom() {
    assert_eq!(drop_slot(&ROWS, 25.0), Some(3));
    assert_eq!(drop_slot(&ROWS, 30.0), Some(3));
}

#[test]
fn slot_outside_the_list_is_none() {
    assert_eq!(drop_slot(&ROWS, -0.1), None);
    assert_eq!(drop_slot(&ROWS, 30.1), None);
}

#[test]
fn slot_of_an_empty_list_is_none() {
    assert_eq!(drop_slot(&[], 5.0), None);
}

#[test]
fn reordered_moves_down() {
    assert_eq!(
        reordered(&['a', 'b', 'c', 'd'], 0, 3),
        Some(vec!['b', 'c', 'a', 'd'])
    );
}

#[test]
fn reordered_moves_up() {
    assert_eq!(
        reordered(&['a', 'b', 'c', 'd'], 3, 1),
        Some(vec!['a', 'd', 'b', 'c'])
    );
}

#[test]
fn reordered_moves_to_the_end() {
    assert_eq!(reordered(&['a', 'b', 'c'], 0, 3), Some(vec!['b', 'c', 'a']));
}

#[test]
fn reordered_moves_to_the_start() {
    assert_eq!(reordered(&['a', 'b', 'c'], 2, 0), Some(vec!['c', 'a', 'b']));
}

#[test]
fn reordered_own_place_is_none() {
    assert_eq!(reordered(&['a', 'b', 'c'], 1, 1), None);
    assert_eq!(reordered(&['a', 'b', 'c'], 1, 2), None);
}

#[test]
fn reordered_out_of_range_is_none() {
    assert_eq!(reordered(&['a', 'b', 'c'], 3, 0), None);
    assert_eq!(reordered(&['a', 'b', 'c'], 0, 4), None);
    assert_eq!(reordered::<char>(&[], 0, 0), None);
}

#[test]
fn reordered_is_a_permutation_with_the_item_in_place() {
    let items = [0, 1, 2, 3];
    for from in 0..=4 {
        for slot in 0..=5 {
            let result = reordered(&items, from, slot);
            if from >= 4 || slot > 4 || slot == from || slot == from + 1 {
                assert_eq!(result, None, "from {from} slot {slot}");
                continue;
            }
            let moved = result.expect("a real move");
            let at = if slot > from { slot - 1 } else { slot };
            assert_eq!(moved[at], from, "from {from} slot {slot}");
            let mut sorted = moved.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, items, "from {from} slot {slot}");
            let rest: Vec<_> = moved.iter().copied().filter(|&x| x != from).collect();
            let expected: Vec<_> = items.iter().copied().filter(|&x| x != from).collect();
            assert_eq!(rest, expected, "from {from} slot {slot}");
        }
    }
}

#[test]
fn slot_of_a_nan_pointer_is_none() {
    assert_eq!(drop_slot(&ROWS, f32::NAN), None);
}

#[test]
fn slot_in_a_gap_between_rows() {
    assert_eq!(drop_slot(&[(0.0, 10.0), (12.0, 22.0)], 11.0), Some(1));
}

#[test]
fn slot_with_rows_at_an_offset() {
    let rows = [(100.0, 110.0), (110.0, 120.0)];
    assert_eq!(drop_slot(&rows, 99.0), None);
    assert_eq!(drop_slot(&rows, 104.0), Some(0));
    assert_eq!(drop_slot(&rows, 112.0), Some(1));
    assert_eq!(drop_slot(&rows, 120.0), Some(2));
    assert_eq!(drop_slot(&rows, 121.0), None);
}
