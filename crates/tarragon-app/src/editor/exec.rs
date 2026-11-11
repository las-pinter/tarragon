//! Turn the `Exec` text of a desktop entry into a launch for one or many files.
//!
//! The code is pure: it reads no file and starts no process.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::Launch;
use crate::clipboard::{encode_into, path_bytes};

/// How many files one start of the app takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub enum ListKind {
    /// One file (`%f`, `%u`): the caller starts the app once for each file.
    One,
    /// Many files (`%F`, `%U`): the caller starts the app once for all files.
    Many,
}

/// An `Exec` line with a slot for the files.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
pub struct ExecLine {
    /// The program to run.
    pub program: String,
    /// The arguments before the file slot.
    pub before: Vec<String>,
    /// The arguments after the file slot.
    pub after: Vec<String>,
    /// Whether the slot takes one file or many.
    pub list: ListKind,
    /// Whether the slot takes `file://` URIs (`%u`, `%U`) and not paths.
    pub uri: bool,
}

/// What one `Exec` argument becomes.
enum Expanded {
    /// Zero or more plain arguments.
    Args(Vec<String>),
    /// The file slot.
    Files { list: ListKind, uri: bool },
}

#[cfg_attr(not(test), expect(dead_code, reason = "wired in brief 06"))]
impl ExecLine {
    /// Parse the `Exec` text of the entry at `entry_path`.
    ///
    /// `exec` is the value after the entry reader has undone the string escapes
    /// of the file format. Quoting follows the Desktop Entry spec. `%%` gives
    /// `%`. `%i` gives `--icon` and `icon`, and nothing without an icon. `%c`
    /// is `name` and `%k` is `entry_path`. The deprecated codes give nothing.
    /// Only the first of `%f %F %u %U` marks the file slot.
    ///
    /// The result is `None` when the line is not usable here: no file code, an
    /// empty program, an unclosed quote, an unknown field code, or a file code
    /// or `%i` inside a longer argument.
    pub fn parse(
        exec: &str,
        name: &str,
        icon: Option<&str>,
        entry_path: &Path,
    ) -> Option<ExecLine> {
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut slot = None;
        for token in split_exec(exec)? {
            match expand_token(&token, name, icon, entry_path)? {
                Expanded::Args(args) if slot.is_none() => before.extend(args),
                Expanded::Args(args) => after.extend(args),
                Expanded::Files { list, uri } => {
                    slot.get_or_insert((list, uri));
                }
            }
        }
        let (list, uri) = slot?;
        if before.is_empty() {
            return None;
        }
        let program = before.remove(0);
        if program.is_empty() {
            return None;
        }
        Some(ExecLine {
            program,
            before,
            after,
            list,
            uri,
        })
    }

    /// The launch that opens `files`, in order, in the file slot.
    ///
    /// The `uri` kind gives percent-encoded `file://` URIs. A path that is not
    /// absolute stays a plain path. The caller passes one file for
    /// [`ListKind::One`].
    pub fn launch(&self, files: &[PathBuf]) -> Launch {
        let mut args: Vec<OsString> = self.before.iter().map(OsString::from).collect();
        args.extend(files.iter().map(|file| self.file_arg(file)));
        args.extend(self.after.iter().map(OsString::from));
        Launch {
            program: OsString::from(&self.program),
            args,
        }
    }

    /// One file as an argument: a path, or a `file://` URI for the `uri` kind.
    fn file_arg(&self, file: &Path) -> OsString {
        if !self.uri || !file.is_absolute() {
            return file.as_os_str().to_owned();
        }
        let mut text = String::from("file://");
        encode_into(&mut text, &path_bytes(file));
        text.into()
    }
}

/// Split `text` into arguments and undo the quoting. A double quote opens or
/// closes a quoted part. Inside one, a backslash before `"`, a backtick, `$`,
/// or `\` gives that character. Outside quotes a backslash is plain text.
/// An unclosed quote gives `None`.
fn split_exec(text: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                match chars.next_if(|next| matches!(next, '"' | '`' | '$' | '\\')) {
                    Some(escaped) => current.push(escaped),
                    None => current.push('\\'),
                }
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
    if quoted {
        return None;
    }
    if started {
        parts.push(current);
    }
    Some(parts)
}

/// Expand the field codes of one unquoted argument. An unknown code, a lone
/// `%` at the end, or a file code or `%i` inside a longer argument gives `None`.
fn expand_token(
    token: &str,
    name: &str,
    icon: Option<&str>,
    entry_path: &Path,
) -> Option<Expanded> {
    let files = |list, uri| Some(Expanded::Files { list, uri });
    match token {
        "%f" => return files(ListKind::One, false),
        "%u" => return files(ListKind::One, true),
        "%F" => return files(ListKind::Many, false),
        "%U" => return files(ListKind::Many, true),
        "%i" => {
            let args = icon
                .filter(|icon| !icon.is_empty())
                .map(|icon| vec!["--icon".to_owned(), icon.to_owned()]);
            return Some(Expanded::Args(args.unwrap_or_default()));
        }
        _ => {}
    }
    let mut out = String::new();
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next()? {
            '%' => out.push('%'),
            'c' => out.push_str(name),
            'k' => out.push_str(&entry_path.to_string_lossy()),
            'd' | 'D' | 'n' | 'N' | 'v' | 'm' => {}
            _ => return None,
        }
    }
    // A code that gives nothing removes its argument. A quoted empty argument stays.
    if out.is_empty() && !token.is_empty() {
        return Some(Expanded::Args(Vec::new()));
    }
    Some(Expanded::Args(vec![out]))
}

#[cfg(test)]
mod tests;
