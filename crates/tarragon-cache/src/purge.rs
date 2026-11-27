//! Cache purge and the `clear_full_res_on_exit` rule.
//!
//! The manual purge deletes the three tier folders under the cache root and
//! nothing else. A tier folder counts only when it holds the format marker
//! that the store writes, so a user folder named like a tier stays. The caller
//! cancels the pending renders and drains the worker pool first, so no render
//! writes while the purge deletes. The cache links of the rows are reset in a
//! separate call, because the cache crate cannot reach the database.
//!
//! The shutdown rule clears only the `full` tier when the setting is true. The
//! `320` and `1024` tiers always stay. A false setting deletes nothing.
//!
//! Every delete reuses the `is_within_root` guard, so a path that resolves
//! outside the cache root is never removed. The walk never follows a symlink.
//! A symlink inside the tree is unlinked in place, and a symlinked tier
//! directory is refused.
//!
//! The caller passes the application data directory as `protected`. The purge
//! refuses a cache root that is the data directory or an ancestor of it, so
//! the purge cannot delete `tarragon.db`. The cache crate cannot resolve
//! the data path itself, so the caller supplies it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tarragon_core::tier::Tier;

use crate::CacheError;
use crate::eviction::is_within_root;
use crate::store::CacheStore;

/// The result of one purge or tier clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurgeOutcome {
    /// The number of regular files and links removed.
    pub files_removed: u64,
    /// The byte size of the removed files. A removed link counts zero bytes.
    pub bytes_removed: u64,
}

/// How far a purge has come, in bytes, for another thread to show.
///
/// The total comes from a walk of the tiers before the first delete, so it
/// is zero until that walk ends. A file that a render writes after the walk
/// is still removed, so `done` can pass `total` by a little.
#[derive(Debug, Default)]
pub struct PurgeProgress {
    total: AtomicU64,
    done: AtomicU64,
}

impl PurgeProgress {
    /// The bytes the tiers held when the purge began, or zero before the
    /// walk ends.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// The bytes removed so far.
    #[must_use]
    pub fn done(&self) -> u64 {
        self.done.load(Ordering::Relaxed)
    }

    /// Set both values back to zero for the next purge.
    pub fn reset(&self) {
        self.total.store(0, Ordering::Relaxed);
        self.done.store(0, Ordering::Relaxed);
    }
}

impl CacheStore {
    /// Delete the cache tiers (`320`, `1024`, `full`) and their markers, and
    /// prune the empty directories. The cache root itself stays. A tier folder
    /// without the `format=1` marker is left alone, and a marker is removed
    /// only after the rest of its tier, so a failed purge can run again.
    ///
    /// Anything else in the cache root is left untouched. The user can point
    /// the cache at a folder that holds other files, and a purge must never
    /// delete those. Refusing such a root instead would leave the user unable
    /// to purge at all.
    ///
    /// This is the filesystem half of the purge. The caller cancels the
    /// pending renders and drains the worker pool first. The
    /// `DELETE FROM thumbnails` clear is a separate call on the database.
    ///
    /// A missing cache root gives a zero outcome. An unsafe root, for example
    /// the filesystem root, is refused. A root that is `protected`, or an
    /// ancestor of it, is refused. The caller passes the application data
    /// directory as `protected`, so the purge cannot delete `tarragon.db`.
    /// A `None` value skips the data-dir guard.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::UnsafeRoot`] when the root is unsafe to purge.
    /// Returns [`CacheError::Io`] when a directory read or file remove fails.
    pub fn purge(&self, protected: Option<&Path>) -> Result<PurgeOutcome, CacheError> {
        self.purge_with(protected, &PurgeProgress::default())
    }

