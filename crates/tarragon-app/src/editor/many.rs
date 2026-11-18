//! Open many files with as few process starts as the apps allow.

use std::ffi::OsStr;
use std::path::PathBuf;

use tarragon_db::Database;

use super::plan::plan;
use super::{OpenError, Route, open_with_path, resolve_route, spawn_with_path};

#[cfg(target_os = "linux")]
use super::exec::ExecLine;
#[cfg(target_os = "linux")]
use super::xdg::Env;
#[cfg(target_os = "linux")]
use super::{desktop, system_opener, template_extension, xdg};
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::path::Path;

/// The process settings of the desktop lookups. Only Linux makes them; other
/// systems start one process per file and ask nothing.
#[cfg(not(target_os = "linux"))]
struct Env;

/// What an open of many files did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenReport {
    /// How many files were not opened.
    pub failed: usize,
    /// The text of the first error, if any.
    pub first_error: Option<String>,
}

impl OpenReport {
    /// Count `files` more failed files and keep the first error text.
    fn fail(&mut self, files: usize, error: &OpenError) {
        tracing::warn!("cannot open the file in an editor: {error}");
        self.failed += files;
        self.first_error.get_or_insert_with(|| error.to_string());
    }
}

/// Open `files` in their editors without waiting for an editor.
///
/// One file opens as [`open`](super::open) does. For more, files that share an
/// app go into one start when the app takes many files. A failed start counts
/// every file in it as failed, and the other starts still run.
pub fn open_many(db: &Database, files: &[PathBuf]) -> OpenReport {
    #[cfg(target_os = "linux")]
    let env = Env::from_process();
    #[cfg(not(target_os = "linux"))]
    let env = Env;
    open_many_with(db, files, &env, None)
}

/// [`open_many`] with the lookup settings and an optional `PATH` for the
/// started apps, so tests need not change the environment of the process.
fn open_many_with(db: &Database, files: &[PathBuf], env: &Env, path: Option<&OsStr>) -> OpenReport {
    let mut report = OpenReport {
        failed: 0,
        first_error: None,
    };
    if let [file] = files {
        if let Err(error) = open_with_path(db, file, path) {
            report.fail(1, &error);
        }
        return report;
    }
    let mut routes = Vec::with_capacity(files.len());
    for file in files {
        match resolve_route(db, file) {
            Ok(route) => routes.push((file.clone(), route)),
            Err(error) => report.fail(1, &error.into()),
        }
    }
    use_default_apps(&mut routes, env);
    for planned in plan(routes) {
        match spawn_with_path(&planned.launch, path) {
            Ok(()) => tracing::info!("user opened editor for {} files", planned.paths.len()),
            Err(source) => {
                let program = planned.launch.program.to_string_lossy().into_owned();
                report.fail(planned.paths.len(), &OpenError::Spawn { program, source });
            }
        }
    }
    report
}

/// Give each file that would go to the system opener the default app of its
/// type, so files of one app share a start. Other systems keep the opener.
#[cfg(not(target_os = "linux"))]
fn use_default_apps(_routes: &mut [(PathBuf, Route)], _env: &Env) {}

/// The default app of an extension: the group key and the parsed line.
#[cfg(target_os = "linux")]
type DefaultRoute = Option<(String, ExecLine)>;

/// Give each file that would go to the system opener the default app of its
/// type, so files of one app share a start.
///
/// The app is looked up once per extension, from the first file that has it. A
/// file with no known app keeps the opener. After one query times out no more
/// run, so the wait is one deadline at most and the rest keep the opener.
#[cfg(target_os = "linux")]
fn use_default_apps(routes: &mut [(PathBuf, Route)], env: &Env) {
    let mut found: HashMap<String, DefaultRoute> = HashMap::new();
    let mut timed_out = false;
    for (path, route) in routes {
        if !matches!(route, Route::Single(launch) if *launch == system_opener(path)) {
            continue;
        }
        // Files with no extension share no type, so each keeps the opener.
        let Some(extension) = template_extension(path) else {
            continue;
        };
        if !found.contains_key(&extension) {
            if timed_out {
                continue;
            }
            if let Ok(app) = default_route(path, env) {
                found.insert(extension.clone(), app);
            } else {
                timed_out = true;
                continue;
            }
        }
        if let Some(Some((key, line))) = found.get(&extension) {
            *route = Route::List {
                key: key.clone(),
                line: line.clone(),
            };
        }
    }
}

/// The default app of `file`, as a group key and a parsed line.
#[cfg(target_os = "linux")]
fn default_route(file: &Path, env: &Env) -> Result<DefaultRoute, xdg::TimedOut> {
    let Some(mime) = xdg::mime_of(file, env)? else {
        return Ok(None);
    };
    let Some(id) = xdg::default_app_id(&mime, env)? else {
        return Ok(None);
    };
    Ok(desktop::load(&id, env).map(|app| (format!("desktop:{}", app.id), app.exec)))
}

#[cfg(all(test, target_os = "linux"))]
mod tests;
