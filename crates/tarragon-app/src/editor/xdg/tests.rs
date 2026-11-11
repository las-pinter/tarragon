use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

/// A temporary folder that removes itself on drop.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An empty temporary folder.
fn temp_dir() -> TempDir {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("tarragon-app-xdg-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    TempDir(dir)
}

/// An `Env` with a short deadline and the given `PATH`.
fn env_with_path(path: &OsStr, timeout: Duration) -> Env {
    Env {
        path: Some(path.to_owned()),
        data_home: None,
        data_dirs: Vec::new(),
        timeout,
    }
}

/// A folder with a fake `xdg-mime` that runs `body`, and an `Env` that finds it
/// first. The system folders stay on the `PATH` for `sleep`.
fn fake_tool(body: &str, timeout: Duration) -> (TempDir, Env) {
    let dir = temp_dir();
    let script = dir.0.join("xdg-mime");
    let text = format!("#!/bin/sh\n[ -z \"${PROBE}\" ] || exit 0\n{body}\n");
    std::fs::write(&script, text).expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    wait_until_executable(&script);
    let path = format!("{}:/usr/bin:/bin", dir.0.display());
    let env = env_with_path(OsStr::new(&path), timeout);
    (dir, env)
}

/// Run `script` in probe mode until the system lets it start.
///
/// Another test thread may fork while this thread still holds the write
/// handle of the new script. The child keeps a copy of the handle until it
/// starts its own program, and in that time starting the script fails with
/// "Text file busy". The handle is closed by now, so a start that works shows
/// that no copy is left.
fn wait_until_executable(script: &Path) {
    for _ in 0..500 {
        match Command::new(script).env(PROBE, "1").status() {
            Ok(_) => return,
            Err(error) if error.raw_os_error() == Some(ETXTBSY) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("cannot start the fake tool: {error}"),
        }
    }
    panic!("the fake tool stayed busy");
}

/// Poll `done` until it is true or `limit` has passed.
fn wait_for(limit: Duration, done: impl Fn() -> bool) -> bool {
    let end = Instant::now() + limit;
    while !done() {
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

/// The error number of "Text file busy" on Linux.
const ETXTBSY: i32 = 26;
/// The variable that makes a fake tool end at once, with no work.
const PROBE: &str = "TARRAGON_FAKE_PROBE";
/// A deadline that a fast fake tool always meets.
const LONG: Duration = Duration::from_secs(10);
/// A short deadline that a fast fake tool still meets, even on a loaded machine.
const FAST: Duration = Duration::from_secs(1);
/// A deadline that a hanging fake tool always misses. It leaves time for the
/// tool to write its pid first.
const HANG_TIMEOUT: Duration = Duration::from_millis(500);
/// How much later than its deadline a query may return.
const SLACK: Duration = Duration::from_secs(1);
/// A fake tool that writes its pid to `pid` and then sleeps in the same process.
const HANG: &str = "echo $$ > \"$(dirname \"$0\")/pid\"\nexec sleep 10";

/// Check that the hanging tool of `dir` is gone: no process entry is left, so
/// it was reaped and is not a zombie. A zombie or a live tool would stay.
fn assert_tool_gone(dir: &TempDir) {
    let file = dir.0.join("pid");
    // The pid line is complete when it ends with a newline.
    let written = || std::fs::read_to_string(&file).is_ok_and(|text| text.ends_with('\n'));
    assert!(
        wait_for(Duration::from_secs(2), written),
        "the tool wrote no pid"
    );
    let pid = std::fs::read_to_string(&file).expect("read the pid");
    let pid = pid.trim();
    let entry = Path::new("/proc").join(pid);
    assert!(
        wait_for(Duration::from_secs(1), || !entry.exists()),
        "process {pid} is still there"
    );
}

#[test]
fn mime_of_gives_the_printed_type() {
    let (_dir, env) = fake_tool("echo image/png", LONG);
    assert_eq!(
        mime_of(Path::new("/a/x.png"), &env),
        Ok(Some("image/png".to_owned()))
    );
}

#[test]
fn mime_of_passes_the_query_and_the_file() {
    let (_dir, env) = fake_tool("echo \"$1 $2 $3\"", LONG);
    assert_eq!(
        mime_of(Path::new("/a/b c.png"), &env),
        Ok(Some("query filetype /a/b c.png".to_owned()))
    );
}

#[test]
fn mime_of_gives_none_for_octet_stream() {
    let (_dir, env) = fake_tool("echo application/octet-stream", LONG);
    assert_eq!(mime_of(Path::new("/a/x.psb"), &env), Ok(None));
}

#[test]
fn mime_of_gives_none_for_empty_output() {
    let (_dir, env) = fake_tool("true", LONG);
    assert_eq!(mime_of(Path::new("/a/x.png"), &env), Ok(None));
}

#[test]
fn mime_of_gives_none_when_the_tool_exits_with_an_error() {
    let (_dir, env) = fake_tool("echo image/png\nexit 1", LONG);
    assert_eq!(mime_of(Path::new("/a/x.png"), &env), Ok(None));
}

#[test]
fn default_app_id_gives_the_id_without_the_newline() {
    let (_dir, env) = fake_tool("echo org.gimp.GIMP.desktop", LONG);
    assert_eq!(
        default_app_id("image/png", &env),
        Ok(Some("org.gimp.GIMP.desktop".to_owned()))
    );
}

#[test]
fn default_app_id_passes_the_query_and_the_type() {
    let (_dir, env) = fake_tool("echo \"$1 $2 $3\"", LONG);
    assert_eq!(
        default_app_id("image/png", &env),
        Ok(Some("query default image/png".to_owned()))
    );
}

#[test]
fn default_app_id_gives_none_for_empty_output() {
    let (_dir, env) = fake_tool("echo", LONG);
    assert_eq!(default_app_id("image/png", &env), Ok(None));
}

#[test]
fn both_queries_give_none_when_xdg_mime_is_not_on_the_path() {
    let dir = temp_dir();
    let env = env_with_path(dir.0.as_os_str(), LONG);
    assert_eq!(mime_of(Path::new("/a/x.png"), &env), Ok(None));
    assert_eq!(default_app_id("image/png", &env), Ok(None));
}

#[test]
fn mime_of_kills_and_reaps_a_hanging_tool() {
    let (dir, env) = fake_tool(HANG, HANG_TIMEOUT);
    let start = Instant::now();
    let result = mime_of(Path::new("/a/x.png"), &env);
    assert_eq!(result, Err(TimedOut));
    assert!(start.elapsed() < HANG_TIMEOUT + SLACK);
    assert_tool_gone(&dir);
}

#[test]
fn default_app_id_kills_and_reaps_a_hanging_tool() {
    let (dir, env) = fake_tool(HANG, HANG_TIMEOUT);
    let start = Instant::now();
    let result = default_app_id("image/png", &env);
    assert_eq!(result, Err(TimedOut));
    assert!(start.elapsed() < HANG_TIMEOUT + SLACK);
    assert_tool_gone(&dir);
}

#[test]
fn a_fast_tool_still_answers_under_a_short_deadline() {
    let (_dir, env) = fake_tool("echo image/png", FAST);
    assert_eq!(
        mime_of(Path::new("/a/x.png"), &env),
        Ok(Some("image/png".to_owned()))
    );
    assert_eq!(
        default_app_id("image/png", &env),
        Ok(Some("image/png".to_owned()))
    );
}

/// A variable lookup over a fixed list.
fn lookup(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    move |name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| OsString::from(value))
    }
}

#[test]
fn env_uses_the_defaults_when_nothing_is_set() {
    let env = Env::from_lookup(lookup(&[("HOME", "/home/a")]));
    assert_eq!(env.path, None);
    assert_eq!(env.data_home, Some(PathBuf::from("/home/a/.local/share")));
    assert_eq!(
        env.data_dirs,
        vec![
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share")
        ]
    );
    assert_eq!(env.timeout, QUERY_TIMEOUT);
}

#[test]
fn env_from_process_sets_the_query_deadline() {
    assert_eq!(Env::from_process().timeout, QUERY_TIMEOUT);
}

#[test]
fn env_reads_the_variables_and_skips_blank_data_dirs() {
    let env = Env::from_lookup(lookup(&[
        ("PATH", "/bin"),
        ("HOME", "/home/a"),
        ("XDG_DATA_HOME", "/data"),
        ("XDG_DATA_DIRS", ":/x/share::/y/share:"),
    ]));
    assert_eq!(env.path, Some(OsString::from("/bin")));
    assert_eq!(env.data_home, Some(PathBuf::from("/data")));
    assert_eq!(
        env.data_dirs,
        vec![PathBuf::from("/x/share"), PathBuf::from("/y/share")]
    );
}