    /// Purge as [`CacheStore::purge`] does, and report the bytes in
    /// `progress`: the total of the tiers it will clear, then each removed
    /// file. It does not reset `progress`; the caller does that first.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`CacheStore::purge`].
    pub fn purge_with(
        &self,
        protected: Option<&Path>,
        progress: &PurgeProgress,
    ) -> Result<PurgeOutcome, CacheError> {
        let Some(resolved) = self.resolve_root()? else {
            return Ok(PurgeOutcome::default());
        };
        if !is_safe_purge_root(&resolved, protected) {
            return Err(CacheError::UnsafeRoot(resolved));
        }
        let mut total = 0u64;
        for tier in Tier::ALL {
            // A tier that cannot be checked adds nothing here. The clear below
            // meets the same error after the tiers before it, as it did
            // before the total existed.
            if let Ok(Some((tier_dir, _))) = self.clearable_tier(&resolved, tier) {
                total = total.saturating_add(tree_bytes(&tier_dir));
            }
        }
        progress.total.store(total, Ordering::Relaxed);
        let mut outcome = PurgeOutcome::default();
        for tier in Tier::ALL {
            let tier_outcome = self.clear_tier_in(&resolved, tier, &progress.done)?;
            outcome.files_removed += tier_outcome.files_removed;
            outcome.bytes_removed += tier_outcome.bytes_removed;
            // The tier folder is cache-owned, so drop it once it is empty.
            let _ = std::fs::remove_dir(resolved.join(tier.as_str()));
        }
        Ok(outcome)
    }

    /// Delete every file under one tier and prune its empty directories. The
    /// tier root itself stays. A tier folder without the `format=1` marker is
    /// left alone.
    ///
    /// A missing tier gives a zero outcome. A tier directory that resolves
    /// outside the cache root deletes nothing. A tier directory that is a
    /// symlink is refused, so `full -> 320` cannot empty the `320` tier. The
    /// canonical tier directory must be exactly `<root>/<tier>`.
    ///
    /// A cache root that is `protected`, or an ancestor of it, is refused.
    /// The caller passes the application data directory as `protected`.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::UnsafeRoot`] when the cache root is unsafe.
    /// Returns [`CacheError::Io`] when a directory read or file remove fails.
    pub fn clear_tier(
        &self,
        tier: Tier,
        protected: Option<&Path>,
    ) -> Result<PurgeOutcome, CacheError> {
        let Some(resolved_root) = self.resolve_root()? else {
            return Ok(PurgeOutcome::default());
        };
        if !is_safe_purge_root(&resolved_root, protected) {
            return Err(CacheError::UnsafeRoot(resolved_root));
        }
        self.clear_tier_in(&resolved_root, tier, &AtomicU64::new(0))
    }

    /// Clear the `full` tier when `enabled` is true. A false value deletes
    /// nothing.
    ///
    /// This is the shutdown rule. The `320` and `1024` tiers always stay.
    /// The caller drains the worker pool first, as [`CacheStore::purge`] does.
    /// The caller passes the application data directory as `protected`, so the
    /// clear cannot touch `tarragon.db`.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::UnsafeRoot`] when the cache root is unsafe.
    /// Returns [`CacheError::Io`] when a directory read or file remove fails.
    pub fn clear_full_res_on_exit(
        &self,
        enabled: bool,
        protected: Option<&Path>,
    ) -> Result<PurgeOutcome, CacheError> {
        if !enabled {
            return Ok(PurgeOutcome::default());
        }
        self.clear_tier(Tier::Full, protected)
    }

    /// Clear one tier folder below the already resolved cache root, and add
    /// each removed file's bytes to `done`.
    fn clear_tier_in(
        &self,
        resolved_root: &Path,
        tier: Tier,
        done: &AtomicU64,
    ) -> Result<PurgeOutcome, CacheError> {
        let Some((tier_dir, marker)) = self.clearable_tier(resolved_root, tier)? else {
            return Ok(PurgeOutcome::default());
        };
        // The marker goes last, after every other file is gone. A clear that
        // fails partway keeps it, so the next purge still knows the folder.
        let (mut files_removed, mut bytes_removed) =
            remove_tree_contents(&tier_dir, &marker, done)?;
        // A link counts zero bytes, as in the walk.
        let size = std::fs::symlink_metadata(&marker)
            .map_or(0, |meta| if meta.is_file() { meta.len() } else { 0 });
        match std::fs::remove_file(&marker) {
            Ok(()) => {
                files_removed += 1;
                bytes_removed = bytes_removed.saturating_add(size);
                done.fetch_add(size, Ordering::Relaxed);
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(CacheError::Io {
                    path: marker,
                    source,
                });
            }
        }
        Ok(PurgeOutcome {
            files_removed,
            bytes_removed,
        })
    }

