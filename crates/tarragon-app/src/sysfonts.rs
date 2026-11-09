//! Find system fonts that fill the glyphs Roboto lacks: CJK and symbols.
//!
//! The family names are fixed for each OS and the order is fixed. There is no
//! locale detection, so a shared Han character shows the Simplified Chinese
//! form. Only the fixed system font folders are scanned.

use std::collections::HashSet;
use std::path::PathBuf;

use fontdb::{Database, Family, ID, Query, Source, Style, Weight};
use tarragon_ui::FallbackFont;

/// The most CJK faces that `pick` keeps.
///
/// A face costs its whole file in memory, and one `.ttc` can hold many faces
/// (Noto Sans CJK is about 20 MB). So `pick` keeps one face for each file. The
/// first name in the list wins, and the other faces of that file are skipped.
const MAX_CJK: usize = 3;

/// The most symbol faces that `pick` keeps.
const MAX_SYMBOLS: usize = 2;

/// CJK family names, in the order of use: SC, then JP, then KR.
#[cfg(target_os = "linux")]
const CJK: &[&str] = &[
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Noto Sans CJK KR",
    "Source Han Sans SC",
    "WenQuanYi Micro Hei",
    "WenQuanYi Zen Hei",
    "Droid Sans Fallback",
];
/// CJK family names, in the order of use: SC, then JP, then KR.
#[cfg(target_os = "windows")]
const CJK: &[&str] = &["Microsoft YaHei", "Yu Gothic", "Malgun Gothic"];
/// CJK family names, in the order of use: SC, then JP, then KR.
#[cfg(target_os = "macos")]
const CJK: &[&str] = &["PingFang SC", "Hiragino Sans", "Apple SD Gothic Neo"];
/// No font names are known for other systems, so nothing is picked.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
const CJK: &[&str] = &[];

/// Symbol family names, in the order of use.
#[cfg(target_os = "linux")]
// Debian-based systems name this family without the space, other systems might
// use the space, and the match is exact, so both spellings are listed.
const SYMBOLS: &[&str] = &[
    "Noto Sans Symbols",
    "Noto Sans Symbols 2",
    "Noto Sans Symbols2",
    "DejaVu Sans",
];
/// Symbol family names, in the order of use.
#[cfg(target_os = "windows")]
const SYMBOLS: &[&str] = &["Segoe UI Symbol"];
/// Symbol family names, in the order of use.
#[cfg(target_os = "macos")]
const SYMBOLS: &[&str] = &["Apple Symbols"];
/// No font names are known for other systems, so nothing is picked.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
const SYMBOLS: &[&str] = &[];

/// The fonts that were found.
pub(crate) struct Found {
    /// The faces to add as fallbacks: CJK faces first, then symbol faces.
    pub fonts: Vec<FallbackFont>,
    /// True when at least one CJK face was found.
    pub cjk: bool,
}

/// Pick the faces for the given family names from `db`.
///
/// A name matches a family exactly, with the same case. A name with no face,
/// or with a file that cannot be read, is skipped. One face is kept once, even
/// when two names reach it. One file gives one face: a later face from a file
/// that already gave one is skipped and uses no cap slot.
pub(crate) fn pick(db: &Database, cjk: &[&str], symbols: &[&str]) -> Found {
    let mut fonts = Vec::new();
    let mut seen = Seen::default();
    take(db, cjk, MAX_CJK, &mut fonts, &mut seen);
    let cjk_found = !fonts.is_empty();
    take(db, symbols, MAX_SYMBOLS, &mut fonts, &mut seen);
    Found {
        fonts,
        cjk: cjk_found,
    }
}

/// The faces and files that already gave a font, shared by both passes.
#[derive(Default)]
struct Seen {
    /// Faces already kept.
    ids: HashSet<ID>,
    /// Files that already gave a face. A binary source has no path.
    files: HashSet<PathBuf>,
}

/// The path of the file behind a face, or `None` for in-memory data.
fn file_of(db: &Database, id: ID) -> Option<PathBuf> {
    match &db.face(id)?.source {
        Source::File(path) | Source::SharedFile(path, _) => Some(path.clone()),
        Source::Binary(_) => None,
    }
}

/// Add up to `max` new faces for `names` to `fonts`.
fn take(db: &Database, names: &[&str], max: usize, fonts: &mut Vec<FallbackFont>, seen: &mut Seen) {
    let mut kept = 0;
    for name in names {
        if kept == max {
            break;
        }
        let families = [Family::Name(name)];
        let query = Query {
            families: &families,
            weight: Weight::NORMAL,
            style: Style::Normal,
            ..Query::default()
        };
        let Some(id) = db.query(&query) else {
            continue;
        };
        if seen.ids.contains(&id) {
            continue;
        }
        let file = file_of(db, id);
        if file.as_ref().is_some_and(|path| seen.files.contains(path)) {
            continue;
        }
        let Some(font) = db.with_face_data(id, |data, index| FallbackFont {
            name: format!("fallback-{}-{name}", fonts.len()),
            data: data.to_vec(),
            index,
        }) else {
            continue;
        };
        seen.ids.insert(id);
        seen.files.extend(file);
        fonts.push(font);
        kept += 1;
    }
}

/// Scan the system font folders and pick the fallback faces for this OS.
///
/// This reads the font folders, so call it off the UI thread.
pub(crate) fn discover() -> Found {
    let mut db = Database::new();
    db.load_system_fonts();
    pick(&db, CJK, SYMBOLS)
}

#[cfg(test)]
mod tests;
