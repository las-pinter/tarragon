//! The details of one previewed file, and the line that shows them.

/// The text for a value that is not known.
pub const UNKNOWN: &str = "Unknown";

/// The unit names, each 1024 times the one before.
const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

/// The details of one file, as the line under the picture shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    /// The file name, without the folder.
    pub name: String,
    /// The pixel width and height of the original, when known.
    pub dimensions: Option<(u32, u32)>,
    /// The file size in bytes, when known.
    pub size: Option<u64>,
    /// The format name, such as "PNG".
    pub format: String,
    /// The modified date, such as "Oct 14, 2026", when known.
    pub modified: Option<String>,
}

impl Meta {
    /// The one line under the name: pixel size, file size, format, and date,
    /// joined by " · ". A value that is not known is left out.
    #[must_use]
    pub fn line(&self) -> String {
        let parts = [
            self.dimensions.map(dimensions_text),
            self.size.map(size_text),
            Some(self.format.clone()),
            self.modified.clone(),
        ];
        parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
    }
}

/// Width and height as "2048 × 1536".
#[must_use]
pub fn dimensions_text((width, height): (u32, u32)) -> String {
    format!("{width} × {height}")
}

/// A byte count with one decimal and the largest unit that keeps the number
/// below 1024, such as "2.4 MB". Whole bytes show without a decimal.
#[must_use]
pub fn size_text(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut unit = 1;
    // Whole tenths, rounded half up. A value that rounds up to 1024 moves to
    // the next unit, so the text never reads "1024.0 KB".
    let tenths = |unit: usize| (u128::from(bytes) * 10 + (1u128 << (10 * unit - 1))) >> (10 * unit);
    while unit < UNITS.len() - 1 && tenths(unit) >= 10240 {
        unit += 1;
    }
    let tenths = tenths(unit);
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

#[cfg(test)]
mod tests;
