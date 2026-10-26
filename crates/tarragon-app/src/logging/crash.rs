//! Panic hook and crash report files.

use std::fmt::Write as _;
use std::io::{self, Write as _};
use std::panic::{AssertUnwindSafe, PanicHookInfo};
use std::path::{Path, PathBuf};

use tarragon_core::render_scope::inside_render_guard;
use time::{OffsetDateTime, UtcOffset};

use super::stamp;

/// The start of every crash file name.
const PREFIX: &str = "crash-";
/// The end of a crash file name that nobody has marked yet.
const SUFFIX: &str = ".txt";
/// The end of a crash file name after [`mark_crash_report_seen`].
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "read by the crash card at the next start")
)]
const SEEN_SUFFIX: &str = ".seen.txt";
/// How many same-second names to try before giving up.
const MAX_NAME_TRIES: u32 = 100;

/// The facts a crash report holds.
#[derive(Debug, Clone)]
pub(super) struct CrashInfo {
    /// When the panic happened. The report prints it in UTC.
    pub time: OffsetDateTime,
    /// The application version.
    pub version: String,
    /// The operating system and CPU family.
    pub os: String,
    /// The panic message.
    pub message: String,
    /// Where the panic happened, as `file:line:column`.
    pub location: String,
    /// The name of the panicking thread.
    pub thread: String,
    /// The captured backtrace text.
    pub backtrace: String,
}

/// Render the report text.
pub(super) fn render(info: &CrashInfo) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "Tarragon crash report");
    let _ = writeln!(
        text,
        "Time (UTC): {}",
        stamp::date_time(info.time.to_offset(UtcOffset::UTC))
    );
    let _ = writeln!(text, "Version: {}", info.version);
    let _ = writeln!(text, "OS: {}", info.os);
    let _ = writeln!(text, "Thread: {}", info.thread);
    let _ = writeln!(text, "Location: {}", info.location);
    let _ = writeln!(text, "Message: {}", info.message);
    let _ = writeln!(text, "\nBacktrace:\n{}", info.backtrace);
    text
}

/// Write the report into `dir` and return its path.
///
/// The file is new: an existing report is never replaced.
pub(super) fn write_crash_report(dir: &Path, info: &CrashInfo) -> io::Result<PathBuf> {
    let stamp = stamp::file_stamp(info.time);
    let text = render(info);
    for attempt in 0..MAX_NAME_TRIES {
        let name = if attempt == 0 {
            format!("{PREFIX}{stamp}{SUFFIX}")
        } else {
            format!("{PREFIX}{stamp}-{attempt:02}{SUFFIX}")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(text.as_bytes())?;
                file.sync_all()?;
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free crash file name",
    ))
}

/// Log panics and write a crash file into `crash_dir`, then run the hook that
/// was set before.
///
/// A panic inside the render guard is only logged: it gets no crash file and
/// the previous hook does not run.
/// The hook ignores its own I/O errors. It prints the crash file path to
/// stderr.
pub fn install_panic_hook(crash_dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        report(&crash_dir, info);
        // A caught render panic is only logged, so the default panic text
        // stays off the terminal.
        if !inside_render_guard() {
            previous(info);
        }
    }));
}

/// True when a panic must leave a crash file.
///
/// The render guard catches a render panic and turns it into an error, so the
/// app goes on. That panic is only logged.
const fn writes_crash_file(inside_render_guard: bool) -> bool {
    !inside_render_guard
}

/// Log one panic and write its crash file, unless the render guard catches it.
fn report(dir: &Path, info: &PanicHookInfo<'_>) {
    let thread = std::thread::current();
    let crash = CrashInfo {
        time: OffsetDateTime::now_utc(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        message: info
            .payload_as_str()
            .unwrap_or("<non-text panic payload>")
            .to_owned(),
        location: info
            .location()
            .map_or_else(|| "<unknown>".to_owned(), ToString::to_string),
        thread: thread.name().unwrap_or("<unnamed>").to_owned(),
        backtrace: std::backtrace::Backtrace::force_capture().to_string(),
    };
    // The file comes first: the log call is the part most likely to fail.
    if writes_crash_file(inside_render_guard())
        && let Ok(path) = write_crash_report(dir, &crash)
    {
        // `eprintln!` would panic on a closed stderr.
        let _ = writeln!(io::stderr(), "tarragon: crash report: {}", path.display());
    }
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
        tracing::error!(
            target: "tarragon::panic",
            "panic in thread '{}' at {}: {}",
            crash.thread,
            crash.location,
            crash.message
        );
    }));
}

