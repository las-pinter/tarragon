//! The file system side of data directory resolution.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tarragon_core::paths::{DataPaths, LOG_FILENAME, PORTABLE_DIRNAME, platform_data_dir};
use tarragon_db::settings::SettingValue;

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

/// Where the thumbnail cache lives: the `cache_dir` setting when it holds
/// text, else the cache folder of the data root. The data root is the folder
/// of the database file.
pub fn cache_root(db_path: &Path, setting: &SettingValue) -> PathBuf {
    if let SettingValue::Text(text) = setting
        && !text.is_empty()
    {
        return PathBuf::from(text);
    }
    let root = match db_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    DataPaths::new(root).cache()
}

/// Where the log file and the crash files go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLocation {
    /// The log file path.
    pub log_file: PathBuf,
    /// The folder for crash files.
    pub crash_dir: PathBuf,
}

/// Pick the log and crash locations.
///
/// A non-empty `override_db` puts both beside the database file, and no folder
/// is created. Otherwise the log is the data root's log file and crash files
/// go in the data root. This call creates no folder.
///
/// # Errors
///
/// Returns [`DataDirError`] when the data root cannot be found.
pub fn log_location(override_db: Option<OsString>) -> Result<LogLocation, DataDirError> {
    locate(override_path(override_db), resolve)
}

/// The choice behind [`log_location`], with the root lookup passed in.
fn locate(
    override_db: Option<PathBuf>,
    resolve_root: impl FnOnce() -> Result<DataPaths, DataDirError>,
) -> Result<LogLocation, DataDirError> {
    if let Some(db) = override_db {
        let dir = match db.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        return Ok(LogLocation {
            log_file: dir.join(LOG_FILENAME),
            crash_dir: dir,
        });
    }
    let paths = resolve_root()?;
    Ok(LogLocation {
        log_file: paths.log(),
        crash_dir: paths.root().to_path_buf(),
    })
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

    use tarragon_db::settings::SettingValue;

    use super::{
        DataDirError, LogLocation, cache_root, create, db_path, ensure_dirs, locate, override_path,
        portable_dir_exists,
    };

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
    fn the_cache_dir_setting_wins_over_the_default() {
        let db = PathBuf::from("/data/tarragon.db");
        let set = SettingValue::Text("/elsewhere/cache".to_owned());
        assert_eq!(cache_root(&db, &set), PathBuf::from("/elsewhere/cache"));
        assert_eq!(
            cache_root(&db, &SettingValue::None),
            PathBuf::from("/data/cache")
        );
        assert_eq!(
            cache_root(&db, &SettingValue::Text(String::new())),
            PathBuf::from("/data/cache")
        );
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

    #[test]
    fn override_puts_log_and_crash_files_beside_the_db() {
        let dir = std::env::temp_dir().join("custom");
        let found = locate(Some(dir.join("my.db")), || {
            Err(DataDirError::NoExeDir(PathBuf::new()))
        })
        .expect("locate");
        assert_eq!(
            found,
            LogLocation {
                log_file: dir.join("tarragon.log"),
                crash_dir: dir,
            }
        );
    }

    #[test]
    fn bare_override_file_name_uses_the_current_folder() {
        let found = locate(Some(PathBuf::from("my.db")), || {
            Err(DataDirError::NoExeDir(PathBuf::new()))
        })
        .expect("locate");
        assert_eq!(found.crash_dir, PathBuf::from("."));
        assert_eq!(found.log_file, PathBuf::from(".").join("tarragon.log"));
    }

    #[test]
    fn without_override_the_data_root_holds_both() {
        let root = std::env::temp_dir().join("root");
        let paths = DataPaths::new(root.clone());
        let found = locate(None, || Ok(paths)).expect("locate");
        assert_eq!(found.log_file, root.join("tarragon.log"));
        assert_eq!(found.crash_dir, root);
    }

    #[test]
    fn without_override_a_root_failure_is_returned() {
        let result = locate(None, || Err(DataDirError::NoExeDir(PathBuf::new())));
        assert!(result.is_err());
    }
}
