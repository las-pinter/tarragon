//! Purge the thumbnail cache: the tier files and the stored thumbnail rows.

use std::path::Path;

use tarragon_cache::purge::PurgeOutcome;
use tarragon_cache::store::CacheStore;

use super::{Pipeline, PipelineError};

impl Pipeline {
    /// Delete every cache file and every `thumbnails` row.
    ///
    /// It drains the pool first, so no render writes while the files go, and
    /// stores the rows the writer still holds, so none arrives after the
    /// clear. It forgets the remembered failures. `protected` is the data
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
    /// [`PipelineError::Db`] when the row clear fails.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "only tests call it until the app does")
    )]
    pub fn purge(&mut self, protected: &Path) -> Result<PurgeOutcome, PipelineError> {
        self.drain();
        self.flush();
        self.failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        let outcome = CacheStore::new(self.cache_root.clone()).purge(Some(protected))?;
        let rows = self.db.clear_thumbnails()?;
        tracing::info!(
            "purged the cache: {} files, {} bytes, {rows} rows",
            outcome.files_removed,
            outcome.bytes_removed
        );
        Ok(outcome)
    }
}
