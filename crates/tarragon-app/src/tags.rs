//! The Preview tag pills and the known tag list, read from the database.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use tarragon_core::entity::{Tag, TagSource};
use tarragon_db::repository::TagUsage;
use tarragon_db::{Database, DbError};
use tarragon_ui::Shell;
use tarragon_ui::preview::{ColorState, ColorStates, Preview, TagPill};
use tarragon_ui::swatch::Swatch;

use crate::colortags::swatch_name;

mod events;
pub use events::apply_preview_events;
mod manager;
pub use manager::apply_manager_events;

/// What one refresh reads: the tags of the selected paths, then every tag.
type Read = (HashMap<String, Vec<Tag>>, Vec<TagUsage>);

/// Whether the selection changed since `seen`, which then takes the new
/// revision. The caller refreshes only on true, so a quiet frame reads nothing.
pub fn selection_changed(seen: &mut u64, revision: u64) -> bool {
    let changed = *seen != revision;
    *seen = revision;
    changed
}

/// Fill the Preview tag pills and known tags from the database.
///
/// An empty selection gives no pills. A failed read is logged and the Preview
/// keeps what it had.
pub fn refresh_preview(shell: &mut Shell, db: &Database) {
    let paths = shell.selected_paths();
    refresh_with(shell.preview_mut(), &paths, |stored| {
        Ok((db.tags_for_files(stored)?, db.all_tags()?))
    });
}

/// Like [`refresh_preview`], with the read as a parameter.
fn refresh_with(
    preview: &mut Preview,
    paths: &[PathBuf],
    read: impl FnOnce(&[String]) -> Result<Read, DbError>,
) {
    let stored: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    match read(&stored) {
        Ok((by_file, all)) => {
            preview.set_colors(color_states(&by_file, stored.len()));
            preview.set_tags(pills(&by_file, stored.len()));
            preview.set_known_tags(known_tags(all));
        }
        Err(error) => tracing::warn!("cannot read the tags of the selection: {error}"),
    }
}

/// One pill for each user tag on any selected file, in name order.
///
/// The map keys are normalised paths, so only the values count. `total` is
/// how many files were asked for: a file missing from the map has no tags.
fn pills(by_file: &HashMap<String, Vec<Tag>>, total: usize) -> Vec<TagPill> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for tags in by_file.values() {
        // A repeated name on one file must count once.
        let names: BTreeSet<&str> = tags
            .iter()
            .filter(|tag| tag.source == TagSource::User)
            .map(|tag| tag.name.as_str())
            .collect();
        for name in names {
            *counts.entry(name).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(name, count)| TagPill {
            name: name.to_owned(),
            state: if count == total {
                ColorState::All
            } else {
                ColorState::Some
            },
        })
        .collect()
}

/// How many selected files carry each colour, in `Swatch::ALL` order.
///
/// Only `AutoColor` rows count, so a user tag with a colour name changes
/// nothing. `total` is how many files were asked for.
fn color_states(by_file: &HashMap<String, Vec<Tag>>, total: usize) -> ColorStates {
    let mut states = ColorStates::default();
    for (state, swatch) in states.iter_mut().zip(Swatch::ALL) {
        let name = swatch_name(swatch);
        let count = by_file
            .values()
            .filter(|tags| {
                tags.iter()
                    .any(|tag| tag.source == TagSource::AutoColor && tag.name == name)
            })
            .count();
        *state = if count == 0 {
            ColorState::None
        } else if count == total {
            ColorState::All
        } else {
            ColorState::Some
        };
    }
    states
}

/// The names of the user tags, in name order. Colour tags are not offered.
fn known_tags(all: Vec<TagUsage>) -> Vec<String> {
    all.into_iter()
        .filter(|usage| usage.tag.source == TagSource::User)
        .map(|usage| usage.tag.name)
        .collect()
}

#[cfg(test)]
mod tests;
