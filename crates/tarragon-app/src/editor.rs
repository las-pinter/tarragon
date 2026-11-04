//! Pick the program and arguments that open a file in an editor.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use tarragon_db::{Database, DbError};

/// The key that holds the fallback template for every extension.
const FALLBACK_KEY: &str = "*";
/// The text in a template that stands for the file path.
const PLACEHOLDER: &str = "{file}";

/// A program and its arguments, ready to hand to a process builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// The program to run.
    pub program: OsString,
    /// The arguments, with the file path already in place.
    pub args: Vec<OsString>,
}

/// Find how to open `file`.
///
/// The template for the file extension wins, then the `*` template, then the
/// system opener. A blank template counts as no template.
///
/// # Errors
///
/// Returns the [`DbError`] of a failed template read.
pub fn resolve(db: &Database, file: &Path) -> Result<Launch, DbError> {
    let extension = file
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .filter(|ext| !ext.is_empty());
    let keys = extension.as_deref().into_iter().chain([FALLBACK_KEY]);
    for key in keys {
        let Some(template) = db.editor_command(key)? else {
            continue;
        };
        if let Some(launch) = expand(&template, file) {
            return Ok(launch);
        }
    }
    Ok(system_opener(file))
}

/// Why an editor could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The editor template could not be read.
    #[error("cannot read the editor command: {0}")]
    Db(#[from] DbError),
    /// The program did not start.
    #[error("cannot start `{program}`: {source}")]
    Spawn {
        /// The program that failed to start.
        program: String,
        /// The error from the operating system.
        source: io::Error,
    },
}

/// Open `file` in its editor without waiting for the editor to end.
///
/// # Errors
///
/// Returns [`OpenError::Db`] when the template read fails, and
/// [`OpenError::Spawn`] when the program does not start.
pub fn open(db: &Database, file: &Path) -> Result<(), OpenError> {
    open_with_path(db, file, None)
}

/// [`open`] with an optional `PATH` for the child, so tests need not change the
/// environment of the whole process.
fn open_with_path(db: &Database, file: &Path, path: Option<&OsStr>) -> Result<(), OpenError> {
    let launch = resolve(db, file)?;
    spawn_with_path(&launch, path).map_err(|source| OpenError::Spawn {
        program: launch.program.to_string_lossy().into_owned(),
        source,
    })?;
    let name = file.file_name().unwrap_or(file.as_os_str());
    tracing::info!("user opened editor for {}", name.to_string_lossy());
    Ok(())
}

/// Start the program of `launch` and return at once.
///
/// A small thread waits for the child, so it does not stay a zombie.
///
/// # Errors
///
/// Returns the error of the failed process start.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "`open` starts the child through the path form")
)]
pub fn spawn(launch: &Launch) -> io::Result<()> {
    spawn_with_path(launch, None)
}

/// [`spawn`] with an optional `PATH` for the child.
fn spawn_with_path(launch: &Launch, path: Option<&OsStr>) -> io::Result<()> {
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let mut child = command.spawn()?;
    let waiter = std::thread::Builder::new()
        .name("tarragon-editor-wait".into())
        .spawn(move || {
            let _ = child.wait();
        });
    if let Err(error) = waiter {
        tracing::warn!("cannot start the editor wait thread: {error}");
    }
    Ok(())
}

/// Build a launch from a template, or `None` when it names no program.
fn expand(template: &str, file: &Path) -> Option<Launch> {
    let mut parts = split_template(template);
    if parts.first().is_none_or(String::is_empty) {
        return None;
    }
    let program = OsString::from(parts.remove(0));
    let has_placeholder = parts.iter().any(|part| part.contains(PLACEHOLDER));
    let mut args: Vec<OsString> = parts.iter().map(|part| substitute(part, file)).collect();
    if !has_placeholder {
        args.push(file.as_os_str().to_owned());
    }
    Some(Launch { program, args })
}

/// Replace each placeholder in `part` with the path, keeping a bare path exact.
fn substitute(part: &str, file: &Path) -> OsString {
    if part == PLACEHOLDER {
        return file.as_os_str().to_owned();
    }
    OsString::from(part.replace(PLACEHOLDER, &file.to_string_lossy()))
}

/// Split a template on white space. A double-quoted part is one argument with
/// the quotes removed, and an unclosed quote runs to the end. No shell runs, so
/// there are no escapes.
fn split_template(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    for ch in text.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    parts.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        parts.push(current);
    }
    parts
}

/// The opener the operating system ships for the default application.
fn system_opener(file: &Path) -> Launch {
    let path = file.as_os_str().to_owned();
    if cfg!(target_os = "windows") {
        Launch {
            program: "cmd".into(),
            args: vec!["/C".into(), "start".into(), "".into(), path],
        }
    } else if cfg!(target_os = "macos") {
        Launch {
            program: "open".into(),
            args: vec![path],
        }
    } else {
        Launch {
            program: "xdg-open".into(),
            args: vec![path],
        }
    }
}

#[cfg(test)]
mod tests;
