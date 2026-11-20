use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::*;

/// A temporary folder that removes itself on drop.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The error number of "Text file busy" on Linux.
const ETXTBSY: i32 = 26;
/// The variable that makes a fake program end at once, with no work.
const PROBE: &str = "TARRAGON_FAKE_PROBE";
/// A deadline that a fast fake tool always meets.
const LONG: Duration = Duration::from_secs(10);

/// A folder of fake programs and desktop entries, a database with no rows, and
/// the settings that point every lookup at them.
struct Fixture {
    dir: TempDir,
    db: Database,
    env: Env,
    path: OsString,
}

impl Fixture {
    fn new(timeout: Duration) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tarragon-app-many-{}-{id}",
            tarragon_core::testing::run_tag()
        ));
        std::fs::create_dir_all(dir.join("bin")).expect("create bin");
        std::fs::create_dir_all(dir.join("share/applications")).expect("create share");
        let dir = TempDir(dir);
        let db = Database::open(dir.0.join("t.db")).expect("open");
        db.initialize().expect("initialize");
        // The system folders stay on the path for `sleep`.
        let path = OsString::from(format!("{}:/usr/bin:/bin", dir.0.join("bin").display()));
        let env = Env {
            path: Some(path.clone()),
            data_home: Some(dir.0.join("share")),
            data_dirs: Vec::new(),
            timeout,
        };
        Self { dir, db, env, path }
    }

    /// Write an executable fake program `name` into the `bin` folder.
    fn program(&self, name: &str, body: &str) -> PathBuf {
        let script = self.dir.0.join("bin").join(name);
        let text = format!("#!/bin/sh\n[ -z \"${PROBE}\" ] || exit 0\n{body}\n");
        std::fs::write(&script, text).expect("write script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        wait_until_executable(&script);
        script
    }

    /// A fake app that logs `<name> <argument count>` to `starts.log`.
    fn app(&self, name: &str) -> PathBuf {
        let log = self.dir.0.join("starts.log");
        self.program(name, &format!("echo \"{name} $#\" >> {}", log.display()))
    }

    /// A desktop entry `<id>` whose `Exec` is `exec`.
    fn entry(&self, id: &str, exec: &str) {
        let text = format!("[Desktop Entry]\nType=Application\nName=Fake\nExec={exec}\n");
        std::fs::write(self.dir.0.join("share/applications").join(id), text).expect("write entry");
    }

    /// A fake `xdg-mime`: png and jpg are `image/png`, psd is `image/x-psd`,
    /// and the default apps are `viewer.desktop` and `paint.desktop`. It logs
    /// each call to `mime.log`.
    fn mime(&self) {
        let log = self.dir.0.join("mime.log");
        let body = format!(
            "echo \"$2\" >> {}\n\
             case \"$2 $3\" in\n\
             \"filetype \"*.png|\"filetype \"*.PNG|\"filetype \"*.jpg) echo image/png;;\n\
             \"filetype \"*.psd) echo image/x-psd;;\n\
             \"default image/png\") echo viewer.desktop;;\n\
             \"default image/x-psd\") echo paint.desktop;;\n\
             esac",
            log.display()
        );
        self.program("xdg-mime", &body);
    }

    /// A fake `xdg-open` that logs its argument count to `open.log`.
    fn opener(&self) {
        let log = self.dir.0.join("open.log");
        self.program("xdg-open", &format!("echo \"$#\" >> {}", log.display()));
    }

    fn open(&self, files: &[&str]) -> OpenReport {
        let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        open_many_with(&self.db, &files, &self.env, Some(&self.path))
    }

    /// The sorted lines of the log `name`, once it holds `count` lines.
    fn lines(&self, name: &str, count: usize) -> Vec<String> {
        let file = self.dir.0.join(name);
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
            if lines.len() >= count || Instant::now() >= end {
                lines.sort();
                return lines;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Run `script` in probe mode until the system lets it start.
///
/// Another test thread may fork while this thread still holds the write handle
/// of the new script, and then starting the script fails with "Text file busy".
fn wait_until_executable(script: &Path) {
    for _ in 0..500 {
        match std::process::Command::new(script).env(PROBE, "1").status() {
            Ok(_) => return,
            Err(error) if error.raw_os_error() == Some(ETXTBSY) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("cannot start the fake program: {error}"),
        }
    }
    panic!("the fake program stayed busy");
}

/// A fixture with the fake `xdg-mime`, `xdg-open`, and two many-file apps.
fn two_apps() -> Fixture {
    let fixture = Fixture::new(LONG);
    fixture.mime();
    fixture.opener();
    for (id, name) in [("viewer.desktop", "viewer"), ("paint.desktop", "paint")] {
        let program = fixture.app(name);
        fixture.entry(id, &format!("{} %F", program.display()));
    }
    fixture
}

#[test]
fn files_of_two_apps_give_one_start_each() {
    let fixture = two_apps();
    let report = fixture.open(&["/a/1.png", "/a/2.jpg", "/a/3.psd"]);
    assert_eq!(report.failed, 0);
    assert_eq!(report.first_error, None);
    assert_eq!(fixture.lines("starts.log", 2), ["paint 1", "viewer 2"]);
    assert!(!fixture.dir.0.join("open.log").exists());
}

#[test]
fn a_one_file_app_gets_one_start_per_file() {
    let fixture = two_apps();
    let program = fixture.app("viewer");
    fixture.entry("viewer.desktop", &format!("{} %f", program.display()));
    let report = fixture.open(&["/a/1.png", "/a/2.png", "/a/3.png"]);
    assert_eq!(report.failed, 0);
    assert_eq!(
        fixture.lines("starts.log", 3),
        ["viewer 1", "viewer 1", "viewer 1"]
    );
}

#[test]
fn a_missing_desktop_entry_gives_the_opener_once_per_file() {
    let fixture = Fixture::new(LONG);
    fixture.mime();
    fixture.opener();
    let report = fixture.open(&["/a/1.png", "/a/2.png"]);
    assert_eq!(report.failed, 0);
    assert_eq!(fixture.lines("open.log", 2), ["1", "1"]);
}

#[test]
fn a_timed_out_query_ends_all_lookups_and_keeps_one_start_per_file() {
    let fixture = Fixture::new(Duration::from_millis(200));
    fixture.program(
        "xdg-mime",
        &format!(
            "echo call >> {}\nexec sleep 10",
            fixture.dir.0.join("mime.log").display()
        ),
    );
    fixture.opener();
    let start = Instant::now();
    let report = fixture.open(&["/a/1.png", "/a/2.jpg", "/a/3.psd"]);
    // A query that waited out its script would take ten seconds; the bound
    // leaves room for a loaded machine.
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(report.failed, 0);
    assert_eq!(fixture.lines("open.log", 3), ["1", "1", "1"]);
    // The lookups run one after another before `open` returns, so a second
    // query would be in the log by now. Under load the deadline can end the
    // first query before its script writes, so none is allowed too.
    let mime = std::fs::read_to_string(fixture.dir.0.join("mime.log")).unwrap_or_default();
    assert!(mime.lines().count() <= 1, "more than one query: {mime:?}");
}

#[test]
fn a_hanging_query_is_asked_once_and_the_rest_skip_the_lookup() {
    // A deadline long enough for the script to log its call on a loaded
    // machine, so exactly one query must show.
    let fixture = Fixture::new(Duration::from_millis(1500));
    fixture.program(
        "xdg-mime",
        &format!(
            "echo call >> {}\nexec sleep 10",
            fixture.dir.0.join("mime.log").display()
        ),
    );
    fixture.opener();
    let start = Instant::now();
    let report = fixture.open(&["/a/1.png", "/a/2.jpg", "/a/3.psd"]);
    assert!(start.elapsed() < Duration::from_secs(5));
    assert_eq!(report.failed, 0);
    assert_eq!(fixture.lines("open.log", 3), ["1", "1", "1"]);
    assert_eq!(fixture.lines("mime.log", 1), ["call"]);
}

#[test]
fn a_files_row_starts_once_with_all_paths() {
    let fixture = Fixture::new(LONG);
    fixture.app("gimp");
    fixture
        .db
        .upsert_editor_association("*", "gimp {files}")
        .expect("upsert");
    let report = fixture.open(&["/a/1.png", "/a/2.psd", "/a/3.jpg"]);
    assert_eq!(report.failed, 0);
    assert_eq!(fixture.lines("starts.log", 1), ["gimp 3"]);
}

#[test]
fn a_missing_files_program_counts_all_its_paths_as_failed() {
    let fixture = Fixture::new(LONG);
    fixture.app("gimp");
    fixture
        .db
        .upsert_editor_association("psd", "tarragon-no-such-editor {files}")
        .expect("upsert");
    fixture
        .db
        .upsert_editor_association("*", "gimp {files}")
        .expect("upsert");
    let report = fixture.open(&["/a/1.psd", "/a/2.psd", "/a/3.png"]);
    assert_eq!(report.failed, 2);
    let error = report.first_error.expect("an error text");
    assert!(error.contains("tarragon-no-such-editor"));
    // The other start still ran.
    assert_eq!(fixture.lines("starts.log", 1), ["gimp 1"]);
}

#[test]
fn one_file_takes_the_open_path_with_no_lookup() {
    let fixture = two_apps();
    let report = fixture.open(&["/a/1.png"]);
    assert_eq!(report.failed, 0);
    assert_eq!(fixture.lines("open.log", 1), ["1"]);
    assert!(!fixture.dir.0.join("mime.log").exists());
    assert!(!fixture.dir.0.join("starts.log").exists());
}

#[test]
fn one_file_that_fails_is_reported_as_one_failure() {
    let fixture = Fixture::new(LONG);
    fixture
        .db
        .upsert_editor_association("*", "tarragon-no-such-editor {file}")
        .expect("upsert");
    let report = fixture.open(&["/a/1.png"]);
    assert_eq!(report.failed, 1);
    assert!(report.first_error.is_some());
}

#[test]
fn a_user_row_beats_the_desktop_default() {
    let fixture = two_apps();
    fixture.app("gimp");
    fixture
        .db
        .upsert_editor_association("psd", "gimp {file}")
        .expect("upsert");
    let report = fixture.open(&["/a/1.psd", "/a/2.psd"]);
    assert_eq!(report.failed, 0);
    assert_eq!(fixture.lines("starts.log", 2), ["gimp 1", "gimp 1"]);
    // The desktop app would have logged `paint` after this wait.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(fixture.lines("starts.log", 2), ["gimp 1", "gimp 1"]);
}

#[test]
fn files_with_no_extension_keep_the_opener_and_ask_nothing() {
    let fixture = two_apps();
    let report = fixture.open(&["/a/README", "/a/NOTES"]);
    assert_eq!(report.failed, 0);
    assert_eq!(fixture.lines("open.log", 2), ["1", "1"]);
    assert!(!fixture.dir.0.join("mime.log").exists());
}

#[test]
fn extensions_that_differ_in_case_share_one_lookup() {
    let fixture = two_apps();
    let report = fixture.open(&["/a/A.PNG", "/a/b.png"]);
    assert_eq!(report.failed, 0);
    assert_eq!(fixture.lines("starts.log", 1), ["viewer 2"]);
    // One `filetype` query and one `default` query.
    assert_eq!(fixture.lines("mime.log", 2), ["default", "filetype"]);
}
