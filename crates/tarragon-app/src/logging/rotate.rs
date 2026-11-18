//! A size-capped log file writer that keeps a few numbered backups.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The largest size of the log file before it rolls.
pub(super) const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
/// How many rolled files stay on disk: `.1` and `.2`.
pub(super) const LOG_BACKUP_COUNT: u32 = 2;

/// Writes to one file and rolls it over at a size cap.
///
/// A roll moves `name` to `name.1`, `name.1` to `name.2`, and so on. The
/// oldest backup is deleted. When the live file cannot move, it stays and
/// grows, and the roll is tried again after it grows by a tenth of the cap.
#[derive(Debug)]
pub(super) struct RotatingWriter {
    /// The live log file path.
    path: PathBuf,
    /// The open live file. `None` after a failed open; a write reopens it.
    file: Option<File>,
    /// Bytes in the live file.
    size: u64,
    /// The size cap.
    max_bytes: u64,
    /// The number of backups to keep.
    backups: u32,
    /// A rotation problem found while opening, for the caller to report.
    warning: Option<String>,
    /// Whether a rotation problem was already noted. Only the first counts.
    warned: bool,
    /// Whether a later problem is logged here. It is off while opening, when
    /// the logger may not be installed yet.
    announce: bool,
    /// The size below which a roll is not tried. It is set after a failed roll.
    retry_at: u64,
    /// How many rolls were tried.
    #[cfg(test)]
    rolls_tried: u32,
}

impl RotatingWriter {
    /// Open the log with the production limits. An existing file rolls first,
    /// so the last session becomes `.1`.
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        Self::open_with(path, MAX_LOG_BYTES, LOG_BACKUP_COUNT)
    }

    /// Open the log with chosen limits.
    pub(super) fn open_with(path: &Path, max_bytes: u64, backups: u32) -> io::Result<Self> {
        let mut writer = Self {
            path: path.to_path_buf(),
            file: None,
            size: 0,
            max_bytes,
            backups,
            warning: None,
            warned: false,
            announce: false,
            retry_at: 0,
            #[cfg(test)]
            rolls_tried: 0,
        };
        if path.exists() {
            writer.roll()?;
        } else {
            writer.file = Some(create_file(path)?);
        }
        writer.announce = true;
        Ok(writer)
    }

    /// The rotation problem found while opening, if any. It is given once.
    pub(super) fn take_warning(&mut self) -> Option<String> {
        self.warning.take()
    }

    /// Record a rotation problem. Only the first one is kept or logged.
    fn note(&mut self, what: &str, error: &io::Error) {
        if self.warned {
            return;
        }
        self.warned = true;
        let message = format!("log rotation: {what}: {error}");
        if self.announce {
            tracing::warn!("{message}");
        } else {
            self.warning = Some(message);
        }
    }

    /// Note an error unless the file was simply absent, which is normal.
    fn check(&mut self, what: &str, result: io::Result<()>) -> bool {
        match result {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => {
                self.note(what, &error);
                false
            }
        }
    }

    /// Roll the files and open the live file.
    ///
    /// The live file moves first, to a temporary name. If that fails, no
    /// backup is touched. A temporary file comes from an interrupted roll. The
    /// next roll deletes it, which can lose the log of that session.
    fn roll(&mut self) -> io::Result<()> {
        // Close first: other programs may hold handles, and some systems
        // refuse to rename an open file.
        self.file = None;
        #[cfg(test)]
        {
            self.rolls_tried += 1;
        }
        if self.backups == 0 {
            return self.start_fresh();
        }
        let temp = self.rolling_path();
        let _ = std::fs::remove_file(&temp);
        match std::fs::rename(&self.path, &temp) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return self.start_fresh();
            }
            Err(error) => {
                self.note("cannot move the log file", &error);
                return self.keep_live_file();
            }
        }
        // With `.1` absent, the shift below would leave a stale oldest backup
        // in place, so remove it first.
        let oldest = self.backup_path(self.backups);
        self.check(
            "cannot remove the oldest backup",
            std::fs::remove_file(oldest),
        );
        for n in (1..self.backups).rev() {
            let result = std::fs::rename(self.backup_path(n), self.backup_path(n + 1));
            self.check("cannot shift a backup", result);
        }
        if let Err(error) = std::fs::rename(&temp, self.backup_path(1)) {
            self.note("cannot store the rolled log file", &error);
            // Put the content back; if that fails too, the next roll
            // deletes the temporary file.
            if std::fs::rename(&temp, &self.path).is_ok() {
                return self.keep_live_file();
            }
        }
        self.start_fresh()
    }

    /// Open an empty live file after a roll.
    fn start_fresh(&mut self) -> io::Result<()> {
        self.file = Some(create_file(&self.path)?);
        self.size = 0;
        self.retry_at = 0;
        Ok(())
    }

    /// Keep appending to the live file after a failed roll. The next roll
    /// waits until the file grows by a tenth of the cap.
    fn keep_live_file(&mut self) -> io::Result<()> {
        self.reopen()?;
        self.retry_at = self.size.saturating_add((self.max_bytes / 10).max(1));
        Ok(())
    }

    /// The temporary name the live file has while a roll runs.
    fn rolling_path(&self) -> PathBuf {
        let mut name = self.path.clone().into_os_string();
        name.push(".rolling");
        PathBuf::from(name)
    }

    /// Open the live file for appending and read its size.
    fn reopen(&mut self) -> io::Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.size = file.metadata()?.len();
        self.file = Some(file);
        Ok(())
    }

    /// The path of backup `n`.
    fn backup_path(&self, n: u32) -> PathBuf {
        let mut name = self.path.clone().into_os_string();
        name.push(format!(".{n}"));
        PathBuf::from(name)
    }
}

