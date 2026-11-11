use image::DynamicImage;
use tarragon_core::entity::TagSource;
use tarragon_db::repository::{Thumbnail, ThumbnailWrite};

use super::*;

/// A temporary folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tarragon-app-rename-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("create the temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A database, a cache, and a picture folder in one temp dir.
struct Fixture {
    dir: TempDir,
    db: Database,
    cache: CacheStore,
    folder: PathBuf,
}

const UUID: &str = "uuid-1";

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new();
        let db = Database::open(dir.0.join("tarragon.db")).expect("open the database");
        db.initialize().expect("create the schema");
        let folder = dir.0.join("pics");
        fs::create_dir_all(&folder).expect("create the folder");
        db.upsert_folder_uuid(&db_text(&folder), UUID)
            .expect("store the folder id");
        let cache = CacheStore::new(dir.0.join("cache"));
        Self {
            dir,
            db,
            cache,
            folder,
        }
    }

    /// Make a file with a thumbnail row, a tag, a favorite, and tier files.
    fn add_file(&self, name: &str) -> (PathBuf, Vec<PathBuf>) {
        let path = self.folder.join(name);
        fs::write(&path, name.as_bytes()).expect("write the file");
        let text = db_text(&path);
        let tiers = self.add_tiers(name);
        self.db
            .upsert_thumbnail(&ThumbnailWrite {
                path: text.clone(),
                mtime: 11,
                size: 22,
                width: 4,
                height: 4,
                cache_uuid: UUID.to_owned(),
                thumbnail_cache_path: Some(db_text(&tiers[0])),
                preview_cache_path: Some(db_text(&tiers[1])),
                full_cache_path: Some(db_text(&tiers[2])),
                last_accessed: 0,
            })
            .expect("write the row");
        let tag = self
            .db
            .ensure_tag(&format!("tag-{name}"), TagSource::User)
            .expect("make the tag");
        self.db
            .add_tags_to_files_batch(std::slice::from_ref(&text), &[tag])
            .expect("tag the file");
        self.db
            .add_favorite(&text, None, 7)
            .expect("favorite the file");
        (path, tiers)
    }

    /// Drop the favorites table through a second connection, so the next
    /// `rename_file` call fails after it changed the other tables.
    fn break_favorites(&self) {
        rusqlite::Connection::open(self.dir.0.join("tarragon.db"))
            .expect("second connection")
            .execute("DROP TABLE favorites", [])
            .expect("drop the table");
    }

    /// Write the three tier files of a name and give their paths.
    fn add_tiers(&self, name: &str) -> Vec<PathBuf> {
        let entry = self
            .cache
            .write_all(&DynamicImage::new_rgb8(4, 4), "pics", UUID, name)
            .expect("write the tiers");
        vec![entry.thumb320, entry.preview1024, entry.full]
    }

    /// The tag names and the thumbnail row of a path.
    fn tags_and_row(&self, path: &Path) -> (Vec<String>, Option<Thumbnail>) {
        let text = db_text(path);
        let tags = self
            .db
            .tags_for_files(std::slice::from_ref(&text))
            .expect("read the tags")
            .remove(&text)
            .unwrap_or_default()
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        (tags, self.db.thumbnail(&text).expect("read the row"))
    }

    /// The tag names, the favorite sort order, and the thumbnail row of a path.
    fn rows(&self, path: &Path) -> (Vec<String>, Option<i64>, Option<Thumbnail>) {
        let text = db_text(path);
        let (tags, row) = self.tags_and_row(path);
        let favorite = self
            .db
            .list_favorites()
            .expect("read the favorites")
            .into_iter()
            .find(|favorite| favorite.path == text)
            .map(|favorite| favorite.sort_order);
        (tags, favorite, row)
    }
}

