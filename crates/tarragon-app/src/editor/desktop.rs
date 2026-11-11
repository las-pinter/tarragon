//! Find and read the desktop entry of an app id.
//!
//! The code reads local files only. It starts no process. The id comes from
//! `xdg-mime query default`; the folders come from [`Env`].

use std::ffi::OsString;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::exec::ExecLine;
use super::xdg::Env;

/// The most bytes of one desktop file that the reader keeps.
const MAX_ENTRY_BYTES: u64 = 1024 * 1024;
/// The most `-` characters of an id that the folder lookup tries as `/`.
/// The lookup tries every mix, so the work grows as two to the power of this.
const MAX_DASH_SPLITS: usize = 8;
/// The name of the group that holds the entry keys.
const GROUP: &str = "[Desktop Entry]";

/// The default app of a type: its desktop id and its `Exec` line.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub struct DefaultApp {
    /// The desktop id, such as `org.kde.krita.desktop`.
    pub id: String,
    /// The parsed `Exec` line of the entry.
    pub exec: ExecLine,
}

/// The keys of the `[Desktop Entry]` group that the reader uses.
#[derive(Default)]
struct Fields {
    kind: Option<String>,
    exec: Option<String>,
    name: Option<String>,
    icon: Option<String>,
    try_exec: Option<String>,
    terminal: Option<String>,
    hidden: Option<String>,
}

/// The path of the desktop file for `id`.
///
/// Looks at `<dir>/applications/<id>` in `env.data_home`, then in each of
/// `env.data_dirs`; the first regular file wins. When no folder has it and
/// `id` holds `-`, it looks again with some `-` read as `/`, so
/// `kde-foo.desktop` finds `applications/kde/foo.desktop`. That second pass
/// goes folder by folder in the same order, and tries the mixes that use the
/// fewest `/` first. Only the first eight `-` of an id are tried as `/`. An id
/// that is empty, `.`, `..`, or holds `/` or NUL gives `None`. A mix of dashes
/// that makes an empty, `.`, or `..` part is skipped.
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub fn locate(id: &str, env: &Env) -> Option<PathBuf> {
    if matches!(id, "" | "." | "..") || id.contains(['/', '\0']) {
        return None;
    }
    let dirs: Vec<PathBuf> = env
        .data_home
        .iter()
        .chain(&env.data_dirs)
        .map(|dir| dir.join("applications"))
        .collect();
    if let Some(found) = dirs.iter().map(|dir| dir.join(id)).find(|p| p.is_file()) {
        return Some(found);
    }
    let variants = dash_variants(id);
    dirs.iter()
        .flat_map(|dir| variants.iter().map(move |variant| dir.join(variant)))
        .find(|path| path.is_file())
}

/// Every relative path of `id` with at least one `-` read as `/`, fewest
/// slashes first. A mix that has an empty part, a `.` part, or a `..` part is
/// left out, so no path leaves the `applications` folder.
fn dash_variants(id: &str) -> Vec<PathBuf> {
    let dashes: Vec<usize> = id
        .match_indices('-')
        .map(|(index, _)| index)
        .take(MAX_DASH_SPLITS)
        .collect();
    let mut masks: Vec<u32> = (1..1u32 << dashes.len()).collect();
    masks.sort_by_key(|mask| mask.count_ones());
    masks
        .into_iter()
        .filter_map(|mask| {
            let mut text = id.to_owned();
            for (bit, index) in dashes.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    text.replace_range(*index..=*index, "/");
                }
            }
            // An empty part, `.`, or `..` could leave the `applications` folder.
            let bad_part = text.split('/').any(|part| matches!(part, "" | "." | ".."));
            (!bad_part).then(|| PathBuf::from(text))
        })
        .collect()
}

/// The default app for the desktop id `id`.
///
/// Reads the `[Desktop Entry]` group of the file that [`locate`] finds. The
/// keys are `Type`, `Exec`, `Name`, `Icon`, `Terminal`, `Hidden`, and
/// `TryExec`. A key with a locale suffix, a comment, and every other group is
/// skipped. The value escapes `\s \n \t \r \\` are undone before the `Exec`
/// text is parsed. A missing `Name` stands for the id.
///
/// Gives `None` when the file is missing, not text, or too large; `Type` is
/// not `Application`; `Terminal` or `Hidden` is `true`; `TryExec` names a
/// program that is not found or not executable; `Exec` is missing; or
/// [`ExecLine::parse`] gives `None`. `NoDisplay` is not read, because it only
/// hides the entry from menus.
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub fn load(id: &str, env: &Env) -> Option<DefaultApp> {
    let path = locate(id, env)?;
    let text = read_text(&path)?;
    let fields = read_fields(&text);
    let is_true = |flag: &Option<String>| flag.as_deref() == Some("true");
    if fields.kind.as_deref() != Some("Application")
        || is_true(&fields.terminal)
        || is_true(&fields.hidden)
    {
        return None;
    }
    if let Some(program) = fields.try_exec.as_deref().filter(|p| !p.is_empty())
        && !is_runnable(program, env.path.clone())
    {
        return None;
    }
    let exec = ExecLine::parse(
        fields.exec.as_deref()?,
        fields.name.as_deref().unwrap_or(id),
        fields.icon.as_deref(),
        &path,
    )?;
    Some(DefaultApp {
        id: id.to_owned(),
        exec,
    })
}

/// The text of a file of at most [`MAX_ENTRY_BYTES`]. `None` for a file that
/// is missing, too large, or not UTF-8.
fn read_text(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    // One byte more than the limit shows a file that is too large.
    file.take(MAX_ENTRY_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() as u64 <= MAX_ENTRY_BYTES).then_some(text)
}

/// Collect the keys of the `[Desktop Entry]` group. The first line of a key wins.
fn read_fields(text: &str) -> Fields {
    let mut fields = Fields::default();
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_group = line == GROUP;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !in_group {
            continue;
        }
        let value = unescape(value.trim());
        let slot = match key.trim() {
            "Type" => &mut fields.kind,
            "Exec" => &mut fields.exec,
            "Name" => &mut fields.name,
            "Icon" => &mut fields.icon,
            "TryExec" => &mut fields.try_exec,
            "Terminal" => &mut fields.terminal,
            "Hidden" => &mut fields.hidden,
            _ => continue,
        };
        slot.get_or_insert(value);
    }
    fields
}

/// Undo the value escapes `\s \n \t \r \\`. Any other backslash stays.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') | None => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

/// Whether `program` is an executable file: by its own path when it holds a
/// `/`, otherwise in a folder of `path` (the process `PATH` when `None`).
fn is_runnable(program: &str, path: Option<OsString>) -> bool {
    let executable = |file: &Path| {
        file.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        return executable(Path::new(program));
    }
    let Some(path) = path.or_else(|| std::env::var_os("PATH")) else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| executable(&dir.join(program)))
}

#[cfg(test)]
mod tests;
