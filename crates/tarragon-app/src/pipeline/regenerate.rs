//! Render chosen files again from their sources.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use tarragon_cache::store::{CacheStore, TierFormat};

use super::Pipeline;

impl Pipeline {
    /// Delete the cache files of these files, reset their cache links, forget
    /// their failures, and render them again. It returns how many files it
    /// queued. Paths the last open does not know are skipped.
    ///
    /// It first drops the waiting jobs of the files and waits for the renders
    /// of them that already run. It then stores the rows the writer still
    /// holds. Nothing of the old renders can write after the files go or link
    /// the old files again, and other files keep rendering meanwhile.
    ///
    /// The files go before the links. A crash between the two leaves rows that
    /// point to missing files, and the render path falls back to a render for
    /// those. A file that cannot be removed is logged and skipped. The new
    /// render writes it again.
    ///
    /// A running render is not stopped, so the wait can be long, for example
    /// seconds for a large PSD. Run it on the work thread, as `purge` runs,
    /// and never on the UI thread. A worker that waits for room in the results
    /// channel with a result of a chosen file drops that result and is not
    /// waited for. A result of an old render that is already in the channel
    /// stays there; the new result follows it.
    // Not called yet.
    #[allow(dead_code)]
    pub fn regenerate(&mut self, keys: &HashSet<PathBuf>) -> usize {
        let known: HashSet<PathBuf> = keys
            .iter()
            .filter(|key| self.jobs.contains_key(*key))
            .cloned()
            .collect();
        if known.is_empty() {
            return 0;
        }
        self.queue.settle(&known);
        self.flush();
        let store = CacheStore::new(self.cache_root.clone());
        let mut paths = Vec::with_capacity(known.len());
        for key in &known {
            if let Some(job) = self.jobs.get(key) {
                remove_tier_files(&store, &job.folder, &job.uuid, key);
            }
            paths.push(key.to_string_lossy().replace('\\', "/"));
        }
        if let Err(error) = self.db.clear_cache_links_for(&paths) {
            tracing::warn!("cannot reset the cache links: {error}");
        }
        self.retry(&known)
    }
}

/// Remove the tier files of one source, in both formats. A missing file is
/// fine; other errors are logged. A partial set goes too.
fn remove_tier_files(store: &CacheStore, folder: &str, uuid: &str, key: &Path) {
    let Some(name) = key.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    for format in TierFormat::PROBE_ORDER {
        let Ok(entry) = store.entry(folder, uuid, name, format) else {
            continue;
        };
        for file in [&entry.thumb320, &entry.preview1024, &entry.full] {
            if let Err(error) = std::fs::remove_file(file)
                && error.kind() != ErrorKind::NotFound
            {
                tracing::warn!("cannot remove the cache file {}: {error}", file.display());
            }
        }
    }
}
