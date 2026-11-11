use std::path::PathBuf;

use super::*;

/// A temporary folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-app-copy-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("create the temp dir");
        Self(dir)
    }

    fn sub(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).expect("create the sub dir");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The sorted file names in a folder.
fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .expect("list the folder")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .into_string()
                .expect("utf8")
        })
        .collect();
    names.sort();
    names
}

#[test]
fn duplicate_twice_gives_copy_then_copy_2_and_keeps_the_source() {
    let dir = TempDir::new();
    let source = dir.0.join("a.png");
    fs::write(&source, b"pixels").expect("write");

    let first = copy_into(&source, &dir.0).expect("first copy");
    let second = copy_into(&source, &dir.0).expect("second copy");

    assert_eq!(first.path, dir.0.join("a copy.png"));
    assert_eq!(second.path, dir.0.join("a copy 2.png"));
    assert_eq!(first.len, 6);
    assert!(first.mtime > UNIX_EPOCH);
    assert_eq!(fs::read(&first.path).expect("read"), b"pixels");
    assert_eq!(fs::read(&second.path).expect("read"), b"pixels");
    assert_eq!(fs::read(&source).expect("read"), b"pixels");
    assert_eq!(names(&dir.0), ["a copy 2.png", "a copy.png", "a.png"]);
}

#[test]
fn paste_keeps_the_name_when_free_and_counts_up_when_taken() {
    let dir = TempDir::new();
    let from = dir.sub("from");
    let into = dir.sub("into");
    let source = from.join("a.png");
    fs::write(&source, b"new").expect("write");

    let kept = copy_into(&source, &into).expect("free name");
    assert_eq!(kept.path, into.join("a.png"));

    let taken = copy_into(&source, &into).expect("taken name");
    assert_eq!(taken.path, into.join("a copy.png"));
    assert_eq!(names(&into), ["a copy.png", "a.png"]);
    assert_eq!(names(&from), ["a.png"]);
}

#[test]
fn a_target_made_after_the_name_pick_is_not_overwritten() {
    let dir = TempDir::new();
    let source = dir.0.join("a.png");
    fs::write(&source, b"source").expect("write");
    let mut raced = false;

    let copied = copy_with(&source, &dir.0, |target| {
        if !raced {
            raced = true;
            assert_eq!(target, dir.0.join("a copy.png"));
            fs::write(target, b"other").expect("race file");
        }
    })
    .expect("copy");

    assert_eq!(copied.path, dir.0.join("a copy 2.png"));
    assert_eq!(fs::read(dir.0.join("a copy.png")).expect("read"), b"other");
    assert_eq!(fs::read(&copied.path).expect("read"), b"source");
}

#[test]
fn a_directory_is_refused_and_leaves_no_file() {
    let dir = TempDir::new();
    let into = dir.sub("into");
    let folder_source = dir.sub("pics");

    let error = copy_into(&folder_source, &into).expect_err("directory");

    assert!(matches!(error, FileOpError::NotAFile(_)), "{error}");
    assert_eq!(names(&into), Vec::<String>::new());
}

#[test]
fn a_missing_source_is_refused_and_leaves_no_file() {
    let dir = TempDir::new();
    let into = dir.sub("into");

    let error = copy_into(&dir.0.join("gone.png"), &into).expect_err("missing");

    assert!(matches!(error, FileOpError::Io { .. }), "{error}");
    assert_eq!(names(&into), Vec::<String>::new());
}

#[test]
fn a_copy_name_over_255_bytes_is_refused_and_leaves_no_file() {
    let dir = TempDir::new();
    // 248 + ".png" is 252 bytes, so " copy" would make 257.
    let name = format!("{}.png", "a".repeat(248));
    assert_eq!(name.len(), 252);
    let source = dir.0.join(&name);
    fs::write(&source, b"x").expect("write");

    let error = copy_into(&source, &dir.0).expect_err("too long");

    assert!(
        matches!(error, FileOpError::NameTooLong { len: 257, max: 255 }),
        "{error}"
    );
    assert_eq!(names(&dir.0), [name]);
}

#[test]
fn a_short_copy_is_reported_and_the_partial_file_is_removed() {
    let dir = TempDir::new();
    let source = dir.0.join("a.png");
    fs::write(&source, b"abc").expect("write");
    let target = dir.0.join("out.png");
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .expect("create the target");
    let mut input = File::open(&source).expect("open");

    let error = write_copy(&mut input, output, &target, 4).expect_err("short copy");

    assert!(
        matches!(
            error,
            FileOpError::ShortCopy {
                copied: 3,
                expected: 4,
                ..
            }
        ),
        "{error}"
    );
    assert!(!target.exists());
}

