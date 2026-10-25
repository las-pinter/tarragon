//! The file system side of data directory resolution.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tarragon_core::paths::{DataPaths, PORTABLE_DIRNAME, platform_data_dir};

/// Why the data directory could not be set up.
#[derive(Debug, thiserror::Error)]
pub enum DataDirError {
    /// The binary path cannot be found.
    #[error("cannot find the binary path: {0}")]
    CurrentExe(#[source] std::io::Error),
    /// The binary path has no parent folder.
    #[error("the binary path has no parent folder: {0}")]
    NoExeDir(PathBuf),
    /// No data root can be chosen.
    #[error(transparent)]
    Resolve(#[from] tarragon_core::error::CoreError),
    /// A folder cannot be created.
    #[error("cannot create {path}: {source}")]
    Create {
        /// The folder path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
}

/// Pick the database path.
///
/// A non-empty `override_db` wins and touches no folder. Otherwise the data
/// root and its cache folder are created and the root's database path is
/// returned.
///
/// # Errors
///
/// Returns [`DataDirError`] when the root cannot be found or created.
pub fn db_path(override_db: Option<OsString>) -> Result<PathBuf, DataDirError> {
    if let Some(path) = override_path(override_db) {
        return Ok(path);
    }
    let paths = resolve()?;
    ensure_dirs(&paths)?;
    Ok(paths.db())
}

/// The database override from the environment. An empty value counts as unset.
fn override_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The binary path with links resolved on unix, so the folder beside the real
/// binary counts. A failed lookup keeps the plain path. Windows keeps the
/// plain path to avoid verbatim `\\?\` prefixes.
fn real_exe_path(exe: PathBuf) -> PathBuf {
    if cfg!(unix) {
        exe.canonicalize().unwrap_or(exe)
    } else {
        exe
    }
}

/// Whether `<exe_dir>/data` is a directory. A plain file does not count.
fn portable_dir_exists(exe_dir: &Path) -> bool {
    exe_dir.join(PORTABLE_DIRNAME).is_dir()
}

/// Resolve the data paths from the real file system.
fn resolve() -> Result<DataPaths, DataDirError> {
    let exe = real_exe_path(std::env::current_exe().map_err(DataDirError::CurrentExe)?);
    let exe_dir = exe
        .parent()
        .ok_or_else(|| DataDirError::NoExeDir(exe.clone()))?;
    let portable = portable_dir_exists(exe_dir);
    let platform = platform_data_dir(
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
        std::env::var_os("APPDATA"),
    );
    Ok(DataPaths::resolve(exe_dir, portable, platform)?)
}

/// Create the cache folder. Its parent is the data root, so the root exists
/// after this call.
fn ensure_dirs(paths: &DataPaths) -> Result<(), DataDirError> {
    create(&paths.cache())
}

/// Create one folder and its parents.
fn create(path: &Path) -> Result<(), DataDirError> {
    std::fs::create_dir_all(path).map_err(|source| DataDirError::Create {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use tarragon_core::paths::DataPaths;

    use super::{create, db_path, ensure_dirs, override_path, portable_dir_exists};

    /// A temporary folder that removes itself on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("tarragon-app-test-{}-{id}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("create the temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn override_wins_for_the_db_path() {
        let path = db_path(Some(OsString::from("/x/custom.db"))).expect("override");
        assert_eq!(path, PathBuf::from("/x/custom.db"));
    }

    #[test]
    fn empty_override_counts_as_unset() {
        assert_eq!(override_path(Some(OsString::new())), None);
        assert_eq!(override_path(None), None);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_override_is_kept_as_is() {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec(vec![b'/', 0xFF, b'.', b'd', b'b']);
        let path = override_path(Some(raw.clone())).expect("override");
        assert_eq!(path.into_os_string(), raw);
    }

    #[test]
    fn data_directory_makes_the_folder_portable() {
        let temp = TempDir::new();
        std::fs::create_dir(temp.0.join("data")).expect("mkdir");
        assert!(portable_dir_exists(&temp.0));
    }

    #[test]
    fn data_file_does_not_make_the_folder_portable() {
        let temp = TempDir::new();
        std::fs::write(temp.0.join("data"), b"x").expect("write");
        assert!(!portable_dir_exists(&temp.0));
    }

    #[test]
    fn missing_data_does_not_make_the_folder_portable() {
        let temp = TempDir::new();
        assert!(!portable_dir_exists(&temp.0));
    }

    #[test]
    fn ensure_dirs_creates_the_root_and_the_cache() {
        let temp = TempDir::new();
        let paths = DataPaths::new(temp.0.join("root"));
        ensure_dirs(&paths).expect("ensure");
        assert!(paths.root().is_dir());
        assert!(paths.cache().is_dir());
    }

    #[test]
    fn create_makes_nested_folders() {
        let temp = TempDir::new();
        let nested = temp.0.join("a").join("cache");
        create(&nested).expect("create");
        assert!(nested.is_dir());
    }

    #[test]
    fn create_reports_the_failing_path() {
        let temp = TempDir::new();
        let file = temp.0.join("file");
        std::fs::write(&file, b"x").expect("write");
        let error = create(&file.join("sub")).expect_err("must fail");
        assert!(error.to_string().contains("file"));
    }
}