/// Create or truncate the live file.
fn create_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.file.is_none() {
            self.reopen()?;
        }
        let incoming = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        if self.size > 0
            && self.size >= self.retry_at
            && self.size.saturating_add(incoming) > self.max_bytes
        {
            self.roll()?;
        }
        let Some(file) = self.file.as_mut() else {
            return Err(io::Error::other("the log file is not open"));
        };
        let written = file.write(buf)?;
        self.size += u64::try_from(written).unwrap_or(0);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.file.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::RotatingWriter;

    /// A temporary folder that removes itself on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir()
                .join(format!("tarragon-rotate-test-{}-{id}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("create the temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).expect("read")
    }

    fn backup(dir: &Path, n: u32) -> PathBuf {
        dir.join(format!("t.log.{n}"))
    }

    #[test]
    fn writer_rolls_when_the_cap_would_be_exceeded() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        let mut writer = RotatingWriter::open_with(&log, 10, 2).expect("open");
        writer.write_all(b"123456").expect("write");
        assert!(!backup(&temp.0, 1).exists());
        writer.write_all(b"abcdef").expect("write");
        assert_eq!(read(&backup(&temp.0, 1)), "123456");
        assert_eq!(read(&log), "abcdef");
    }

    #[test]
    fn writer_does_not_roll_at_exactly_the_cap() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        let mut writer = RotatingWriter::open_with(&log, 10, 2).expect("open");
        writer.write_all(b"12345").expect("write");
        writer.write_all(b"67890").expect("write");
        assert!(!backup(&temp.0, 1).exists());
        assert_eq!(read(&log), "1234567890");
    }

    #[test]
    fn writer_rolls_one_byte_past_the_cap() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        let mut writer = RotatingWriter::open_with(&log, 10, 2).expect("open");
        writer.write_all(b"12345").expect("write");
        writer.write_all(b"678901").expect("write");
        assert_eq!(read(&backup(&temp.0, 1)), "12345");
        assert_eq!(read(&log), "678901");
    }

    #[test]
    fn writer_keeps_only_two_backups() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        let mut writer = RotatingWriter::open_with(&log, 4, 2).expect("open");
        for chunk in ["aaaa", "bbbb", "cccc", "dddd"] {
            writer.write_all(chunk.as_bytes()).expect("write");
        }
        assert_eq!(read(&log), "dddd");
        assert_eq!(read(&backup(&temp.0, 1)), "cccc");
        assert_eq!(read(&backup(&temp.0, 2)), "bbbb");
        assert!(!backup(&temp.0, 3).exists());
    }

    #[test]
    fn open_rolls_an_existing_file_once() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        std::fs::write(&log, "last session").expect("seed");
        let mut writer = RotatingWriter::open_with(&log, 100, 2).expect("open");
        assert_eq!(writer.take_warning(), None);
        drop(writer);
        assert_eq!(read(&backup(&temp.0, 1)), "last session");
        assert_eq!(read(&log), "");
        assert!(!backup(&temp.0, 2).exists());
    }

    #[test]
    fn open_does_not_roll_when_no_file_exists() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        let writer = RotatingWriter::open(&log).expect("open");
        drop(writer);
        assert!(log.is_file());
        assert!(!backup(&temp.0, 1).exists());
    }

    #[test]
    fn open_shifts_older_backups_up() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        std::fs::write(&log, "new").expect("seed");
        std::fs::write(backup(&temp.0, 1), "mid").expect("seed");
        std::fs::write(backup(&temp.0, 2), "old").expect("seed");
        drop(RotatingWriter::open_with(&log, 100, 2).expect("open"));
        assert_eq!(read(&backup(&temp.0, 1)), "new");
        assert_eq!(read(&backup(&temp.0, 2)), "mid");
    }

    #[test]
    fn open_fails_when_the_folder_is_missing() {
        let temp = TempDir::new();
        let log = temp.0.join("missing").join("t.log");
        assert!(RotatingWriter::open(&log).is_err());
    }

    #[test]
    fn a_single_oversized_write_goes_to_an_empty_file() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        let mut writer = RotatingWriter::open_with(&log, 4, 2).expect("open");
        writer.write_all(b"longer than the cap").expect("write");
        assert!(!backup(&temp.0, 1).exists());
        assert_eq!(read(&log), "longer than the cap");
    }

    #[test]
    fn a_blocked_roll_keeps_the_old_content_and_reports_once() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        std::fs::write(&log, "old").expect("seed");
        std::fs::create_dir(temp.0.join("t.log.rolling")).expect("block");
        let mut writer = RotatingWriter::open_with(&log, 4, 1).expect("open");
        let warning = writer.take_warning().expect("a warning");
        assert!(warning.contains("log rotation"), "{warning}");
        assert_eq!(writer.take_warning(), None);
        writer.write_all(b"more").expect("write");
        writer.write_all(b"!").expect("write");
        drop(writer);
        assert_eq!(read(&log), "oldmore!");
        assert!(temp.0.join("t.log.rolling").is_dir());
    }

    #[test]
    fn a_blocked_live_move_keeps_the_backups_and_retries_rarely() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        std::fs::write(&log, "x".repeat(95)).expect("seed");
        std::fs::write(backup(&temp.0, 1), "one").expect("seed");
        std::fs::write(backup(&temp.0, 2), "two").expect("seed");
        // A directory on the temporary name stops the live move on every
        // system.
        let blocker = temp.0.join("t.log.rolling");
        std::fs::create_dir(&blocker).expect("block");
        let mut writer = RotatingWriter::open_with(&log, 100, 2).expect("open");
        assert!(writer.take_warning().is_some());
        assert_eq!(read(&log).len(), 95);
        for _ in 0..100 {
            writer.write_all(b"y").expect("write");
        }
        // One try at open, then about one per ten bytes.
        assert!(writer.rolls_tried < 20, "{}", writer.rolls_tried);
        drop(writer);
        assert_eq!(read(&backup(&temp.0, 1)), "one");
        assert_eq!(read(&backup(&temp.0, 2)), "two");
        assert_eq!(read(&log).len(), 195);
        assert!(blocker.is_dir());
    }

    #[test]
    fn a_leftover_temporary_file_is_replaced_by_the_next_roll() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        std::fs::write(&log, "live").expect("seed");
        std::fs::write(temp.0.join("t.log.rolling"), "stale").expect("seed");
        drop(RotatingWriter::open_with(&log, 100, 2).expect("open"));
        assert_eq!(read(&backup(&temp.0, 1)), "live");
        assert!(!temp.0.join("t.log.rolling").exists());
    }

    #[test]
    fn a_blocked_backup_slot_still_lets_the_roll_happen() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        std::fs::write(&log, "old").expect("seed");
        std::fs::create_dir(backup(&temp.0, 2)).expect("block");
        let mut writer = RotatingWriter::open_with(&log, 100, 2).expect("open");
        assert!(writer.take_warning().is_some());
        drop(writer);
        assert_eq!(read(&backup(&temp.0, 1)), "old");
        assert_eq!(read(&log), "");
    }

    #[test]
    fn a_missing_backup_is_not_a_problem() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        std::fs::write(&log, "old").expect("seed");
        std::fs::write(backup(&temp.0, 1), "older").expect("seed");
        let mut writer = RotatingWriter::open_with(&log, 100, 2).expect("open");
        assert_eq!(writer.take_warning(), None);
    }

    #[test]
    fn a_write_reopens_a_closed_file() {
        let temp = TempDir::new();
        let log = temp.0.join("t.log");
        let mut writer = RotatingWriter::open_with(&log, 100, 2).expect("open");
        writer.write_all(b"one ").expect("write");
        writer.file = None;
        writer.write_all(b"two").expect("write");
        drop(writer);
        assert_eq!(read(&log), "one two");
    }
}