    /// The folder and the marker of `tier` when the purge may clear it: a
    /// real folder at exactly `<root>/<tier>` that holds the format marker.
    /// `None` leaves the tier alone.
    fn clearable_tier(
        &self,
        resolved_root: &Path,
        tier: Tier,
    ) -> Result<Option<(PathBuf, PathBuf)>, CacheError> {
        let tier_dir = self.root().join(tier.as_str());
        // A symlinked tier directory can point at another tier. Refuse it, so
        // the clear cannot empty the wrong directory.
        match std::fs::symlink_metadata(&tier_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Ok(None);
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(source) => {
                return Err(CacheError::Io {
                    path: tier_dir,
                    source,
                });
            }
        }
        // The canonical tier directory must be exactly `<root>/<tier>`, so a
        // link cannot redirect the clear.
        let expected = resolved_root.join(tier.as_str());
        match dunce::canonicalize(&tier_dir) {
            Ok(canonical) if canonical == expected => {}
            Ok(_) => return Ok(None),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(source) => {
                return Err(CacheError::Io {
                    path: tier_dir,
                    source,
                });
            }
        }
        // A folder that only shares a tier name, with no format marker, was not
        // made by the store and may hold user files, so it stays.
        let marker = self.marker_path(tier);
        if !marker.is_file() {
            return Ok(None);
        }
        Ok(Some((tier_dir, marker)))
    }

