//! Cache purge and the `clear_full_res_on_exit` rule.
//!
//! The manual purge deletes the three tier folders under the cache root and
//! nothing else. The caller
//! cancels the pending renders and drains the worker pool first, so no render
//! writes while the purge deletes. The `DELETE FROM thumbnails` clear is a
//! separate call, because the cache crate cannot reach the database.
//!
//! The shutdown rule clears only the `full` tier when the setting is true. The
//! `256` and `1024` tiers always stay. A false setting deletes nothing.
//!
//! Every delete reuses the [`is_within_root`] guard, so a path that resolves
//! outside the cache root is never removed. The walk never follows a symlink.
//! A symlink inside the tree is unlinked in place, and a symlinked tier
//! directory is refused.
//!
//! The caller passes the application data directory as `protected`. The purge
//! refuses a cache root that is the data directory or an ancestor of it, so
//! the purge cannot delete `tarragon.db`. The cache crate cannot resolve
//! the data path itself, so the caller supplies it.

use std::path::{Path, PathBuf};

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

impl CacheStore {
    /// Delete the cache tiers (`256`, `1024`, `full`) and their markers, and
    /// prune the empty directories. The cache root itself stays.
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
        let Some(resolved) = self.resolve_root()? else {
            return Ok(PurgeOutcome::default());
        };
        if !is_safe_purge_root(&resolved, protected) {
            return Err(CacheError::UnsafeRoot(resolved));
        }
        let mut outcome = PurgeOutcome::default();
        for tier in Tier::ALL {
            let tier_outcome = self.clear_tier_in(&resolved, tier)?;
            outcome.files_removed += tier_outcome.files_removed;
            outcome.bytes_removed += tier_outcome.bytes_removed;
            // The tier folder is cache-owned, so drop it once it is empty.
            let _ = std::fs::remove_dir(resolved.join(tier.as_str()));
        }
        Ok(outcome)
    }

    /// Delete every file under one tier and prune its empty directories. The
    /// tier root itself stays.
    ///
    /// A missing tier gives a zero outcome. A tier directory that resolves
    /// outside the cache root deletes nothing. A tier directory that is a
    /// symlink is refused, so `full -> 256` cannot empty the `256` tier. The
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
        self.clear_tier_in(&resolved_root, tier)
    }

    /// Clear the `full` tier when `enabled` is true. A false value deletes
    /// nothing.
    ///
    /// This is the shutdown rule. The `256` and `1024` tiers always stay.
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

    /// Clear one tier folder below the already resolved cache root.
    fn clear_tier_in(&self, resolved_root: &Path, tier: Tier) -> Result<PurgeOutcome, CacheError> {
        let tier_dir = self.root().join(tier.as_str());
        // A symlinked tier directory can point at another tier. Refuse it, so
        // the clear cannot empty the wrong directory.
        match std::fs::symlink_metadata(&tier_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Ok(PurgeOutcome::default());
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PurgeOutcome::default());
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
        match std::fs::canonicalize(&tier_dir) {
            Ok(canonical) if canonical == expected => {}
            Ok(_) => return Ok(PurgeOutcome::default()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PurgeOutcome::default());
            }
            Err(source) => {
                return Err(CacheError::Io {
                    path: tier_dir,
                    source,
                });
            }
        }
        let (files_removed, bytes_removed) = remove_tree_contents(&tier_dir)?;
        Ok(PurgeOutcome {
            files_removed,
            bytes_removed,
        })
    }

    /// Resolve the cache root to an absolute path. A missing root gives
    /// `None`.
    fn resolve_root(&self) -> Result<Option<PathBuf>, CacheError> {
        match std::fs::canonicalize(self.root()) {
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
    if let Some(home) = home.and_then(|home| std::fs::canonicalize(home).ok()) {
        // home.starts_with(resolved) is true when resolved is an ancestor.
        if home.starts_with(resolved) {
            return false;
        }
    }
    if let Some(protected) = protected {
        let protected =
            std::fs::canonicalize(protected).unwrap_or_else(|_| protected.to_path_buf());
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

/// Remove every file under `root` and prune the empty subdirectories. The
/// `root` directory itself stays.
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
fn remove_tree_contents(root: &Path) -> Result<(u64, u64), CacheError> {
    let canonical_root = std::fs::canonicalize(root).ok();
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

#[cfg(test)]
mod tests;
