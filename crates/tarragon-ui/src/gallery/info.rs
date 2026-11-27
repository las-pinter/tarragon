//! The gallery header text: the breadcrumb, the folder name, and the counts.

use std::path::Path;

use super::{Gallery, Scope};

/// The parts of the path above the folder that the breadcrumb shows.
const BREADCRUMB_PARTS: usize = 2;

impl Gallery {
    /// The scope name for the header: the folder name, or `All images`.
    pub(super) fn scope_name(&self) -> String {
        match self.scope {
            Scope::All => Scope::All.label().to_owned(),
            Scope::Folder => self
                .opening
                .folder()
                .or(self.folder.as_deref())
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map_or_else(|| "No folder".to_owned(), ToOwned::to_owned),
        }
    }

    /// The breadcrumb above the heading: the last folder parts of the path,
    /// or none when the folder sits at the root or no folder is open.
    pub(super) fn breadcrumb(&self) -> Option<String> {
        if self.scope == Scope::All {
            return None;
        }
        let folder = self.opening.folder().or(self.folder.as_deref())?;
        let mut parts: Vec<&str> = folder
            .parent()?
            .components()
            .filter_map(|part| match part {
                std::path::Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect();
        if parts.is_empty() {
            return None;
        }
        let start = parts.len().saturating_sub(BREADCRUMB_PARTS);
        parts.drain(..start);
        Some(parts.join(" / "))
    }

    /// The count beside the heading: the files the filters show, and the files
    /// in the scope when a filter hides some.
    pub(super) fn count_text(&self) -> String {
        let scoped = self.scoped_len();
        let visible = self.visible_len();
        let noun = if scoped == 1 { "file" } else { "files" };
        if visible == scoped {
            format!("{scoped} {noun}")
        } else {
            format!("{visible} of {scoped} {noun}")
        }
    }
}
