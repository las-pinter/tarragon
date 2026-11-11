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
    assert!(first.mtime > 0);
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
