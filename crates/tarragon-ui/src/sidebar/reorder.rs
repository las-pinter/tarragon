//! Index math for dragging a favorite to a new place, with no drawing.

#[cfg(test)]
mod tests;

/// The insertion slot for a pointer height, or none outside the list.
///
/// `rows` holds each row's `(top, bottom)` from top to bottom. Slot `k` is
/// before row `k`, and slot `rows.len()` is after the last row. The rows must
/// be sorted top to bottom and not overlap. A pointer on a midpoint gives the
/// slot after that row, and outside means outside first top to last bottom,
/// both ends included.
#[must_use]
pub fn drop_slot(rows: &[(f32, f32)], pointer_y: f32) -> Option<usize> {
    let (first, last) = (rows.first()?, rows.last()?);
    if !(first.0..=last.1).contains(&pointer_y) {
        return None;
    }
    let after_mid = rows
        .iter()
        .take_while(|(top, bottom)| pointer_y >= (top + bottom) / 2.0);
    Some(after_mid.count())
}

/// Move item `from` to insertion slot `slot`, or none when nothing changes.
///
/// Dropping a row on its own place, or an index out of range, changes nothing.
#[must_use]
pub fn reordered<T: Clone>(items: &[T], from: usize, slot: usize) -> Option<Vec<T>> {
    if from >= items.len() || slot > items.len() || slot == from || slot == from + 1 {
        return None;
    }
    let mut moved = items.to_vec();
    let item = moved.remove(from);
    moved.insert(if slot > from { slot - 1 } else { slot }, item);
    Some(moved)
}
