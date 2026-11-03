//! The info bar text: scope name, file count, and selection count.

use std::fmt::Write as _;
use std::path::Path;

use egui::{FontId, TextStyle, Ui};

use super::{Gallery, Scope};
use crate::tokens;

/// The mark that replaces the cut end of a long scope name.
const ELLIPSIS: char = '…';

impl Gallery {
    /// The scope name for the info bar: the folder name, or `All Images`.
    pub(super) fn scope_name(&self) -> String {
        match self.scope {
            Scope::All => Scope::All.label().to_owned(),
            Scope::Folder => self
                .folder
                .as_deref()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map_or_else(|| "No folder".to_owned(), ToOwned::to_owned),
        }
    }

    /// The info bar text after the scope name: separator, file count, and
    /// selection count.
    fn info_counts(&self) -> String {
        let count = self.visible.len();
        let noun = if count == 1 { "file" } else { "files" };
        let mut text = format!(" - {count} {noun}");
        let selected = self.selection.len();
        if selected > 0 {
            let _ = write!(text, " - {selected} selected");
        }
        text
    }

    /// The info bar text: scope name, file count, and selection count.
    #[cfg(test)]
    pub(super) fn info_text(&self) -> String {
        format!("{}{}", self.scope_name(), self.info_counts())
    }

    /// The info bar text cut to `width` points.
    ///
    /// A long scope name gives way first, so the counts stay whole. The label
    /// still truncates the text last when even the counts do not fit.
    pub(super) fn info_text_fitted(&self, ui: &Ui, width: f32) -> String {
        let font = FontId::new(
            tokens::SIZE_CAPTION,
            TextStyle::Body.resolve(ui.style()).family,
        );
        let measure = |text: &str| {
            ui.painter()
                .layout_no_wrap(text.to_owned(), font.clone(), tokens::TEXT_MUTED)
                .size()
                .x
        };
        fit_name(&self.scope_name(), &self.info_counts(), width, measure)
    }
}

/// Join `name` and `counts`, shortening `name` with an ellipsis until the
/// joined text measures at most `width`.
pub(super) fn fit_name(
    name: &str,
    counts: &str,
    width: f32,
    measure: impl Fn(&str) -> f32,
) -> String {
    let full = format!("{name}{counts}");
    if measure(&full) <= width {
        return full;
    }
    let fits = |keep: usize| {
        let head: String = name.chars().take(keep).collect();
        let text = format!("{head}{ELLIPSIS}{counts}");
        (measure(&text) <= width).then_some(text)
    };
    // The width grows with the kept length, so the search finds the longest.
    let (mut low, mut high) = (0, name.chars().count());
    let mut best = format!("{ELLIPSIS}{counts}");
    while low < high {
        let mid = (low + high).div_ceil(2);
        if let Some(text) = fits(mid) {
            best = text;
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    best
}
