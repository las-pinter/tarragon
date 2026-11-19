//! Cleanup of temp files that a crashed write left behind.
//!
//! A write goes through a `.tmp-` file and a rename. A crash between the two
//! leaves the temp file. The size total counts it, but eviction only deletes
//! listed entries, so orphans would push the total over the cap and evict
//! good entries. The caller sweeps them once at start.

use std::path::Path;
use std::time::{Duration, SystemTime};

use tarragon_core::tier::Tier;

use crate::CacheError;
use crate::store::{CacheStore, TEMP_PREFIX};

/// A temp file younger than this may belong to a write in progress.
const ORPHAN_AGE: Duration = Duration::from_secs(60);

impl CacheStore {
    /// Delete temp files older than one minute from the tier folders.
    ///
    /// Call it once at start, before any render writes. It returns the number
    /// of files removed. A missing tier folder counts zero. Symlinks are not
    /// followed.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Io`] when a directory read fails.
    pub fn sweep_orphan_temps(&self) -> Result<u64, CacheError> {
        self.sweep_older_than(ORPHAN_AGE)
    }

    /// Delete temp files whose age is at least `age`.
    fn sweep_older_than(&self, age: Duration) -> Result<u64, CacheError> {
        let mut removed = 0;
        for tier in Tier::ALL {
            let tier_dir = self.root().join(tier.as_str());
            for folder in read_dir_or_empty(&tier_dir)? {
                let is_dir = folder.file_type().is_ok_and(|kind| kind.is_dir());
                if is_dir {
                    removed += sweep_folder(&folder.path(), age)?;
                }
            }
        }
        Ok(removed)
    }
}

/// Remove the old temp files directly inside `folder`.
fn sweep_folder(folder: &Path, age: Duration) -> Result<u64, CacheError> {
    let mut removed = 0;
    for item in read_dir_or_empty(folder)? {
        let is_temp = is_temp_name(&item.file_name());
        let is_file = item.file_type().is_ok_and(|kind| kind.is_file());
        if !is_temp || !is_file {
            continue;
        }
        let old_enough = item
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|elapsed| elapsed >= age);
        if old_enough && std::fs::remove_file(item.path()).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Whether `name` is a temp name of a write: the prefix, the process id, a
/// dash, and a sequence number. A cache file always ends in a format
/// extension, so one made from a source called `.tmp-x.png` does not match.
fn is_temp_name(name: &std::ffi::OsStr) -> bool {
    let Some(rest) = name
        .to_str()
        .and_then(|text| text.strip_prefix(TEMP_PREFIX))
    else {
        return false;
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    rest.split_once('-')
        .is_some_and(|(pid, seq)| digits(pid) && digits(seq))
}

/// List a directory. A missing directory gives an empty list.
fn read_dir_or_empty(dir: &Path) -> Result<Vec<std::fs::DirEntry>, CacheError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(CacheError::Io {
                path: dir.to_path_buf(),
                source,
            });
        }
    };
    Ok(entries.flatten().collect())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::store::CacheStore;

    /// Make a unique empty cache root.
    fn root(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tarragon-temp-{tag}-{}",
            tarragon_core::testing::run_tag()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("make the root");
        path
    }

    #[test]
    fn the_sweep_removes_old_temps_and_keeps_real_files() {
        let root = root("sweep");
        let folder = root.join("320").join("gallery_abcd");
        std::fs::create_dir_all(&folder).expect("make the folder");
        let orphan = folder.join(".tmp-1-0");
        let real = folder.join("art.jpg.jpg");
        // The cache file of a source whose name starts like a temp.
        let lookalike = folder.join(".tmp-1-0.png.jpg");
        for file in [&orphan, &real, &lookalike] {
            std::fs::write(file, b"x").expect("write the file");
        }
        let store = CacheStore::new(&root);

        // A fresh temp may belong to a live write, so the real sweep keeps it.
        assert_eq!(store.sweep_orphan_temps().expect("sweep"), 0);
        assert!(orphan.is_file());

        let removed = store.sweep_older_than(Duration::ZERO).expect("sweep");

        assert_eq!(removed, 1);
        assert!(!orphan.exists());
        assert!(real.is_file());
        assert!(lookalike.is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_exact_temp_pattern_is_a_temp_name() {
        for (name, temp) in [
            (".tmp-12-3", true),
            (".tmp-12-3.jpg", false),
            (".tmp-x.png.jpg", false),
            (".tmp-12-", false),
            (".tmp--3", false),
            ("tmp-12-3", false),
        ] {
            assert_eq!(super::is_temp_name(name.as_ref()), temp, "{name}");
        }
    }

    #[test]
    fn the_sweep_of_a_missing_root_removes_nothing() {
        let store = CacheStore::new(std::env::temp_dir().join("tarragon-temp-missing-root"));
        assert_eq!(store.sweep_orphan_temps().expect("sweep"), 0);
    }
}
