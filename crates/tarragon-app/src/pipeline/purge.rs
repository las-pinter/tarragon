//! Purge the thumbnail cache: the tier files and the cache links of the rows.

use std::path::Path;

use tarragon_cache::purge::PurgeOutcome;
use tarragon_cache::store::CacheStore;

use super::{Pipeline, PipelineError};

impl Pipeline {
    /// Delete every cache file and reset the cache links of every row.
    ///
    /// The file rows stay, so the library still lists every file. Each row
    /// loses only its cache links, and the render path renders it again.
    ///
    /// It drains the pool first, so no render writes while the files go, and
    /// stores the rows the writer still holds, so none arrives after the
    /// reset. It forgets the remembered failures. `protected` is the data
    /// directory: a cache root that is that directory or its parent is
    /// refused, and nothing is deleted. Tags and favorites stay.
    ///
    /// The files go before the rows. A crash between the two leaves rows that
    /// point to missing files, and the render path falls back to a render for
    /// those. The reverse order would leave files that no row owns.
    ///
    /// A running render is not stopped, so the wait can be long: call this
    /// off the UI thread.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Cache`] when the cache root is unsafe or a
    /// file cannot be removed; the rows stay then. Returns
    /// [`PipelineError::Db`] when the link reset fails.
    pub fn purge(&mut self, protected: &Path) -> Result<PurgeOutcome, PipelineError> {
        self.drain();
        self.flush();
        self.failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let outcome = CacheStore::new(self.cache_root.clone()).purge(Some(protected))?;
        let rows = self.db.clear_cache_links()?;
        tracing::info!(
            "purged the cache: {} files, {} bytes, {rows} rows reset",
            outcome.files_removed,
            outcome.bytes_removed
        );
        Ok(outcome)
    }
}