/// The stem of a pending crash file name, or `None` for any other name.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "read by the crash card at the next start")
)]
fn pending_stem(name: &str) -> Option<&str> {
    if name.ends_with(SEEN_SUFFIX) || !name.starts_with(PREFIX) {
        return None;
    }
    name.strip_suffix(SUFFIX)
}

/// The crash files in `dir` that were not marked as seen, newest first.
///
/// A missing or unreadable folder gives an empty list. The names sort in time
/// order, and a same-second repeat sorts after the first file.
#[must_use]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "read by the crash card at the next start")
)]
pub fn pending_crash_reports(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = pending_stem(&name)?.to_owned();
            Some((stem, entry.path()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, path)| path).collect()
}

/// Mark a crash file as seen by renaming `crash-<time>.txt` to
/// `crash-<time>.seen.txt`. Returns the new path.
///
/// # Errors
///
/// Returns an error when the name is not a pending crash file name or the
/// rename fails.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "read by the crash card at the next start")
)]
pub fn mark_crash_report_seen(path: &Path) -> io::Result<PathBuf> {
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(pending_stem)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a pending crash file"))?;
    let seen = path.with_file_name(format!("{stem}{SEEN_SUFFIX}"));
    std::fs::rename(path, &seen)?;
    Ok(seen)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use time::OffsetDateTime;

    use super::{
        CrashInfo, install_panic_hook, mark_crash_report_seen, pending_crash_reports, render,
        write_crash_report,
    };

    /// A temporary folder that removes itself on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir()
                .join(format!("tarragon-crash-test-{}-{id}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("create the temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn info_at(secs: u64) -> CrashInfo {
        CrashInfo {
            time: OffsetDateTime::from_unix_timestamp(i64::try_from(secs).expect("secs"))
                .expect("time"),
            version: "1.2.3".into(),
            os: "testos x86".into(),
            message: "boom".into(),
            location: "src/x.rs:1:2".into(),
            thread: "worker".into(),
            backtrace: "frame 0".into(),
        }
    }

    #[test]
    fn report_has_every_expected_section() {
        let text = render(&info_at(1_700_000_000));
        for expected in [
            "Time (UTC): 2023-11-14 22:13:20",
            "Version: 1.2.3",
            "OS: testos x86",
            "Thread: worker",
            "Location: src/x.rs:1:2",
            "Message: boom",
            "Backtrace:\nframe 0",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in {text}");
        }
    }

    #[test]
    fn crash_file_is_written_with_a_time_name() {
        let temp = TempDir::new();
        let path = write_crash_report(&temp.0, &info_at(1_700_000_000)).expect("write");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("crash-20231114T221320Z.txt")
        );
        assert!(
            std::fs::read_to_string(&path)
                .expect("read")
                .contains("Message: boom")
        );
    }

    #[test]
    fn same_second_crashes_do_not_overwrite_each_other() {
        let temp = TempDir::new();
        let first = write_crash_report(&temp.0, &info_at(1_700_000_000)).expect("write");
        let second = write_crash_report(&temp.0, &info_at(1_700_000_000)).expect("write");
        assert_ne!(first, second);
        assert_eq!(pending_crash_reports(&temp.0), vec![second, first]);
    }

    #[test]
    fn write_fails_when_the_folder_is_missing() {
        let temp = TempDir::new();
        let missing = temp.0.join("missing");
        assert!(write_crash_report(&missing, &info_at(0)).is_err());
    }

    #[test]
    fn pending_lists_unseen_files_newest_first() {
        let temp = TempDir::new();
        let old = write_crash_report(&temp.0, &info_at(1_600_000_000)).expect("write");
        let new = write_crash_report(&temp.0, &info_at(1_700_000_000)).expect("write");
        let seen = write_crash_report(&temp.0, &info_at(1_800_000_000)).expect("write");
        mark_crash_report_seen(&seen).expect("mark");
        std::fs::write(temp.0.join("tarragon.log"), "x").expect("other file");
        assert_eq!(pending_crash_reports(&temp.0), vec![new, old]);
    }

    #[test]
    fn mark_seen_hides_the_file_and_renames_it() {
        let temp = TempDir::new();
        let path = write_crash_report(&temp.0, &info_at(1_700_000_000)).expect("write");
        let seen = mark_crash_report_seen(&path).expect("mark");
        assert!(!path.exists());
        assert!(seen.is_file());
        assert!(seen.to_string_lossy().ends_with(".seen.txt"));
        assert_eq!(pending_crash_reports(&temp.0).len(), 0);
    }

    #[test]
    fn mark_seen_refuses_other_files() {
        let temp = TempDir::new();
        let other = temp.0.join("notes.txt");
        std::fs::write(&other, "x").expect("write");
        assert!(mark_crash_report_seen(&other).is_err());
        let again = write_crash_report(&temp.0, &info_at(5)).expect("write");
        let seen = mark_crash_report_seen(&again).expect("mark");
        assert!(mark_crash_report_seen(&seen).is_err());
    }

    #[test]
    fn pending_is_empty_for_a_missing_folder() {
        let temp = TempDir::new();
        assert_eq!(pending_crash_reports(&temp.0.join("nope")).len(), 0);
    }

    /// Puts the panic hook that was set before the test back on drop.
    type Hook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

    struct HookGuard(Option<Hook>);

    impl Drop for HookGuard {
        fn drop(&mut self) {
            let _ = std::panic::take_hook();
            if let Some(original) = self.0.take() {
                std::panic::set_hook(original);
            }
        }
    }

    #[test]
    fn a_panic_inside_the_render_guard_writes_no_crash_file() {
        assert!(!super::writes_crash_file(true));
        assert!(super::writes_crash_file(false));
    }

    #[test]
    fn hook_writes_a_crash_file_for_a_caught_panic() {
        let _serial = super::super::PANIC_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp = TempDir::new();
        let _restore = HookGuard(Some(std::panic::take_hook()));
        // A quiet hook stands in as the previous one, so the test output
        // stays clean.
        std::panic::set_hook(Box::new(|_| {}));
        install_panic_hook(temp.0.clone());
        let result = std::panic::catch_unwind(|| {
            panic!("hook test panic");
        });
        assert!(result.is_err());
        let files = pending_crash_reports(&temp.0);
        assert_eq!(files.len(), 1);
        let text = std::fs::read_to_string(&files[0]).expect("read");
        assert!(text.contains("Message: hook test panic"));
        assert!(text.contains("Backtrace:"));
    }

    #[test]
    fn hook_writes_no_crash_file_inside_the_render_guard() {
        let _serial = super::super::PANIC_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let temp = TempDir::new();
        let _restore = HookGuard(Some(std::panic::take_hook()));
        let previous_ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&previous_ran);
        std::panic::set_hook(Box::new(move |_| {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        install_panic_hook(temp.0.clone());
        let result = std::panic::catch_unwind(|| {
            let _scope = tarragon_core::render_scope::RenderGuardScope::enter();
            panic!("guarded hook test panic");
        });
        assert!(result.is_err());
        assert_eq!(pending_crash_reports(&temp.0).len(), 0);
        assert!(
            !previous_ran.load(std::sync::atomic::Ordering::SeqCst),
            "the previous hook ran inside the guard"
        );
    }

    #[test]
    fn same_second_names_sort_after_the_first_file() {
        let temp = TempDir::new();
        let info = info_at(1_700_000_000);
        let names: Vec<String> = (0..12)
            .map(|_| {
                let path = write_crash_report(&temp.0, &info).expect("write");
                path.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names[1], "crash-20231114T221320Z-01.txt");
        assert_eq!(names[11], "crash-20231114T221320Z-11.txt");
        let listed = pending_crash_reports(&temp.0);
        assert_eq!(listed.len(), 12);
        assert!(listed[0].ends_with(&names[11]));
        assert!(listed[11].ends_with(&names[0]));
    }
}
