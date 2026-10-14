//! Folder walk and file metadata collection.
//!
//! The walk is non-recursive (Q26). Only direct children are read. The `stat`
//! calls run on a `rayon` pool (Q19). The pool default is `min(4, cores)`,
//! which is the Q10 four-core pin.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use tarragon_core::entity::FileInfo;
use tarragon_core::error::ScanError;
use tarragon_core::sorting::SortMode;

use crate::extensions::is_supported;

/// The maximum number of scan threads. The value is the Q10 four-core pin.
pub const MAX_SCAN_THREADS: usize = 4;

/// Return the scan pool size: `min(4, available cores)` (Q10, Q19).
///
/// `std::thread::available_parallelism` returns LOGICAL CPUs. The value is an
/// approximation: a cgroup or an affinity mask can change it. The Q10
/// reference pins four PHYSICAL cores, so this size is a cap, not a reading.
///
/// When the core count is unknown, the size is one.
#[must_use]
pub fn worker_pool_size() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(MAX_SCAN_THREADS)
}

/// Build a `rayon` pool for scanning, capped at [`worker_pool_size`] threads.
///
/// Callers install the pool around a scan, so the [`scan_folder`] `stat` calls
/// run on the capped pool. The app owns one pool for its lifetime (Q19).
///
/// # Errors
///
/// Returns [`rayon::ThreadPoolBuildError`] when rayon cannot start a thread.
pub fn build_pool() -> Result<rayon::ThreadPool, rayon::ThreadPoolBuildError> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(worker_pool_size())
        .build()
}

/// Scan one folder. The scan is non-recursive (Q26).
///
/// The function reads the direct children and keeps only the ten extensions
/// from Q26, matched case-insensitively. Each kept file is stat'ed for its
/// size and mtime. Pixel dimensions are not read, so `width` and `height` are
/// zero. The result is sorted in natural full-path order (Q48).
///
/// A missing folder, or a path that is not a folder, gives an empty list, as
/// the Python scanner does. Any other read fault, such as a permission fault,
/// gives [`ScanError::ReadFolder`]. A file that cannot stat is skipped. A file
/// that stats but has an unreadable mtime is kept with an mtime of zero.
///
/// The `stat` calls run on the current `rayon` pool. Install a capped pool
/// with [`build_pool`] for the Q10 four-core pin.
///
/// # Errors
///
/// Returns [`ScanError::ReadFolder`] when the folder exists but cannot read.
pub fn scan_folder(folder: &Path) -> Result<Vec<FileInfo>, ScanError> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(source) => {
            // A missing or non-directory path is not an error, as in Python.
            return match source.kind() {
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => Ok(Vec::new()),
                _ => Err(ScanError::ReadFolder {
                    path: folder.to_path_buf(),
                    source,
                }),
            };
        }
    };

    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if is_supported(&path) {
            candidates.push(path);
        }
    }

    let mut files: Vec<FileInfo> = candidates
        .par_iter()
        .filter_map(|path| stat_file(path))
        .collect();
    files.sort_by(|left, right| SortMode::Natural.compare(left, right));
    Ok(files)
}

/// Read the size and mtime of one candidate path.
///
/// The function returns `None` for a directory, a broken symlink, or a failed
/// stat. The scan skips such a path.
fn stat_file(path: &Path) -> Option<FileInfo> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(FileInfo {
        path: path.to_path_buf(),
        size: metadata.len(),
        mtime: metadata.modified().map_or(0, modified_secs),
        width: 0,
        height: 0,
    })
}