#[test]
fn rename_moves_the_file_and_the_rows_and_keeps_tags_and_favorite() {
    let fx = Fixture::new();
    let (old, _) = fx.add_file("a.png");

    let new = rename(&fx.db, &fx.cache, &old, "b").expect("rename");

    assert_eq!(new, fx.folder.join("b.png"));
    assert!(!old.exists());
    assert_eq!(fs::read(&new).expect("read"), b"a.png");
    let (tags, favorite, row) = fx.rows(&new);
    assert_eq!(tags, ["tag-a.png"]);
    assert_eq!(favorite, Some(7));
    let row = row.expect("the thumbnail row moved");
    assert_eq!(row.mtime, 11);
    assert!(row.thumbnail_cache_path.is_none());
    let (tags, favorite, row) = fx.rows(&old);
    assert!(tags.is_empty() && favorite.is_none() && row.is_none());
}

#[test]
fn rename_keeps_the_extension_by_the_shared_split_rule() {
    let fx = Fixture::new();
    let (tarball, _) = fx.add_file("a.tar.gz");
    let (hidden, _) = fx.add_file(".hidden");

    let first = rename(&fx.db, &fx.cache, &tarball, "b").expect("rename a.tar.gz");
    let second = rename(&fx.db, &fx.cache, &hidden, "shown").expect("rename .hidden");

    assert_eq!(first, fx.folder.join("b.gz"));
    assert_eq!(second, fx.folder.join("shown"));
}

#[test]
fn a_name_clash_returns_an_error_and_changes_nothing() {
    let fx = Fixture::new();
    let (old, tiers) = fx.add_file("a.png");
    let (other, _) = fx.add_file("b.png");

    let error = rename(&fx.db, &fx.cache, &old, "b").expect_err("clash");

    assert!(matches!(error, FileOpError::Stem(FileNameError::Taken)));
    assert_eq!(fs::read(&old).expect("read"), b"a.png");
    assert_eq!(fs::read(&other).expect("read"), b"b.png");
    let (tags, favorite, row) = fx.rows(&old);
    assert_eq!(tags, ["tag-a.png"]);
    assert_eq!(favorite, Some(7));
    assert!(row.expect("row stays").thumbnail_cache_path.is_some());
    assert!(tiers.iter().all(|tier| tier.exists()));
    assert_eq!(fx.rows(&other).0, ["tag-b.png"]);
}

#[test]
fn a_file_made_after_the_check_is_not_overwritten() {
    let fx = Fixture::new();
    let (old, _) = fx.add_file("a.png");

    let error = rename_with(
        &fx.db,
        &fx.cache,
        &old,
        "b",
        |target| fs::write(target, b"late").expect("make the late file"),
        |_| {},
    )
    .expect_err("clash");

    assert!(matches!(error, FileOpError::Stem(FileNameError::Taken)));
    assert_eq!(fs::read(fx.folder.join("b.png")).expect("read"), b"late");
    assert_eq!(fs::read(&old).expect("read"), b"a.png");
    assert_eq!(fx.rows(&old).0, ["tag-a.png"]);
}

#[test]
fn a_failing_database_call_moves_the_file_back() {
    let fx = Fixture::new();
    let (old, tiers) = fx.add_file("a.png");
    fx.break_favorites();

    let error = rename(&fx.db, &fx.cache, &old, "b").expect_err("database failure");

    assert!(matches!(error, FileOpError::Db(_)));
    assert_eq!(fs::read(&old).expect("the old name is back"), b"a.png");
    assert!(!fx.folder.join("b.png").exists());
    assert!(tiers.iter().all(|tier| tier.exists()));
    // The injection drops the favorites table, so only tags and the row can be read.
    let (tags, row) = fx.tags_and_row(&old);
    assert_eq!(tags, ["tag-a.png"]);
    assert!(row.expect("row stays").thumbnail_cache_path.is_some());
    let (new_tags, _) = fx.tags_and_row(&fx.folder.join("b.png"));
    assert_eq!(new_tags, Vec::<String>::new());
}

