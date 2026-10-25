//! Prepare the folder that holds the instance files.

use std::io;

use super::paths::Location;

/// Make sure the folder exists and only this user can use it.
///
/// `own_uid` is the user id of this process; unix needs it for a private
/// folder. A runtime folder is trusted as it is. A private folder is created
/// with mode 0700 on unix. An existing one is refused unless this user owns
/// it and the mode is 0700, because another user could have planted it.
///
/// # Errors
///
/// Returns an error when the folder cannot be made or cannot be trusted.
pub fn prepare(location: &Location, own_uid: Option<u32>) -> io::Result<()> {
    match location {
        Location::Runtime(_) => Ok(()),
        Location::Private(dir) => private(dir, own_uid),
    }
}

/// The user id of this process. There is no libc here, so it is read from the
/// owner of a file we create in `dir`.
///
/// # Errors
///
/// Returns the error when the file cannot be made.
#[cfg(unix)]
pub fn own_uid(dir: &std::path::Path) -> io::Result<u32> {
    use std::fs::{self, OpenOptions};
    use std::os::unix::fs::MetadataExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let probe = dir.join(format!(".tarragon-uid-{}-{id}", std::process::id()));
    let _ = fs::remove_file(&probe);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)?;
    let uid = file.metadata().map(|meta| meta.uid());
    drop(file);
    let _ = fs::remove_file(&probe);
    uid
}

/// Refuse a folder that is not a plain folder owned by `own_uid` with mode
/// 0700.
#[cfg(unix)]
fn verify(is_dir: bool, uid: u32, mode: u32, own_uid: u32) -> io::Result<()> {
    let deny = |message: &str| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            message.to_owned(),
        ))
    };
    if !is_dir {
        return deny("not a folder");
    }
    if uid != own_uid {
        return deny("owned by another user");
    }
    if mode & 0o777 != 0o700 {
        return deny("the mode is not 0700");
    }
    Ok(())
}

#[cfg(unix)]
fn private(dir: &std::path::Path, own_uid: Option<u32>) -> io::Result<()> {
    use std::fs::{self, DirBuilder};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    match DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    // Do not follow a link that someone put in place of the folder.
    let meta = fs::symlink_metadata(dir)?;
    let own = own_uid.ok_or_else(|| io::Error::other("cannot find the own user id"))?;
    verify(meta.is_dir(), meta.uid(), meta.mode(), own)
}

#[cfg(not(unix))]
fn private(dir: &std::path::Path, _own_uid: Option<u32>) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn creates_the_folder_with_mode_0700_and_accepts_it_again() {
        let base = std::env::temp_dir().join(format!("tarragon-dir-test-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("base");
        let dir: PathBuf = base.join("private");
        let location = Location::Private(dir.clone());
        prepare(&location, Some(own_uid(&base).expect("uid"))).expect("create");
        let mode = std::fs::metadata(&dir).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        prepare(&location, Some(own_uid(&base).expect("uid"))).expect("accept an existing folder");

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let error = prepare(&location, Some(own_uid(&base).expect("uid"))).expect_err("open mode");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn verify_refuses_another_owner_a_wrong_mode_and_a_non_folder() {
        verify(true, 1000, 0o40_700, 1000).expect("good folder");
        assert!(verify(true, 0, 0o40_700, 1000).is_err());
        assert!(verify(true, 1000, 0o40_755, 1000).is_err());
        assert!(verify(false, 1000, 0o100_700, 1000).is_err());
    }

    #[test]
    fn refuses_a_link_in_place_of_the_folder() {
        let base = std::env::temp_dir().join(format!("tarragon-dir-link-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("base");
        let target = base.join("target");
        std::fs::create_dir(&target).expect("target");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).expect("chmod");
        let link = base.join("private");
        std::os::unix::fs::symlink(&target, &link).expect("link");
        let error = prepare(&Location::Private(link), Some(own_uid(&base).expect("uid")))
            .expect_err("link");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn refuses_a_file_in_place_of_the_folder() {
        let base = std::env::temp_dir().join(format!("tarragon-dir-file-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("base");
        let path = base.join("private");
        std::fs::write(&path, b"x").expect("file");
        assert!(prepare(&Location::Private(path), Some(own_uid(&base).expect("uid"))).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn refuses_a_private_folder_when_the_own_uid_is_unknown() {
        let base = std::env::temp_dir().join(format!("tarragon-dir-nouid-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("base");
        let result = prepare(&Location::Private(base.join("private")), None);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
