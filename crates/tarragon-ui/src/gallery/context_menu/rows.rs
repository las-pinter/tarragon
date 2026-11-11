//! The tile menu rows and the event that each row raises.

use std::path::PathBuf;

use egui::Pos2;

use super::GridMenuEvent;

/// The menu rows, without their paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// Open in the editor.
    Open,
    /// Show in the file manager.
    ShowInFileManager,
    /// Render the thumbnails again.
    RegenerateThumbnails,
    /// Add a tag.
    AddTag,
    /// Copy the path.
    CopyPath,
    /// Copy the files.
    Copy,
    /// Paste the files.
    Paste,
    /// Duplicate the files.
    Duplicate,
    /// Rename the file.
    Rename,
    /// Move to the trash.
    Delete,
    /// Delete for good.
    DeletePermanently,
}

/// One menu row.
pub(super) struct Row {
    /// What the row does.
    pub(super) kind: Kind,
    /// The text on the left.
    pub(super) label: String,
    /// The key text on the right, or an empty text.
    pub(super) shortcut: &'static str,
    /// Whether a click works.
    pub(super) enabled: bool,
}

/// The four groups of rows for `count` selected files. Paste works only when
/// one folder is open.
pub(super) fn groups(count: usize, paste: bool) -> [Vec<Row>; 4] {
    let many = count > 1;
    let row = |kind, label: &str, shortcut, enabled| Row {
        kind,
        label: label.to_owned(),
        shortcut,
        enabled,
    };
    let open = if many {
        format!("Open {count} in editor")
    } else {
        "Open in editor".to_owned()
    };
    let regenerate = if many {
        format!("Regenerate {count} Thumbnails")
    } else {
        "Regenerate Thumbnail".to_owned()
    };
    let delete = if many {
        format!("Delete {count} items")
    } else {
        "Delete".to_owned()
    };
    let permanently = if many {
        format!("Delete {count} items permanently...")
    } else {
        "Delete permanently...".to_owned()
    };
    [
        vec![
            row(Kind::Open, &open, "Enter", true),
            row(Kind::ShowInFileManager, "Show in file manager", "", !many),
        ],
        vec![
            row(Kind::RegenerateThumbnails, &regenerate, "", true),
            row(Kind::AddTag, "Add tag...", "", true),
            row(Kind::CopyPath, "Copy path", "", !many),
        ],
        vec![
            row(Kind::Copy, "Copy", "Ctrl+C", true),
            row(Kind::Paste, "Paste", "Ctrl+V", paste),
            row(Kind::Duplicate, "Duplicate", "", true),
            row(Kind::Rename, "Rename...", "F2", !many),
        ],
        vec![
            row(Kind::Delete, &delete, "Del", true),
            row(Kind::DeletePermanently, &permanently, "Shift+Del", true),
        ],
    ]
}

/// The rows of the empty-space menu: one group with Paste. Paste works only
/// when one folder is open.
pub(super) fn empty_groups(paste: bool) -> Vec<Vec<Row>> {
    vec![vec![Row {
        kind: Kind::Paste,
        label: "Paste".to_owned(),
        shortcut: "Ctrl+V",
        enabled: paste,
    }]]
}

/// The event for a click on a row. Paste needs no file. Every other row gives
/// none when no file is selected.
pub(super) fn event(kind: Kind, paths: Vec<PathBuf>, pos: Pos2) -> Option<GridMenuEvent> {
    let first = paths.first().cloned();
    match kind {
        Kind::Paste => Some(GridMenuEvent::Paste),
        _ if paths.is_empty() => None,
        Kind::Open => Some(GridMenuEvent::Open(paths)),
        Kind::ShowInFileManager => first.map(GridMenuEvent::ShowInFileManager),
        Kind::RegenerateThumbnails => Some(GridMenuEvent::RegenerateThumbnails(paths)),
        Kind::AddTag => Some(GridMenuEvent::AddTag { pos, paths }),
        Kind::CopyPath => first.map(GridMenuEvent::CopyPath),
        Kind::Copy => Some(GridMenuEvent::Copy(paths)),
        Kind::Duplicate => Some(GridMenuEvent::Duplicate(paths)),
        Kind::Rename => first.map(GridMenuEvent::Rename),
        Kind::Delete => Some(GridMenuEvent::Delete(paths)),
        Kind::DeletePermanently => Some(GridMenuEvent::DeletePermanently(paths)),
    }
}
