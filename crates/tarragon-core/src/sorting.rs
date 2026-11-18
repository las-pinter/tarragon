//! Natural and date sorting rules for file lists.

use core::cmp::Ordering;
use core::fmt;
use core::str::FromStr;
use std::path::Path;

use crate::entity::FileInfo;
use crate::error::CoreError;

/// The gallery sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SortMode {
    /// Natural full-path order. `name 2` sorts before `name 10`.
    #[default]
    Natural,
    /// Newest file first. The compare orders by `mtime` descending.
    Date,
}

impl SortMode {
    /// Every mode.
    pub const ALL: [Self; 2] = [Self::Natural, Self::Date];

    /// Return the storage name, for example `natural` or `date`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Natural => "natural",
            Self::Date => "date",
        }
    }

    /// Compare two files in this mode.
    ///
    /// Natural order compares the full path with [`natural_path_cmp`]. Date
    /// order puts the largest `mtime` first, so the newest file comes first.
    /// It breaks ties with natural path order. The compare is total and
    /// deterministic.
    #[must_use]
    pub fn compare(self, left: &FileInfo, right: &FileInfo) -> Ordering {
        match self {
            Self::Natural => natural_path_cmp(&left.path, &right.path),
            Self::Date => right
                .mtime
                .cmp(&left.mtime)
                .then_with(|| natural_path_cmp(&left.path, &right.path)),
        }
    }
}

impl fmt::Display for SortMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SortMode {
    type Err = CoreError;

    /// Parse a sort mode name. The compare ignores case and outer space.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownSortMode`] when the name is unknown.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "natural" => Ok(Self::Natural),
            "date" => Ok(Self::Date),
            other => Err(CoreError::UnknownSortMode(other.to_owned())),
        }
    }
}

/// Compare two strings in natural order.
///
/// A run of ASCII digits compares as a number. So `name 2` sorts before
/// `name 10`. Other text compares with Unicode lowercasing, so `char` case
/// differences do not affect the primary compare. A case-only difference
/// breaks the tie with the raw code points, so the order is total.
#[must_use]
pub fn natural_cmp(left: &str, right: &str) -> Ordering {
    let mut left_at = 0;
    let mut right_at = 0;
    while left_at < left.len() && right_at < right.len() {
        let left_char = char_at(left, left_at);
        let right_char = char_at(right, right_at);
        if left_char.is_ascii_digit() && right_char.is_ascii_digit() {
            let (left_start, left_end) = number_span(left, left_at);
            let (right_start, right_end) = number_span(right, right_at);
            match (left_end - left_start).cmp(&(right_end - right_start)) {
                Ordering::Equal => {
                    match left[left_start..left_end].cmp(&right[right_start..right_end]) {
                        Ordering::Equal => {
                            left_at = left_end;
                            right_at = right_end;
                        }
                        order => return order,
                    }
                }
                order => return order,
            }
        } else {
            match left_char.to_lowercase().cmp(right_char.to_lowercase()) {
                Ordering::Equal => {
                    left_at += left_char.len_utf8();
                    right_at += right_char.len_utf8();
                }
                order => return order,
            }
        }
    }
    match (left.len() - left_at).cmp(&(right.len() - right_at)) {
        Ordering::Equal => left.cmp(right),
        order => order,
    }
}

/// Compare two paths in natural order.
///
/// The whole path is the key, so a parent folder decides the order before the
/// file name. The compare lowercases with Unicode rules.
#[must_use]
pub fn natural_path_cmp(left: &Path, right: &Path) -> Ordering {
    natural_cmp(&left.to_string_lossy(), &right.to_string_lossy())
}

/// Return the first `char` of `text` at the byte offset `at`.
fn char_at(text: &str, at: usize) -> char {
    text[at..]
        .chars()
        .next()
        .expect("offset is a char boundary")
}

/// Return the significant digit span of the number that starts at `start`.
///
/// Leading zeros are not significant, so `007` and `7` have the same value.
fn number_span(text: &str, start: usize) -> (usize, usize) {
    let end = text[start..]
        .find(|char: char| !char.is_ascii_digit())
        .map_or(text.len(), |offset| start + offset);
    let mut first = start;
    while first < end && text.as_bytes()[first] == b'0' {
        first += 1;
    }
    (first, end)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::entity::FileInfo;
    use crate::sorting::{SortMode, natural_cmp};

    fn file(name: &str, mtime: i64) -> FileInfo {
        FileInfo {
            path: PathBuf::from(name),
            size: 0,
            mtime,
            width: 0,
            height: 0,
        }
    }

    #[test]
    fn natural_order_puts_name_2_before_name_10() {
        assert!(natural_cmp("name 2", "name 10").is_lt());
    }

    #[test]
    fn natural_order_sorts_a_name_list() {
        let mut names = ["name 10", "name 1", "name 2"];
        names.sort_by(|left, right| natural_cmp(left, right));
        assert_eq!(names, ["name 1", "name 2", "name 10"]);
    }

    #[test]
    fn natural_order_treats_leading_zeros_as_a_number() {
        // 10 is greater than 9, though '0' is smaller than '9'.
        assert!(natural_cmp("name 010", "name 9").is_gt());
    }

    #[test]
    fn natural_order_ignores_case_for_the_primary_compare() {
        assert!(natural_cmp("alpha 2", "BETA 10").is_lt());
    }

    #[test]
    fn natural_order_breaks_a_case_only_tie() {
        let mut names = ["alpha", "ALPHA"];
        names.sort_by(|left, right| natural_cmp(left, right));
        assert_eq!(names, ["ALPHA", "alpha"]);
    }

    #[test]
    fn natural_order_lowers_non_ascii_case() {
        // 'Ä' and 'ä' differ by case only, so the raw code points break the tie.
        assert_eq!(natural_cmp("Ä", "ä"), std::cmp::Ordering::Less);
        assert_eq!(natural_cmp("ä", "Ä"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn natural_order_compares_the_full_path_across_folders() {
        let early = file("/a/B.png", 0);
        let late = file("/z/A.png", 0);
        // The parent folder decides, not the file name.
        assert_eq!(
            SortMode::Natural.compare(&early, &late),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn natural_order_compares_path_digits_as_a_number() {
        let early = file("/frames/2.png", 0);
        let late = file("/frames/10.png", 0);
        assert_eq!(
            SortMode::Natural.compare(&early, &late),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn date_order_puts_the_newest_file_first() {
        let old = file("old.png", 100);
        let new = file("new.png", 200);
        assert_eq!(
            SortMode::Date.compare(&old, &new),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn date_order_breaks_a_tie_with_the_name() {
        let first = file("a.png", 100);
        let second = file("b.png", 100);
        assert_eq!(
            SortMode::Date.compare(&first, &second),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn sort_mode_round_trips_through_its_name() {
        for mode in SortMode::ALL {
            let parsed = mode.as_str().parse::<SortMode>().expect("name parses");
            assert_eq!(parsed, mode);
        }
    }

    #[test]
    fn natural_is_the_default_sort_mode() {
        assert_eq!(SortMode::default(), SortMode::Natural);
    }

    #[test]
    fn unknown_sort_mode_name_is_refused() {
        let result = "size".parse::<SortMode>();
        assert!(result.is_err(), "unknown mode parsed: {result:?}");
    }
}