#[cfg(unix)]
#[test]
fn a_name_that_is_not_utf8_is_refused_and_leaves_no_file() {
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new();
    let into = dir.sub("into");
    let source = dir.0.join(std::ffi::OsStr::from_bytes(b"a\xff.png"));
    fs::write(&source, b"x").expect("write");

    let error = copy_into(&source, &into).expect_err("bad name");

    assert!(matches!(error, FileOpError::InvalidName(_)), "{error}");
    assert_eq!(names(&into), Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn a_fifo_is_refused_without_blocking_and_leaves_no_file() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = TempDir::new();
    let into = dir.sub("into");
    let fifo = dir.0.join("pipe.png");
    let made = std::process::Command::new("/usr/bin/mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(made.success());

    let (sender, receiver) = mpsc::channel();
    let (source, folder) = (fifo.clone(), into.clone());
    std::thread::spawn(move || {
        let _ = sender.send(copy_into(&source, &folder));
    });
    let Ok(result) = receiver.recv_timeout(Duration::from_secs(5)) else {
        // Open the write end so the blocked reader ends, then fail.
        let _ = OpenOptions::new().write(true).open(&fifo);
        panic!("copy_into blocked on a FIFO");
    };

    let error = result.expect_err("fifo");
    assert!(matches!(error, FileOpError::NotAFile(_)), "{error}");
    assert_eq!(names(&into), Vec::<String>::new());
}

#[test]
fn is_unchanged_is_true_for_a_fresh_copy_and_false_after_an_edit_or_a_removal() {
    let dir = TempDir::new();
    let source = dir.0.join("a.png");
    fs::write(&source, b"pixels").expect("write the source");
    let copied = copy_into(&source, &dir.sub("into")).expect("copy");
    assert!(copied.is_unchanged());

    let file = OpenOptions::new()
        .write(true)
        .open(&copied.path)
        .expect("open the copy");
    let saved = file.metadata().expect("read").modified().expect("time");
    file.set_modified(saved + std::time::Duration::from_secs(60))
        .expect("touch the copy");
    drop(file);
    assert!(!copied.is_unchanged(), "the time changed");

    fs::write(&copied.path, b"more pixels").expect("edit the copy");
    let file = OpenOptions::new()
        .write(true)
        .open(&copied.path)
        .expect("open the copy");
    file.set_modified(saved).expect("restore the time");
    drop(file);
    assert!(!copied.is_unchanged(), "the length changed");

    fs::remove_file(&copied.path).expect("remove the copy");
    assert!(!copied.is_unchanged(), "the file is gone");
}

/// Set the modification time of the file at `path`.
fn set_mtime(path: &Path, time: SystemTime) {
    OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open the file")
        .set_modified(time)
        .expect("set the time");
}

#[test]
fn is_unchanged_compares_the_time_below_one_second() {
    let dir = TempDir::new();
    let source = dir.0.join("a.png");
    fs::write(&source, b"pixels").expect("write the source");
    let mut copied = copy_into(&source, &dir.sub("into")).expect("copy");
    let whole = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let saved = whole + std::time::Duration::from_millis(100);
    set_mtime(&copied.path, saved);
    copied.mtime = saved;
    assert!(copied.is_unchanged(), "the saved time matches");

    set_mtime(&copied.path, whole + std::time::Duration::from_millis(900));

    assert!(!copied.is_unchanged(), "the same second, another time");
}

#[cfg(unix)]
#[test]
fn is_unchanged_is_false_for_a_new_file_with_the_same_size_content_and_time() {
    let dir = TempDir::new();
    let source = dir.0.join("a.png");
    fs::write(&source, b"pixels").expect("write the source");
    let copied = copy_into(&source, &dir.sub("into")).expect("copy");
    // The new file is made while the copy still exists, so the inode differs.
    let other = dir.0.join("other.png");
    fs::write(&other, b"pixels").expect("write the new file");
    set_mtime(&other, copied.mtime);
    fs::rename(&other, &copied.path).expect("take the name of the copy");

    assert!(!copied.is_unchanged());
}

#[cfg(unix)]
#[test]
fn is_unchanged_is_false_for_a_link_to_the_copy() {
    let dir = TempDir::new();
    let source = dir.0.join("a.png");
    fs::write(&source, b"pixels").expect("write the source");
    let copied = copy_into(&source, &dir.sub("into")).expect("copy");
    // A link to a second name of the same file: following it gives the same
    // device, inode, size, and time, so only the link check can refuse it.
    let hard = dir.0.join("hard.png");
    fs::hard_link(&copied.path, &hard).expect("hard link");
    fs::remove_file(&copied.path).expect("remove the copy");
    std::os::unix::fs::symlink(&hard, &copied.path).expect("symlink");

    assert!(!copied.is_unchanged());
}
