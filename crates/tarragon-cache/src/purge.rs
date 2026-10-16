//! Cache purge and the `clear_full_res_on_exit` rule (Q25).
//!
//! The manual purge deletes every file under the cache root. The caller
//! cancels the pending renders and drains the worker pool first, so no render
//! writes while the purge deletes (Q25). The `DELETE FROM thumbnails` clear is
//! a separate call, because the cache crate cannot reach the database (Q16).
//!
//! The shutdown rule clears only the `full` tier when the setting is true. The
//! `256` and `1024` tiers always stay (Q25). A false setting deletes nothing.
//!
//! Every delete reuses the [`is_within_root`] guard, so a path that resolves
//! outside the cache root is never removed. The walk never follows a symlink.
//! A symlink inside the tree is unlinked in place, and a symlinked tier
//! directory is refused.
//!
//! The caller passes the application data directory as `protected`. The purge
//! refuses a cache root that is the data directory or an ancestor of it, so
//! the purge cannot delete `tarragon.db` (Q25). The cache crate cannot resolve
//! the data path itself (Q16), so task 34 supplies it.

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
    /// Delete every file under the cache root and prune the empty
    /// directories. The cache root itself stays.
    ///
    /// This is the filesystem half of the Q25 purge. The caller cancels the
    /// pending renders and drains the worker pool first. The
    /// `DELETE FROM thumbnails` clear is a separate call on the database
    /// (Q16).
    ///
    /// A missing cache root gives a zero outcome. An unsafe root, for example
    /// the filesystem root, is refused. A root that is `protected`, or an
    /// ancestor of it, is refused. Task 34 passes the application data
    /// directory as `protected`, so the purge cannot delete `tarragon.db`
    /// (Q25). A `None` value skips the data-dir guard.
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
        let (files_removed, bytes_removed) = remove_tree_contents(&resolved)?;
        Ok(PurgeOutcome {
            files_removed,
            bytes_removed,
        })
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
    /// Task 34 passes the application data directory as `protected` (Q16,
    /// Q25).
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

    /// Clear the `full` tier when `enabled` is true. A false value deletes
    /// nothing.
    ///
    /// This is the Q25 shutdown rule. The `256` and `1024` tiers always stay.
    /// The caller drains the worker pool first, as [`CacheStore::purge`] does.
    /// Task 34 passes the application data directory as `protected`, so the
    /// clear cannot touch `tarragon.db` (Q16, Q25).
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
/// misconfigured cache root cannot delete the wrong tree (Q25).
///
/// Refuses `resolved` when it is `protected` or an ancestor of `protected`.
/// Task 34 passes the application data directory as `protected`, so the purge
/// cannot delete `tarragon.db` (Q16, Q25). A missing `protected` path is
/// compared by its literal components.
fn is_safe_purge_root(resolved: &Path, protected: Option<&Path>) -> bool {
    if resolved.parent().is_none() {
        return false;
    }
    if let Some(home) = home_dir().and_then(|home| std::fs::canonicalize(home).ok()) {
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
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use image::{DynamicImage, Rgba, RgbaImage};
    use tarragon_core::tier::Tier;

    use crate::CacheError;
    use crate::store::CacheStore;

    use super::PurgeOutcome;

    /// A temporary folder that removes itself on drop.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        /// Create a unique temp folder with the given tag.
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("tarragon-purge-{tag}-{}-{id}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create the temp folder");
            Self { path }
        }

        /// Return the folder path.
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// Build a small deterministic source image.
    fn source(width: u32, height: u32) -> DynamicImage {
        let rgba = RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, 255]));
        DynamicImage::ImageRgba8(rgba)
    }

    /// Return the canonical form of `path`, which must exist.
    fn canonical(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).expect("canonicalize the path")
    }

    #[test]
    fn purge_removes_every_tier_file_and_marker() {
        let temp = TempDir::new("all");
        let store = CacheStore::new(temp.path());
        let entry = store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        assert!(store.total_size().expect("total") > 0);

        let outcome = store.purge(None).expect("purge");

        assert_eq!(
            outcome.files_removed, 6,
            "three tier files and three markers must go"
        );
        assert!(outcome.bytes_removed > 0, "the purge must count the bytes");
        assert_eq!(store.total_size().expect("total"), 0, "the tree must empty");
        assert!(temp.path().is_dir(), "the cache root itself must stay");
        for tier in Tier::ALL {
            assert!(
                !entry.path(tier).exists(),
                "tier {} must be gone",
                tier.as_str()
            );
            assert!(
                !store.marker_path(tier).exists(),
                "marker {} must be gone",
                tier.as_str()
            );
        }
    }

    #[test]
    fn purge_of_a_missing_root_removes_nothing() {
        let temp = TempDir::new("missing");
        let store = CacheStore::new(temp.path().join("does-not-exist"));

        let outcome = store.purge(None).expect("purge a missing root");

        assert_eq!(outcome, PurgeOutcome::default());
    }

    #[cfg(unix)]
    #[test]
    fn purge_skips_a_link_that_points_outside_the_root() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new("escape");
        let store = CacheStore::new(temp.path());
        store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        let outside = TempDir::new("outside");
        let victim = outside.path().join("victim.jpg");
        std::fs::write(&victim, b"keep me").expect("write the victim");
        let link = temp.path().join("escape.jpg");
        symlink(&victim, &link).expect("make the escape link");

        store.purge(None).expect("purge");

        assert!(
            victim.is_file(),
            "the target outside the cache root must survive"
        );
        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "the link itself must be unlinked in place"
        );
    }

    #[cfg(unix)]
    #[test]
    fn purge_unlinks_a_broken_symlink() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new("broken-link");
        let store = CacheStore::new(temp.path());
        let link = temp.path().join("broken.jpg");
        symlink(temp.path().join("does-not-exist.jpg"), &link).expect("make the broken link");

        store.purge(None).expect("purge");

        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "a broken link must be removed without an error"
        );
    }

    #[cfg(unix)]
    #[test]
    fn purge_unlinks_a_directory_symlink_to_an_outside_directory() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new("dir-link");
        let store = CacheStore::new(temp.path());
        let outside = TempDir::new("dir-link-outside");
        let victim_dir = outside.path().join("keep");
        std::fs::create_dir_all(&victim_dir).expect("make the outside directory");
        let victim_file = victim_dir.join("keep.jpg");
        std::fs::write(&victim_file, b"keep me").expect("write the outside file");
        let link = temp.path().join("dir-link");
        symlink(&victim_dir, &link).expect("make the directory link");

        let outcome = store.purge(None).expect("purge");

        assert_eq!(
            outcome.files_removed, 1,
            "the directory link must count once"
        );
        assert!(
            victim_file.is_file(),
            "the target outside the cache root must survive"
        );
        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "the directory link itself must be unlinked in place"
        );
    }

    #[test]
    fn is_within_root_accepts_only_a_path_inside_the_root() {
        let temp = TempDir::new("within");
        let inside_file = temp.path().join("256").join("art.jpg");
        std::fs::create_dir_all(inside_file.parent().expect("parent")).expect("make the dir");
        std::fs::write(&inside_file, b"inside").expect("write the inside file");
        let root = canonical(temp.path());
        let outside = TempDir::new("within-outside");
        let outside_file = outside.path().join("victim.jpg");
        std::fs::write(&outside_file, b"outside").expect("write the outside file");

        assert!(
            super::is_within_root(Some(&root), &inside_file),
            "an inside path must pass"
        );
        assert!(
            !super::is_within_root(Some(&root), &outside_file),
            "an outside path must fail"
        );
        assert!(
            !super::is_within_root(Some(&root), &temp.path().join("missing.jpg")),
            "a missing path must fail"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = temp.path().join("escape.jpg");
            symlink(&outside_file, &link).expect("make the escape link");
            assert!(
                !super::is_within_root(Some(&root), &link),
                "a symlink to an outside path must fail"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn clear_tier_refuses_a_symlinked_tier_directory() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new("tier-symlink");
        let store = CacheStore::new(temp.path());
        let entry = store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        let before_256 = std::fs::read(&entry.thumb256).expect("read the 256 tier");
        // Replace the real `full` directory with a link to `256`.
        std::fs::remove_dir_all(temp.path().join("full")).expect("remove the real full dir");
        symlink(temp.path().join("256"), temp.path().join("full")).expect("make the tier link");

        let outcome = store.clear_tier(Tier::Full, None).expect("clear full");

        assert_eq!(
            outcome,
            PurgeOutcome::default(),
            "a symlinked tier must be refused"
        );
        assert_eq!(
            std::fs::read(&entry.thumb256).expect("read the 256 tier again"),
            before_256,
            "the 256 tier must stay byte-identical"
        );
        assert!(entry.thumb256.is_file(), "the 256 tier must survive");
    }

    #[test]
    fn clear_tier_refuses_when_the_root_is_the_protected_path() {
        let temp = TempDir::new("clear-protect");
        let store = CacheStore::new(temp.path());
        store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        let root = canonical(temp.path());

        let error = store
            .clear_tier(Tier::Full, Some(&root))
            .expect_err("refuse the protected root");

        assert!(
            matches!(error, CacheError::UnsafeRoot(_)),
            "the error must name the unsafe root"
        );
        assert!(
            store.total_size().expect("total") > 0,
            "a refused clear must delete nothing"
        );
    }

    #[test]
    fn the_filesystem_root_is_not_a_safe_purge_root() {
        assert!(!super::is_safe_purge_root(Path::new("/"), None));
    }

    #[test]
    fn the_home_directory_is_not_a_safe_purge_root() {
        let Some(home) = super::home_dir() else {
            return;
        };
        let home = canonical(&home);

        assert!(
            !super::is_safe_purge_root(&home, None),
            "the home directory must be refused"
        );
    }

    #[test]
    fn an_ancestor_of_home_is_not_a_safe_purge_root() {
        let Some(home) = super::home_dir() else {
            return;
        };
        let home = canonical(&home);
        let ancestor = home.parent().expect("home must have a parent");

        assert!(
            !super::is_safe_purge_root(ancestor, None),
            "an ancestor of home must be refused"
        );
    }

    #[test]
    fn purge_refuses_when_the_root_is_the_protected_path() {
        let temp = TempDir::new("protect-equal");
        let store = CacheStore::new(temp.path());
        store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        let root = canonical(temp.path());

        let error = store
            .purge(Some(&root))
            .expect_err("refuse the protected root");

        assert!(
            matches!(error, CacheError::UnsafeRoot(_)),
            "the error must name the unsafe root"
        );
        assert!(
            store.total_size().expect("total") > 0,
            "a refused purge must delete nothing"
        );
    }

    #[test]
    fn purge_refuses_when_the_root_is_an_ancestor_of_the_protected_path() {
        let temp = TempDir::new("protect-ancestor");
        let store = CacheStore::new(temp.path());
        store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        // The parent exists because the root was canonicalized. The guard
        // compares the literal path components and does not strip the
        // filename.
        let protected = canonical(temp.path()).join("tarragon.db");

        let error = store
            .purge(Some(&protected))
            .expect_err("refuse the data-dir ancestor");

        assert!(
            matches!(error, CacheError::UnsafeRoot(_)),
            "the error must name the unsafe root"
        );
        assert!(
            store.total_size().expect("total") > 0,
            "a refused purge must delete nothing"
        );
    }

    #[test]
    fn purge_allows_an_unrelated_protected_path() {
        let temp = TempDir::new("protect-unrelated");
        let store = CacheStore::new(temp.path());
        store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        let other = TempDir::new("protect-other");
        let protected = canonical(other.path());

        store
            .purge(Some(&protected))
            .expect("an unrelated protected path must pass");

        assert_eq!(
            store.total_size().expect("total"),
            0,
            "the purge must empty the tree"
        );
    }

    #[test]
    fn clear_full_res_on_exit_true_removes_only_the_full_tier() {
        let temp = TempDir::new("clear-full");
        let store = CacheStore::new(temp.path());
        let entry = store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        let before_256 = std::fs::read(&entry.thumb256).expect("read the 256 tier");
        let before_1024 = std::fs::read(&entry.preview1024).expect("read the 1024 tier");

        let outcome = store
            .clear_full_res_on_exit(true, None)
            .expect("clear full");

        assert!(!entry.full.exists(), "the full tier file must be removed");
        assert!(
            !store.marker_path(Tier::Full).exists(),
            "the full tier marker must be removed"
        );
        for tier in [Tier::Thumb256, Tier::Preview1024] {
            assert!(
                entry.path(tier).is_file(),
                "tier {} must stay",
                tier.as_str()
            );
            assert!(
                store.marker_path(tier).is_file(),
                "marker {} must stay",
                tier.as_str()
            );
        }
        assert_eq!(
            std::fs::read(&entry.thumb256).expect("read the 256 tier again"),
            before_256,
            "the 256 tier must stay byte-identical"
        );
        assert_eq!(
            std::fs::read(&entry.preview1024).expect("read the 1024 tier again"),
            before_1024,
            "the 1024 tier must stay byte-identical"
        );
        assert_eq!(outcome.files_removed, 2, "one full file and one marker");
    }

    #[test]
    fn clear_full_res_on_exit_false_removes_nothing() {
        let temp = TempDir::new("keep-full");
        let store = CacheStore::new(temp.path());
        let entry = store
            .write_all(&source(40, 30), "gallery", "abcd1234", "art.jpg")
            .expect("write the tiers");
        let before = store.total_size().expect("total");

        let outcome = store
            .clear_full_res_on_exit(false, None)
            .expect("clear full");

        assert_eq!(outcome, PurgeOutcome::default());
        assert_eq!(store.total_size().expect("total"), before);
        assert!(entry.full.is_file(), "the full tier file must stay");
    }
}
