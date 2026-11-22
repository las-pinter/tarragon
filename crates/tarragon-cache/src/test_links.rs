//! A directory link helper for the Windows tests.

use std::os::windows::fs::symlink_dir;
use std::path::Path;

/// Make a directory link, or return false when this account may not.
///
/// Windows needs administrator rights or Developer Mode for a link
/// (OS error 1314). A developer machine without them skips the test; on CI
/// (`CI` is set) the test fails instead, so it cannot pass without running.
pub(crate) fn link_dir(target: &Path, link: &Path) -> bool {
    match symlink_dir(target, link) {
        Ok(()) => true,
        Err(error) if error.raw_os_error() == Some(1314) && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: this account may not make a directory link");
            false
        }
        Err(error) => panic!("make the directory link: {error}"),
    }
}