    /// Resolve the cache root to an absolute path. A missing root gives
    /// `None`.
    fn resolve_root(&self) -> Result<Option<PathBuf>, CacheError> {
        match dunce::canonicalize(self.root()) {
            Ok(resolved) => Ok(Some(resolved)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(CacheError::Io {
                path: self.root().to_path_buf(),
                source,
            }),
        }
    }
}

/// Return true when `resolved` is safe to purge recursively.
///
/// Refuses the filesystem root and any ancestor of the home directory, so a
/// misconfigured cache root cannot delete the wrong tree.
///
/// Refuses `resolved` when it is `protected` or an ancestor of `protected`.
/// The caller passes the application data directory as `protected`, so the
/// purge cannot delete `tarragon.db`. A missing `protected` path is compared
/// by its literal components.
fn is_safe_purge_root(resolved: &Path, protected: Option<&Path>) -> bool {
    is_safe_with_home(resolved, protected, home_dir().as_deref())
}

/// The root check with the home directory passed in, so a test can set it.
fn is_safe_with_home(resolved: &Path, protected: Option<&Path>, home: Option<&Path>) -> bool {
    if resolved.parent().is_none() {
        return false;
    }
    if let Some(home) = home.and_then(|home| dunce::canonicalize(home).ok()) {
        // home.starts_with(resolved) is true when resolved is an ancestor.
        if home.starts_with(resolved) {
            return false;
        }
    }
    if let Some(protected) = protected {
        let protected = dunce::canonicalize(protected).unwrap_or_else(|_| protected.to_path_buf());
        // protected.starts_with(resolved) is true when resolved is an
        // ancestor of the data directory, or equal to it.
        if protected.starts_with(resolved) {
            return false;
        }
    }
    true
}

/// Return the home directory from `HOME` or `USERPROFILE`.
///
/// `HOME` is set on Unix and `USERPROFILE` on Windows. The guard has no
/// fallback when both variables are unset, so the home check is absent then.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Remove every file under `root` except `keep`, and prune the empty
/// subdirectories. The `root` directory itself stays.
///
/// The walk never follows a symlink, so a link cannot escape the tree. A
/// symlink inside the tree is unlinked in place, never followed, so "delete
/// every file" holds and a broken link is removed. A regular path that
/// resolves outside the root is skipped, as [`is_within_root`] decides.
/// Files are removed before their parent directories, so the prune can remove
/// every empty directory. The helper keeps the first error and continues, so
/// one bad file cannot abort the purge.
///
/// A removed symlink counts as one file with zero bytes.
///
/// # Errors
///
/// Returns [`CacheError::Io`] when a directory read or file remove fails.
fn remove_tree_contents(
    root: &Path,
    keep: &Path,
    done: &AtomicU64,
) -> Result<(u64, u64), CacheError> {
    let canonical_root = dunce::canonicalize(root).ok();
    let mut files_removed = 0u64;
    let mut bytes_removed = 0u64;
    let mut first_error: Option<CacheError> = None;
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                first_error.get_or_insert(CacheError::Io {
                    path: dir.clone(),
                    source,
                });
                continue;
            }
        };
        for item in entries {
            let item = match item {
                Ok(item) => item,
                Err(source) => {
                    first_error.get_or_insert(CacheError::Io {
                        path: dir.clone(),
                        source,
                    });
                    continue;
                }
            };
            let path = item.path();
            if path == keep {
                continue;
            }
            let file_type = match item.file_type() {
                Ok(file_type) => file_type,
                Err(source) => {
                    first_error.get_or_insert(CacheError::Io {
                        path: path.clone(),
                        source,
                    });
                    continue;
                }
            };
            if file_type.is_dir() {
                dirs.push(path.clone());
                stack.push(path);
                continue;
            }
            // A symlink is unlinked in place. The walk never follows it, so a
            // link cannot escape the tree and a broken link is removed.
            if file_type.is_symlink() {
                // Windows cannot unlink a directory link with `remove_file`,
                // so follow the link and try the matching remove call first.
                // Unix unlinks a directory link with `remove_file`, so the
                // other call is the fallback. A broken link falls back to
                // `remove_dir` after `remove_file`.
                let result = match std::fs::metadata(&path) {
                    Ok(metadata) if metadata.is_dir() => {
                        std::fs::remove_dir(&path).or_else(|_| std::fs::remove_file(&path))
                    }
                    _ => std::fs::remove_file(&path).or_else(|_| std::fs::remove_dir(&path)),
                };
                match result {
                    Ok(()) => files_removed += 1,
                    Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                    Err(source) => {
                        first_error.get_or_insert(CacheError::Io {
                            path: path.clone(),
                            source,
                        });
                    }
                }
                continue;
            }
            // Never delete a path that resolves outside the cache root.
            if !is_within_root(canonical_root.as_deref(), &path) {
                continue;
            }
            let size = std::fs::symlink_metadata(&path).map_or(0, |meta| meta.len());
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    files_removed += 1;
                    bytes_removed = bytes_removed.saturating_add(size);
                    done.fetch_add(size, Ordering::Relaxed);
                }
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    first_error.get_or_insert(CacheError::Io {
                        path: path.clone(),
                        source,
                    });
                }
            }
        }
    }
    // Remove the deepest directories first, so a parent is empty on its turn.
    // A non-empty directory is left in place.
    dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
    for dir in dirs {
        let _ = std::fs::remove_dir(&dir);
    }
    first_error.map_or(Ok((files_removed, bytes_removed)), Err)
}

/// The bytes of the regular files under `root`, links not followed and
/// counted as zero, as the removal counts them. A folder that cannot be read
/// adds nothing.
fn tree_bytes(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in entries.flatten() {
            let Ok(file_type) = item.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(item.path());
            } else if file_type.is_file() {
                total = total.saturating_add(item.metadata().map_or(0, |meta| meta.len()));
            }
        }
    }
    total
}

#[cfg(test)]
mod tests;