#[test]
fn a_failed_move_back_reports_where_the_file_is() {
    let fx = Fixture::new();
    let (old, _) = fx.add_file("a.png");
    fx.break_favorites();

    let error = rename_with(
        &fx.db,
        &fx.cache,
        &old,
        "b",
        |_| {},
        |old| fs::write(old, b"late").expect("make a file at the old name"),
    )
    .expect_err("rollback failure");

    let new = fx.folder.join("b.png");
    let FileOpError::RollbackFailed {
        from,
        to,
        db: _,
        source,
    } = &error
    else {
        panic!("expected RollbackFailed, got {error:?}");
    };
    assert_eq!((from, to), (&new, &old));
    assert!(matches!(**source, FileOpError::Stem(FileNameError::Taken)));
    assert!(error.to_string().contains("move it back to"));
    assert_eq!(
        fs::read(&new).expect("the file is at the new name"),
        b"a.png"
    );
    assert_eq!(fs::read(&old).expect("the late file stays"), b"late");
}

#[test]
fn a_new_name_over_the_limit_by_a_trailing_dot_is_refused() {
    let fx = Fixture::new();
    let old = fx.folder.join("a.");
    fs::write(&old, b"x").expect("write the file");

    // "b" repeated 255 times and the dot make 256 bytes; the stem rule sees 255.
    let error = rename(&fx.db, &fx.cache, &old, &"b".repeat(255)).expect_err("too long");

    assert!(matches!(error, FileOpError::Stem(FileNameError::TooLong)));
    assert!(old.exists());
    let fits = rename(&fx.db, &fx.cache, &old, &"b".repeat(254)).expect("255 bytes fit");
    assert_eq!(fits.file_name().expect("name").len(), 255);
}

#[test]
fn old_tier_files_and_stale_row_tier_files_are_deleted_after_success() {
    let fx = Fixture::new();
    let (old, old_tiers) = fx.add_file("a.png");
    // A row at the new path from a file that is gone.
    let stale_tiers = fx.add_tiers("b.png");
    fx.db
        .upsert_thumbnail(&ThumbnailWrite {
            path: db_text(&fx.folder.join("b.png")),
            mtime: 1,
            size: 1,
            width: 1,
            height: 1,
            cache_uuid: UUID.to_owned(),
            thumbnail_cache_path: Some(db_text(&stale_tiers[0])),
            preview_cache_path: Some(db_text(&stale_tiers[1])),
            full_cache_path: Some(db_text(&stale_tiers[2])),
            last_accessed: 0,
        })
        .expect("write the stale row");

    let new = rename(&fx.db, &fx.cache, &old, "b").expect("rename");

    assert!(
        old_tiers
            .iter()
            .chain(&stale_tiers)
            .all(|tier| !tier.exists())
    );
    assert_eq!(fx.rows(&new).2.expect("row").mtime, 11);
}

/// Restores the mode of a folder on drop, so a panic cannot leave it locked.
#[cfg(unix)]
struct Unlock(PathBuf);

#[cfg(unix)]
impl Drop for Unlock {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

#[cfg(unix)]
#[test]
fn a_failed_tier_delete_is_logged_and_the_call_still_succeeds() {
    use std::os::unix::fs::PermissionsExt;

    let fx = Fixture::new();
    let (old, tiers) = fx.add_file("a.png");
    let tier_dir = tiers[0].parent().expect("tier dir").to_path_buf();
    let _unlock = Unlock(tier_dir.clone());
    fs::set_permissions(&tier_dir, fs::Permissions::from_mode(0o555)).expect("lock the dir");
    // A user that ignores the mode, such as root, cannot run this test.
    if fs::write(tier_dir.join("probe"), b"x").is_ok() {
        return;
    }

    let result = rename(&fx.db, &fx.cache, &old, "b");

    assert_eq!(result.expect("still Ok"), fx.folder.join("b.png"));
    assert!(tiers[0].exists());
}
