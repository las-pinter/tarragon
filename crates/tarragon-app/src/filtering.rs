//! Database filtering for the gallery.

use std::collections::HashSet;
use std::path::PathBuf;

use tarragon_core::filter_state::{FilterScope, FilterState};
use tarragon_db::{Database, DbError};

/// Return the paths that match the tag, colour, and folder filters.
///
/// Returns `None` when none of these filters is active in `scope`, which means
/// no database filter. The text filter is never sent to the database. The
/// caller applies the text search in memory. In folder scope the chip folders are dropped,
/// because the folder scope is applied in memory too.
///
/// # Errors
///
/// Returns the [`DbError`] of the query.
pub fn filter_paths(
    db: &Database,
    state: &FilterState,
    scope: FilterScope,
) -> Result<Option<HashSet<PathBuf>>, DbError> {
    let mut query = state.clone();
    query.text.clear();
    if scope == FilterScope::Folder {
        query.folders.clear();
    }
    if query.is_empty(scope) {
        return Ok(None);
    }
    let rows = db.filter_thumbnails(&query)?;
    Ok(Some(rows.iter().map(|row| item_path(&row.path)).collect()))
}

/// Build the gallery item path for a stored database path.
///
/// This is the only place that decides the path form of the result set. The
/// database stores `/` separators, so on Linux a stored path equals the item
/// path only when the scan root was stored in the same form as the gallery item
/// paths. The scanner joins file names to the root as the caller gave it, and
/// the root can be relative, end in a slash, or be a symlink. A file
/// name that contains a backslash cannot match: the database stored it as `/`
/// and this helper cannot tell it from a separator.
fn item_path(stored: &str) -> PathBuf {
    PathBuf::from(stored)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use tarragon_core::color::{ColorBucket, TRANSPARENT_TAG};
    use tarragon_core::entity::{FileInfo, TagSource};
    use tarragon_core::filter_state::{FilterScope, FilterState};
    use tarragon_db::Database;

    use super::filter_paths;

    /// A temporary folder, unique per call, removed on drop.
    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Seed a database with five files and these tags.
    ///
    /// `a`: night, red, transparent. `b`: night, red. `c`: night, blue.
    /// `d`: red, transparent. `e`: none.
    fn seeded() -> (Database, TempDir) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = TempDir(
            std::env::temp_dir().join(format!("tarragon-filtering-{}-{id}", std::process::id())),
        );
        std::fs::create_dir_all(&dir.0).expect("create the temp dir");
        let db = Database::open(dir.0.join("t.db")).expect("open");
        db.initialize().expect("initialize");
        let info = |path: &str| FileInfo {
            path: PathBuf::from(path),
            size: 1,
            mtime: 1,
            width: 0,
            height: 0,
        };
        let files = ["/r/one/a.png", "/r/one/sub/b.png", "/r/two/c.png"]
            .map(info)
            .into_iter()
            .chain(["/r/two/d.png", "/r/three/e.png"].map(info))
            .collect::<Vec<_>>();
        db.write_scan_group(&files).expect("write");
        let tagged = [
            ("night", TagSource::User, vec!["a", "b", "c"]),
            ("red", TagSource::AutoColor, vec!["a", "b", "d"]),
            ("blue", TagSource::AutoColor, vec!["c"]),
            (TRANSPARENT_TAG, TagSource::AutoColor, vec!["a", "d"]),
        ];
        for (name, source, names) in tagged {
            let tag = db.ensure_tag(name, source).expect("tag");
            let paths: Vec<String> = names
                .iter()
                .map(|n| {
                    files
                        .iter()
                        .map(|f| f.path.to_string_lossy().into_owned())
                        .find(|p| p.ends_with(&format!("/{n}.png")))
                        .expect("file")
                })
                .collect();
            db.add_tags_to_files_batch(&paths, &[tag]).expect("attach");
        }
        (db, dir)
    }

    fn names(set: Option<HashSet<PathBuf>>) -> Vec<String> {
        let mut out: Vec<String> = set
            .expect("a filter set")
            .iter()
            .map(|p| {
                p.file_name()
                    .expect("a file name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        out.sort();
        out
    }

    fn run(state: &FilterState, scope: FilterScope) -> Option<HashSet<PathBuf>> {
        let (db, dir) = seeded();
        let out = filter_paths(&db, state, scope).expect("query");
        drop(db);
        drop(dir);
        out
    }

    fn tags(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|t| (*t).to_owned()).collect()
    }

    fn folders(list: &[&str]) -> BTreeSet<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn two_tags_combine_with_and() {
        let state = FilterState {
            tags: tags(&["night", "red"]),
            ..FilterState::default()
        };
        assert_eq!(names(run(&state, FilterScope::Global)), ["a.png", "b.png"]);
    }

    #[test]
    fn colours_and_transparent_combine_with_and() {
        let mut state = FilterState {
            colors: BTreeSet::from([ColorBucket::Red, ColorBucket::Blue]),
            ..FilterState::default()
        };
        assert!(run(&state, FilterScope::Global).is_some_and(|set| set.is_empty()));
        state.colors = BTreeSet::from([ColorBucket::Red]);
        state.transparent = true;
        assert_eq!(names(run(&state, FilterScope::Global)), ["a.png", "d.png"]);
    }

    #[test]
    fn folders_combine_with_or_and_match_subfolders() {
        let state = FilterState {
            folders: folders(&["/r/one", "/r/three"]),
            ..FilterState::default()
        };
        assert_eq!(
            names(run(&state, FilterScope::Global)),
            ["a.png", "b.png", "e.png"]
        );
    }

    #[test]
    fn text_is_ignored_and_the_tag_still_filters() {
        let state = FilterState {
            text: "zzz".to_owned(),
            tags: tags(&["blue"]),
            ..FilterState::default()
        };
        assert_eq!(names(run(&state, FilterScope::Global)), ["c.png"]);
    }

    #[test]
    fn folder_scope_ignores_the_chip_folders() {
        let state = FilterState {
            folders: folders(&["/r/one"]),
            ..FilterState::default()
        };
        assert_eq!(run(&state, FilterScope::Folder), None);
        let state = FilterState {
            tags: tags(&["night"]),
            folders: folders(&["/r/three"]),
            ..FilterState::default()
        };
        let scoped = run(&state, FilterScope::Folder);
        assert_eq!(names(scoped), ["a.png", "b.png", "c.png"]);
    }

    #[test]
    fn no_filter_is_none_and_no_match_is_an_empty_set() {
        let text = FilterState {
            text: "a".to_owned(),
            ..FilterState::default()
        };
        assert_eq!(run(&text, FilterScope::Global), None);
        let none = FilterState {
            tags: tags(&["night", "missing"]),
            ..FilterState::default()
        };
        assert_eq!(run(&none, FilterScope::Global), Some(HashSet::new()));
    }
}
