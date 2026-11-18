//! Data directory resolution.
//!
//! The resolver is pure. The caller reads the file system and passes the
//! facts in.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::error::CoreError;

/// The folder name beside the binary that turns on portable mode.
pub const PORTABLE_DIRNAME: &str = "data";

/// The database file name inside the data root.
pub const DB_FILENAME: &str = "tarragon.db";

/// The cache folder name inside the data root.
pub const CACHE_DIRNAME: &str = "cache";

/// The log file name inside the data root.
pub const LOG_FILENAME: &str = "tarragon.log";

/// The folder name inside the platform data directory.
const APP_DIRNAME: &str = "tarragon";

/// Keep a variable only when it is set and not empty.
fn non_empty(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The data directory on Linux and other unix systems.
///
/// `xdg_data_home` is used when it is set, not empty, and absolute. A relative
/// value would depend on the working directory, so it is ignored. Otherwise
/// `home/.local/share` is used.
#[must_use]
pub fn unix_data_dir(xdg_data_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = non_empty(xdg_data_home)
        .filter(|path| path.is_absolute())
        .or_else(|| non_empty(home).map(|h| h.join(".local").join("share")))?;
    Some(base.join(APP_DIRNAME))
}

/// The data directory on macOS.
#[must_use]
pub fn macos_data_dir(home: Option<OsString>) -> Option<PathBuf> {
    let home = non_empty(home)?;
    Some(
        home.join("Library")
            .join("Application Support")
            .join(APP_DIRNAME),
    )
}

/// The data directory on Windows: the Roaming `AppData` folder.
#[must_use]
pub fn windows_data_dir(appdata: Option<OsString>) -> Option<PathBuf> {
    Some(non_empty(appdata)?.join(APP_DIRNAME))
}

/// The platform data directory for the current system.
///
/// The caller passes the values of `XDG_DATA_HOME`, `HOME`, and `APPDATA`.
/// A missing or empty value gives `None`.
#[must_use]
pub fn platform_data_dir(
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
    appdata: Option<OsString>,
) -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        windows_data_dir(appdata)
    } else if cfg!(target_os = "macos") {
        macos_data_dir(home)
    } else {
        unix_data_dir(xdg_data_home, home)
    }
}

/// The resolved locations of the application data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    root: PathBuf,
}

impl DataPaths {
    /// Build the paths under `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Choose the data root.
    ///
    /// A `data` directory beside the binary wins. Otherwise the platform data
    /// directory is used. `portable_dir_exists` says whether
    /// `<exe_dir>/data` is a directory. A plain file does not count.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NoDataDir`] when the folder is not portable and
    /// the platform gives no data directory.
    pub fn resolve(
        exe_dir: &Path,
        portable_dir_exists: bool,
        platform_dir: Option<PathBuf>,
    ) -> Result<Self, CoreError> {
        if portable_dir_exists {
            return Ok(Self::new(exe_dir.join(PORTABLE_DIRNAME)));
        }
        platform_dir.map(Self::new).ok_or(CoreError::NoDataDir)
    }

    /// The data root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The database file path.
    #[must_use]
    pub fn db(&self) -> PathBuf {
        self.root.join(DB_FILENAME)
    }

    /// The cache directory path.
    #[must_use]
    pub fn cache(&self) -> PathBuf {
        self.root.join(CACHE_DIRNAME)
    }

    /// The log file path.
    #[must_use]
    pub fn log(&self) -> PathBuf {
        self.root.join(LOG_FILENAME)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::{DataPaths, macos_data_dir, unix_data_dir, windows_data_dir};
    use crate::error::CoreError;

    fn os(value: &str) -> Option<OsString> {
        Option::from(OsString::from(value))
    }

    #[test]
    fn unix_uses_an_absolute_xdg_data_home() {
        let xdg = std::env::temp_dir().join("xdg");
        let dir = unix_data_dir(Some(xdg.clone().into_os_string()), os("/home/u"));
        assert_eq!(dir, Some(xdg.join("tarragon")));
    }

    #[test]
    fn unix_ignores_a_relative_xdg_data_home() {
        let dir = unix_data_dir(os("rel/dir"), os("/home/u"));
        assert_eq!(dir, Some(PathBuf::from("/home/u/.local/share/tarragon")));
    }

    #[test]
    fn unix_ignores_an_empty_xdg_data_home() {
        let dir = unix_data_dir(os(""), os("/home/u"));
        assert_eq!(dir, Some(PathBuf::from("/home/u/.local/share/tarragon")));
    }

    #[test]
    fn unix_falls_back_to_home_when_xdg_is_unset() {
        let dir = unix_data_dir(None, os("/home/u"));
        assert_eq!(dir, Some(PathBuf::from("/home/u/.local/share/tarragon")));
    }

    #[test]
    fn unix_gives_none_without_home_or_xdg() {
        assert_eq!(unix_data_dir(None, None), None);
        assert_eq!(unix_data_dir(None, os("")), None);
        assert_eq!(unix_data_dir(os("rel"), None), None);
    }

    #[test]
    fn macos_joins_application_support_under_home() {
        let dir = macos_data_dir(os("/Users/u"));
        assert_eq!(
            dir,
            Some(PathBuf::from(
                "/Users/u/Library/Application Support/tarragon"
            ))
        );
    }

    #[test]
    fn macos_gives_none_without_home() {
        assert_eq!(macos_data_dir(None), None);
        assert_eq!(macos_data_dir(os("")), None);
    }

    #[test]
    fn windows_joins_the_app_folder_under_appdata() {
        let dir = windows_data_dir(os("/AppData/Roaming"));
        assert_eq!(dir, Some(PathBuf::from("/AppData/Roaming/tarragon")));
    }

    #[test]
    fn windows_gives_none_without_appdata() {
        assert_eq!(windows_data_dir(None), None);
        assert_eq!(windows_data_dir(os("")), None);
    }

    #[test]
    fn portable_dir_is_used_when_data_is_a_directory() {
        let paths = DataPaths::resolve(
            Path::new("/opt/tarragon"),
            true,
            Some(PathBuf::from("/home/u/.local/share/tarragon")),
        )
        .expect("resolves");
        assert_eq!(paths.root(), Path::new("/opt/tarragon/data"));
    }

    #[test]
    fn portable_dir_is_used_even_without_a_platform_dir() {
        let paths = DataPaths::resolve(Path::new("/opt/tarragon"), true, None).expect("resolves");
        assert_eq!(paths.root(), Path::new("/opt/tarragon/data"));
    }

    #[test]
    fn platform_dir_is_used_when_data_is_absent() {
        let paths = DataPaths::resolve(
            Path::new("/opt/tarragon"),
            false,
            Some(PathBuf::from("/home/u/.local/share/tarragon")),
        )
        .expect("resolves");
        assert_eq!(paths.root(), Path::new("/home/u/.local/share/tarragon"));
    }

    #[test]
    fn resolve_fails_without_a_platform_dir_and_without_data() {
        let error =
            DataPaths::resolve(Path::new("/opt/tarragon"), false, None).expect_err("must fail");
        assert!(matches!(error, CoreError::NoDataDir));
    }

    #[test]
    fn db_cache_and_log_paths_follow_the_root() {
        let paths = DataPaths::new("/r");
        assert_eq!(paths.db(), Path::new("/r/tarragon.db"));
        assert_eq!(paths.cache(), Path::new("/r/cache"));
        assert_eq!(paths.log(), Path::new("/r/tarragon.log"));
    }
}
