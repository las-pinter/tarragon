use std::path::PathBuf;
use std::process::Command;

use super::*;

/// A temporary folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-app-remove-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("create the temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The variable that tells the helper test which file to trash.
const HELPER_FILE: &str = "TARRAGON_TEST_TRASH_FILE";

/// Runs only as a child of `trash_moves_the_file_into_the_trash_home`, which
/// sets the trash home for the child alone. Without the variable it fails
/// before it trashes anything.
#[test]
#[ignore = "runs only as a child process of the trash test"]
fn trash_helper_child() {
    let file = PathBuf::from(std::env::var_os(HELPER_FILE).expect("the file to trash"));
    let failures = trash(&[file]);
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn trash_moves_the_file_into_the_trash_home() {
    // The trash crate reads XDG_DATA_HOME from the process environment, and
    // `unsafe_code` is forbidden here, so the variable cannot be set in this
    // process. A child process of this test binary gets its own environment, so
    // no other test sees it and it holds under `cargo test` as well as nextest.
    // The file and the trash home share one temp folder, so they share a mount
    // and the file goes to the home trash. HOME points there too, so no path
    // leads to the real trash.
    let dir = TempDir::new();
    let data = dir.0.join("data");
    let folder = dir.0.join("pictures");
    fs::create_dir_all(&data).expect("create the data home");
    fs::create_dir_all(&folder).expect("create the folder");
    let file = folder.join("a.png");
    fs::write(&file, b"pixels").expect("write");

    let output = Command::new(std::env::current_exe().expect("test binary"))
        .args(["--ignored", "--nocapture", "trash_helper_child"])
        .env("XDG_DATA_HOME", &data)
        .env("HOME", &dir.0)
        .env(HELPER_FILE, &file)
        .output()
        .expect("run the child");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(output.status.success(), "{text}");
    assert!(text.contains("1 passed"), "the helper did not run: {text}");
    assert!(!file.exists());
    let trashed = data.join("Trash/files/a.png");
    assert_eq!(fs::read(&trashed).expect("trashed file"), b"pixels");
    let info = fs::read_to_string(data.join("Trash/info/a.png.trashinfo")).expect("info file");
    assert!(info.contains("Path="), "{info}");
    assert!(info.contains("DeletionDate="), "{info}");
}

#[test]
fn trash_counts_a_missing_file_as_done() {
    let dir = TempDir::new();

    let failures = trash(&[dir.0.join("gone.png")]);

    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn trash_names_a_path_it_cannot_inspect_and_goes_on() {
    let dir = TempDir::new();
    let plain = dir.0.join("plain.txt");
    fs::write(&plain, b"x").expect("write");
    // A path below a file fails with "not a directory", not "not found".
    let bad = plain.join("child.png");
    let gone = dir.0.join("gone.png");

    let failures = trash(&[bad.clone(), gone]);

    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].0, bad);
    assert!(matches!(failures[0].1, FileOpError::Io { .. }));
    assert!(plain.exists());
}

#[test]
fn delete_permanently_removes_the_files() {
    let dir = TempDir::new();
    let first = dir.0.join("a.png");
    let second = dir.0.join("b.png");
    fs::write(&first, b"1").expect("write");
    fs::write(&second, b"2").expect("write");

    let failures = delete_permanently(&[first.clone(), second.clone()]);

    assert!(failures.is_empty(), "{failures:?}");
    assert!(!first.exists());
    assert!(!second.exists());
}

#[test]
fn delete_permanently_counts_a_missing_file_as_done() {
    let dir = TempDir::new();
    let file = dir.0.join("a.png");
    fs::write(&file, b"1").expect("write");

    let failures = delete_permanently(&[dir.0.join("gone.png"), file.clone()]);

    assert!(failures.is_empty(), "{failures:?}");
    assert!(!file.exists());
}

#[cfg(unix)]
#[test]
fn delete_permanently_removes_a_symlink_and_keeps_its_target() {
    let dir = TempDir::new();
    let target = dir.0.join("a.png");
    let link = dir.0.join("link.png");
    fs::write(&target, b"1").expect("write");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    let failures = delete_permanently(std::slice::from_ref(&link));

    assert!(failures.is_empty(), "{failures:?}");
    assert!(link.symlink_metadata().is_err());
    assert_eq!(fs::read(&target).expect("target"), b"1");
}

#[test]
fn delete_permanently_names_a_failure_and_goes_on() {
    let dir = TempDir::new();
    let first = dir.0.join("a.png");
    let second = dir.0.join("b.png");
    let folder = dir.0.join("folder");
    fs::write(&first, b"1").expect("write");
    fs::write(&second, b"2").expect("write");
    fs::create_dir_all(&folder).expect("create the folder");

    let failures = delete_permanently(&[first.clone(), folder.clone(), second.clone()]);

    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].0, folder);
    assert!(matches!(failures[0].1, FileOpError::Io { .. }));
    assert!(!first.exists());
    assert!(!second.exists());
    assert!(folder.is_dir());
}
