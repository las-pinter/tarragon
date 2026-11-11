//! Ask the `xdg-mime` tool for the type of a file and the default app of a type.
//!
//! Each query runs the tool with a deadline, so a stalled tool cannot freeze
//! the caller. The tool is not a glib or gtk program. On timeout only the
//! direct child is killed; helpers it started may live on.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::clipboard::{kill_and_reap, read_capped, remaining, wait_until};

/// How long one query may take. The tool ends in about 70 ms; the deadline
/// only guards a hang.
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
/// The most bytes of tool output that a query keeps.
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
/// The type that `xdg-mime` gives when it does not know the file.
const UNKNOWN_MIME: &str = "application/octet-stream";
/// The data folders when `XDG_DATA_DIRS` is not set.
const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";

/// A query did not end before its deadline. The tool was killed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub struct TimedOut;

/// The parts of the process environment that the desktop lookups read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub struct Env {
    /// The `PATH` for the tool. `None` keeps the one of the process.
    pub path: Option<OsString>,
    /// The user data folder (`XDG_DATA_HOME`).
    pub data_home: Option<PathBuf>,
    /// The system data folders (`XDG_DATA_DIRS`), in order.
    pub data_dirs: Vec<PathBuf>,
    /// The deadline of one query.
    pub timeout: Duration,
}

impl Env {
    /// Read the values from the process environment.
    #[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
    pub fn from_process() -> Self {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    /// Read the values through `get`, which gives a variable or `None`.
    ///
    /// An empty value counts as not set.
    fn from_lookup(get: impl Fn(&str) -> Option<OsString>) -> Self {
        let get = |name: &str| get(name).filter(|value| !value.is_empty());
        let data_home = get("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| get("HOME").map(|home| PathBuf::from(home).join(".local/share")));
        let dirs = get("XDG_DATA_DIRS").unwrap_or_else(|| DEFAULT_DATA_DIRS.into());
        Self {
            path: get("PATH"),
            data_home,
            data_dirs: std::env::split_paths(&dirs)
                .filter(|dir| !dir.as_os_str().is_empty())
                .collect(),
            timeout: QUERY_TIMEOUT,
        }
    }
}

/// The MIME type of `file`, from `xdg-mime query filetype`.
///
/// Gives `Ok(None)` when the tool is missing, fails, prints nothing, or does
/// not know the type (`application/octet-stream`).
///
/// # Errors
///
/// Returns [`TimedOut`] when the tool does not end before `env.timeout`.
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub fn mime_of(file: &Path, env: &Env) -> Result<Option<String>, TimedOut> {
    let mime = query("filetype", file.as_os_str(), env)?;
    Ok(mime.filter(|mime| mime != UNKNOWN_MIME))
}

/// The desktop id of the default app for `mime`, from `xdg-mime query default`.
///
/// Gives `Ok(None)` when the tool is missing, fails, or prints nothing.
///
/// # Errors
///
/// Returns [`TimedOut`] when the tool does not end before `env.timeout`.
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub fn default_app_id(mime: &str, env: &Env) -> Result<Option<String>, TimedOut> {
    query("default", OsStr::new(mime), env)
}

/// Why a query stopped before it had an answer.
enum Stop {
    /// The deadline passed.
    Deadline,
    /// The tool could not be read or waited for.
    Io,
}

/// Run `xdg-mime query <kind> <arg>` and give its trimmed output.
///
/// `Ok(None)` stands for a start error, a bad exit, or empty output.
fn query(kind: &str, arg: &OsStr, env: &Env) -> Result<Option<String>, TimedOut> {
    let deadline = Instant::now() + env.timeout;
    let mut command = Command::new("xdg-mime");
    command
        .args(["query", kind])
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(path) = &env.path {
        command.env("PATH", path);
    }
    let Ok(mut child) = command.spawn() else {
        return Ok(None);
    };
    match collect(&mut child, deadline) {
        Ok(text) => Ok(text),
        Err(stop) => {
            // A stalled tool ends here, and its closed pipe ends the reader.
            kill_and_reap(&mut child);
            match stop {
                Stop::Deadline => {
                    tracing::warn!("`xdg-mime query {kind} {}` timed out", arg.display());
                    Err(TimedOut)
                }
                Stop::Io => Ok(None),
            }
        }
    }
}

/// Read the output of `child` and wait for its end, both before `deadline`.
fn collect(child: &mut Child, deadline: Instant) -> Result<Option<String>, Stop> {
    let stdout = child.stdout.take().ok_or(Stop::Io)?;
    let out = read_capped("tarragon-xdg-read", stdout, MAX_OUTPUT_BYTES).map_err(|_| Stop::Io)?;
    let bytes = out
        .recv_timeout(remaining(deadline))
        .map_err(|_| Stop::Deadline)?
        .map_err(|_| Stop::Io)?;
    let status = wait_until(child, deadline)
        .map_err(|_| Stop::Io)?
        .ok_or(Stop::Deadline)?;
    if !status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&bytes).trim().to_owned();
    Ok((!text.is_empty()).then_some(text))
}

#[cfg(test)]
mod tests;