/// Return the epoch seconds of a time. A time before the epoch gives zero.
fn modified_secs(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use tarragon_core::error::ScanError;
    use tarragon_db::Database;

    use super::scan_folder;
    use crate::extensions::{SUPPORTED_EXTENSIONS, is_supported};

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
                .join(format!("tarragon-scan-{tag}-{}-{id}", std::process::id()));
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

    /// Write a small file at `folder/name`.
    fn touch(folder: &Path, name: &str) {
        std::fs::write(folder.join(name), b"x").expect("write the file");
    }

    #[test]
    fn every_supported_extension_passes_in_both_cases() {
        for extension in SUPPORTED_EXTENSIONS {
            let lower = format!("image{extension}");
            let upper = format!("image{}", extension.to_uppercase());
            assert!(
                is_supported(Path::new(&lower)),
                "lower case is refused: {lower}"
            );
            assert!(
                is_supported(Path::new(&upper)),
                "upper case is refused: {upper}"
            );
        }
    }

    #[test]
    fn other_extensions_and_bare_names_are_refused() {
        for name in [
            "notes.txt",
            "readme.md",
            "doc.pdf",
            "script.py",
            "archive.zip",
            "jpg",
            "",
            ".jpg",
        ] {
            assert!(
                !is_supported(Path::new(name)),
                "an unsupported name passed: {name}"
            );
        }
    }

    #[test]
    fn scan_keeps_only_the_ten_extensions_case_insensitively() {
        let temp = TempDir::new("filter");
        for name in [
            "a.jpg", "b.jpeg", "c.png", "d.webp", "e.tiff", "f.tif", "g.psd", "h.psb", "i.clip",
            "j.kra",
        ] {
            touch(temp.path(), name);
        }
        touch(temp.path(), "photo.JPG");
        touch(temp.path(), "image.PnG");
        touch(temp.path(), "notes.txt");
        touch(temp.path(), "readme.md");
        touch(temp.path(), "doc.TxT");

        let files = scan_folder(temp.path()).expect("scan the folder");
        assert_eq!(files.len(), 12);
        for info in &files {
            assert!(
                is_supported(&info.path),
                "an unsupported file passed: {:?}",
                info.path
            );
        }
    }

    #[test]
    fn scan_is_non_recursive() {
        let temp = TempDir::new("shallow");
        touch(temp.path(), "root.jpg");
        let nested = temp.path().join("sub");
        std::fs::create_dir_all(&nested).expect("make the subfolder");
        touch(&nested, "nested.png");
        let deep = nested.join("deeper");
        std::fs::create_dir_all(&deep).expect("make the deep folder");
        touch(&deep, "deep.webp");

        let files = scan_folder(temp.path()).expect("scan the folder");
        let names: Vec<String> = files
            .iter()
            .filter_map(|info| info.name().map(str::to_owned))
            .collect();
        assert_eq!(names, vec!["root.jpg"]);
    }

    #[test]
    fn a_directory_with_an_image_name_is_skipped() {
        let temp = TempDir::new("dirdot");
        std::fs::create_dir_all(temp.path().join("folder.png")).expect("make the folder");
        touch(temp.path(), "real.jpg");

        let files = scan_folder(temp.path()).expect("scan the folder");
        let names: Vec<String> = files
            .iter()
            .filter_map(|info| info.name().map(str::to_owned))
            .collect();
        assert_eq!(names, vec!["real.jpg"]);
    }

    #[test]
    fn missing_folder_scans_empty() {
        let temp = TempDir::new("missing");
        let files = scan_folder(&temp.path().join("gone")).expect("scan a missing folder");
        assert_eq!(files.len(), 0, "a missing folder gives no files");
    }

    #[test]
    fn hidden_files_are_kept() {
        let temp = TempDir::new("hidden");
        touch(temp.path(), ".hidden.png");
        touch(temp.path(), "visible.jpg");

        let files = scan_folder(temp.path()).expect("scan the folder");
        assert_eq!(files.len(), 2, "hidden files are not excluded");
    }

    #[test]
    fn scan_reads_size_and_mtime() {
        let temp = TempDir::new("stat");
        std::fs::write(temp.path().join("artwork.png"), b"some-image-bytes").expect("write");
        let files = scan_folder(temp.path()).expect("scan the folder");
        assert_eq!(files.len(), 1);
        let info = &files[0];
        assert_eq!(info.size, 16);
        assert!(info.mtime > 0, "mtime must be a real time");
        assert_eq!(info.width, 0, "the scan does not read dimensions");
        assert_eq!(info.height, 0, "the scan does not read dimensions");
    }

    #[test]
    fn scan_sorts_in_natural_path_order() {
        let temp = TempDir::new("sort");
        for name in ["name 10.png", "name 2.png", "name 1.png"] {
            touch(temp.path(), name);
        }
        let files = scan_folder(temp.path()).expect("scan the folder");
        let names: Vec<String> = files
            .iter()
            .filter_map(|info| info.name().map(str::to_owned))
            .collect();
        assert_eq!(names, vec!["name 1.png", "name 2.png", "name 10.png"]);
    }

    /// Return true when the process runs as root. Root bypasses file modes.
    #[cfg(unix)]
    fn is_privileged() -> bool {
        std::fs::read_to_string("/proc/self/status").is_ok_and(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("Uid:"))
                .is_some_and(|uids| uids.split_whitespace().next() == Some("0"))
        })
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_without_read_permission_is_a_read_error() {
        use std::os::unix::fs::PermissionsExt;

        if is_privileged() {
            return; // root reads a locked folder, so the check does not apply
        }
        let temp = TempDir::new("denied");
        let locked = temp.path().join("locked");
        std::fs::create_dir(&locked).expect("make the locked folder");
        touch(&locked, "art.png");

        let mut mode = std::fs::metadata(&locked)
            .expect("stat the locked folder")
            .permissions();
        mode.set_mode(0o000);
        std::fs::set_permissions(&locked, mode).expect("lock the folder");

        let result = scan_folder(&locked);

        // Restore the mode so the temp folder can be removed.
        let mut open = std::fs::metadata(&locked)
            .expect("stat the locked folder")
            .permissions();
        open.set_mode(0o755);
        std::fs::set_permissions(&locked, open).expect("unlock the folder");

        assert!(
            matches!(result, Err(ScanError::ReadFolder { .. })),
            "a permission fault must not look like an empty folder: {result:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_name_is_handled() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let temp = TempDir::new("nonutf8");
        // A valid extension on a non-UTF-8 stem is kept.
        let kept = temp.path().join(OsStr::from_bytes(b"\xff\xfe.png"));
        std::fs::write(&kept, b"x").expect("write the kept file");
        // A non-UTF-8 extension is not one of the ten, so the file is skipped.
        let skipped = temp.path().join(OsStr::from_bytes(b"art.\xff\xfe"));
        std::fs::write(&skipped, b"x").expect("write the skipped file");

        let files = scan_folder(temp.path()).expect("scan the folder");
        assert_eq!(files.len(), 1, "only the supported extension is kept");
        assert_eq!(files[0].path, kept, "the file with the supported extension");
    }

    #[test]
    fn ten_thousand_files_write_ten_thousand_rows() {
        let temp = TempDir::new("bucket");
        for index in 0..10_000 {
            touch(temp.path(), &format!("img{index:05}.png"));
        }
        let db_dir = TempDir::new("bucket-db");
        let db = Database::open(db_dir.path().join("tarragon.db")).expect("open the database");
        db.initialize().expect("create the schema");

        let files = scan_folder(temp.path()).expect("scan the folder");
        let written = db.write_scan_group(&files).expect("write the group");
        assert_eq!(files.len(), 10_000, "the scan must keep every file");
        assert_eq!(written, 10_000, "the batched write must persist every row");
        assert_eq!(db.count_thumbnails().expect("count"), 10_000);
    }
}
