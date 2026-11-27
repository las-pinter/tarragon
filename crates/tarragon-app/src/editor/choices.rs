//! The apps the Preview offers for one file: its editor rows, then the
//! default app of the system.

use std::ffi::OsStr;
use std::path::Path;

use tarragon_db::{Database, DbError};

use super::{
    Launch, OpenError, expand, open_system_with_path, spawn_with_path, split_template,
    template_extension, template_keys,
};

/// The program suffix on Windows.
const EXE: &str = ".exe";

/// The menu text of the system default app.
pub const DEFAULT_APP: &str = "Default app";

/// One way to open a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// An editor row: the template text and the app name it gives.
    Template {
        /// The app name, from the program of the template.
        name: String,
        /// The stored template.
        template: String,
    },
    /// The default app of the system.
    Default,
}

impl Choice {
    /// The menu text of the choice.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Template { name, .. } => name,
            Self::Default => DEFAULT_APP,
        }
    }
}

/// The ways to open `file`, the one the button uses first: the row of its
/// extension, then the `*` row when it names another template, then the
/// default app. A blank row or one with no program is left out.
///
/// # Errors
///
/// Returns the [`DbError`] of a failed template read.
pub fn choices(db: &Database, file: &Path) -> Result<Vec<Choice>, DbError> {
    let extension = template_extension(file);
    let mut found: Vec<Choice> = Vec::new();
    for key in template_keys(extension.as_deref()) {
        let Some(template) = db.editor_command(key)? else {
            continue;
        };
        let Some(name) = app_name(&template) else {
            continue;
        };
        let seen = found
            .iter()
            .any(|choice| matches!(choice, Choice::Template { template: t, .. } if *t == template));
        if !seen {
            found.push(Choice::Template { name, template });
        }
    }
    found.push(Choice::Default);
    Ok(found)
}

/// The app name of a template: the file name of its program without a
/// trailing `.exe`, with the first letter in upper case, so `krita {file}`
/// gives `Krita` and `/opt/gimp/bin/gimp-2.10` gives `Gimp-2.10`. `None` when
/// the template has no program.
#[must_use]
pub fn app_name(template: &str) -> Option<String> {
    let parts = split_template(template);
    let program = parts.first().filter(|part| !part.is_empty())?;
    let file = Path::new(program).file_name().map_or_else(
        || program.clone(),
        |name| name.to_string_lossy().into_owned(),
    );
    // A Windows program ends in `.exe`, which is not part of the app name.
    let stem = match file.len().checked_sub(EXE.len()) {
        Some(cut) if file.is_char_boundary(cut) && file[cut..].eq_ignore_ascii_case(EXE) => {
            &file[..cut]
        }
        _ => &file,
    };
    let mut letters = stem.chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect())
}

/// The name of the default app for files like `file`, from its desktop
/// entry: the type from `xdg-mime`, the default app of that type, then the
/// entry's `Name`. `Ok(None)` when any step gives nothing.
///
/// # Errors
///
/// Returns [`TimedOut`] when an `xdg-mime` query passes its deadline.
#[cfg(target_os = "linux")]
pub fn default_app_name(file: &Path) -> Result<Option<String>, TimedOut> {
    use super::{desktop, xdg};

    let env = xdg::Env::from_process();
    let Some(mime) = xdg::mime_of(file, &env)? else {
        return Ok(None);
    };
    let Some(id) = xdg::default_app_id(&mime, &env)? else {
        return Ok(None);
    };
    Ok(desktop::load(&id, &env).map(|app| app.name))
}

/// No desktop lookup off Linux: the system opener picks the app.
///
/// # Errors
///
/// Never fails.
#[cfg(not(target_os = "linux"))]
pub fn default_app_name(_file: &Path) -> Result<Option<String>, TimedOut> {
    Ok(None)
}

/// A desktop lookup passed its deadline.
#[cfg(target_os = "linux")]
pub use super::xdg::TimedOut;

/// A desktop lookup passed its deadline. Off Linux no lookup runs.
#[cfg(not(target_os = "linux"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedOut;

/// Open `file` with `choice`, without waiting for the app to end. `path` is
/// the `PATH` for the child, or none to inherit the process one.
///
/// # Errors
///
/// Returns [`OpenError::Spawn`] when the program does not start.
pub fn open_with_path(file: &Path, choice: &Choice, path: Option<&OsStr>) -> Result<(), OpenError> {
    let template = match choice {
        Choice::Template { template, .. } => template,
        Choice::Default => return open_system_with_path(file, path),
    };
    let Some(launch) = expand(template, file) else {
        return open_system_with_path(file, path);
    };
    start(&launch, path)?;
    let name = file.file_name().unwrap_or(file.as_os_str());
    tracing::info!(
        "user opened {} in {}",
        name.to_string_lossy(),
        choice.label()
    );
    Ok(())
}

/// Start `launch` and name its program on failure.
fn start(launch: &Launch, path: Option<&OsStr>) -> Result<(), OpenError> {
    spawn_with_path(launch, path).map_err(|source| OpenError::Spawn {
        program: launch.program.to_string_lossy().into_owned(),
        source,
    })
}

#[cfg(test)]
mod tests;
