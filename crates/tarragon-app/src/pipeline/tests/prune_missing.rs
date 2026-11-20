//! Tests for the removal of rows of files that left a folder.

use tarragon_db::repository::Thumbnail;

use super::*;

/// The stored rows of `folder`, sorted by path.
fn stored(db: &Database, folder: &Path) -> Vec<Thumbnail> {
    let mut rows = db
        .thumbnails_for_folder(&path_text(folder))
        .expect("read the rows");
    // The database match is case-sensitive; keep the exact parent as a guard.
    rows.retain(|row| Path::new(&row.path).parent() == Some(folder));
    rows.sort_by(|left, right| left.path.cmp(&right.path));
    rows
}

/// The three cache paths of a row.
fn cache_files(row: &Thumbnail) -> Vec<PathBuf> {
    [
        &row.thumbnail_cache_path,
        &row.preview_cache_path,
        &row.full_cache_path,
    ]
    .into_iter()
    .map(|path| PathBuf::from(path.as_deref().expect("a cache path")))
    .collect()
}

/// Write `names` as PNGs in `folder`, open it, and wait for every row.
fn open_files(setup: &mut Setup, folder: &Path, names: &[&str]) {
    for name in names {
        write_png(&folder.join(name), 255);
    }
    setup.pipeline.open_folder(folder).expect("open");
    collect(&setup.results, names.len());
    setup.pipeline.flush();
}

fn open_again(setup: &mut Setup, folder: &Path, count: usize) {
    setup.pipeline.open_folder(folder).expect("open again");
    collect(&setup.results, count);
    setup.pipeline.flush();
}

#[test]
fn a_deleted_file_loses_its_row_and_cache_files_and_keeps_its_tag() {
    let calls = Arc::default();
    let mut setup = start(TempDir::new(), 2, counting_render(&calls));
    let folder = setup.dir.sub("pics");
    open_files(&mut setup, &folder, &["a.png", "b.png", "c.png"]);
    let before = stored(&setup.db, &folder);
    assert_eq!(before.len(), 3);
    let gone = &before[1];
    let gone_files = cache_files(gone);
    assert!(gone_files.iter().all(|file| file.exists()));
    let gone_path = gone.path.clone();
    let tagged = color_names(&setup.db, &gone_path);
    assert!(!tagged.is_empty(), "the render tagged the file");

    std::fs::remove_file(&gone_path).expect("delete the source");
    // A cache file that is already missing must not fail the open.
    std::fs::remove_file(&gone_files[1]).expect("delete a cache file");
    open_again(&mut setup, &folder, 2);

    let after = stored(&setup.db, &folder);
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|row| row.path != gone_path));
    assert!(gone_files.iter().all(|file| !file.exists()));
    // The two files that stayed keep their row and their cache files.
    for (kept, old) in after.iter().zip([&before[0], &before[2]]) {
        assert_eq!(kept.path, old.path);
        assert_eq!(kept.mtime, old.mtime);
        assert!(cache_files(kept).iter().all(|file| file.exists()));
    }
    assert_eq!(color_names(&setup.db, &gone_path), tagged);
    // A kept file is served from its cache, so nothing renders again.
    assert_eq!(calls.lock().expect("calls lock").len(), 3);
}

#[test]
fn rows_of_other_folders_and_sub_folders_stay() {
    let mut setup = start(
        TempDir::new(),
        2,
        Arc::new(tarragon_render::registry::render),
    );
    let parent = setup.dir.sub("parent");
    let child = setup.dir.sub("parent/child");
    let other = setup.dir.sub("other");
    open_files(&mut setup, &child, &["deep.png"]);
    open_files(&mut setup, &other, &["side.png"]);
    open_files(&mut setup, &parent, &["top.png", "keep.png"]);
    let child_row = stored(&setup.db, &child);
    let other_row = stored(&setup.db, &other);

    // The child and the other file leave the disk; only the parent is opened.
    std::fs::remove_file(child.join("deep.png")).expect("delete");
    std::fs::remove_file(other.join("side.png")).expect("delete");
    std::fs::remove_file(parent.join("top.png")).expect("delete");
    open_again(&mut setup, &parent, 1);

    assert_eq!(stored(&setup.db, &parent).len(), 1);
    for rows in [&child_row, &other_row] {
        assert_eq!(rows.len(), 1);
        assert!(cache_files(&rows[0]).iter().all(|file| file.exists()));
    }
    assert_eq!(stored(&setup.db, &child).len(), 1);
    assert_eq!(stored(&setup.db, &other).len(), 1);
}

#[test]
fn a_removed_folder_deletes_nothing() {
    let mut setup = start(
        TempDir::new(),
        1,
        Arc::new(tarragon_render::registry::render),
    );
    let folder = setup.dir.sub("gone");
    open_files(&mut setup, &folder, &["a.png"]);
    let rows = stored(&setup.db, &folder);
    assert_eq!(rows.len(), 1);

    std::fs::remove_dir_all(&folder).expect("remove the folder");
    // The scan reads a missing folder as empty.
    let opened = setup.pipeline.open_folder(&folder).expect("open");
    assert_eq!(opened.files.len(), 0);

    assert_eq!(stored(&setup.db, &folder).len(), 1);
    assert!(cache_files(&rows[0]).iter().all(|file| file.exists()));
}

#[test]
fn an_existing_empty_folder_deletes_nothing() {
    let mut setup = start(
        TempDir::new(),
        1,
        Arc::new(tarragon_render::registry::render),
    );
    let folder = setup.dir.sub("mount");
    open_files(&mut setup, &folder, &["a.png"]);
    let rows = stored(&setup.db, &folder);

    std::fs::remove_file(folder.join("a.png")).expect("delete the source");
    let opened = setup.pipeline.open_folder(&folder).expect("open");
    assert_eq!(opened.files.len(), 0);

    assert_eq!(stored(&setup.db, &folder).len(), 1);
    assert!(cache_files(&rows[0]).iter().all(|file| file.exists()));
}

#[test]
fn a_cache_path_outside_the_cache_root_is_not_removed() {
    let mut setup = start(
        TempDir::new(),
        1,
        Arc::new(tarragon_render::registry::render),
    );
    let folder = setup.dir.sub("pics");
    let outside = setup.dir.sub("outside");
    open_files(&mut setup, &folder, &["keep.png"]);
    let victims: Vec<PathBuf> = ["t", "p", "f"]
        .iter()
        .map(|name| {
            let file = outside.join(name);
            std::fs::write(&file, b"data").expect("write");
            file
        })
        .collect();
    let mut record = row_write(&path_text(&folder.join("gone.png")));
    record.thumbnail_cache_path = Some(path_text(&victims[0]));
    record.preview_cache_path = Some(path_text(&victims[1]));
    record.full_cache_path = Some(path_text(&victims[2]));
    setup.db.upsert_thumbnail(&record).expect("store the row");

    open_again(&mut setup, &folder, 1);

    // The row is stale and goes; the files outside the cache stay.
    assert_eq!(stored(&setup.db, &folder).len(), 1);
    assert!(victims.iter().all(|file| file.exists()));
}

#[test]
fn a_folder_that_differs_only_in_case_keeps_its_rows() {
    let mut setup = start(
        TempDir::new(),
        2,
        Arc::new(tarragon_render::registry::render),
    );
    let lower = setup.dir.sub("pics");
    let upper = setup.dir.sub("Pics");
    open_files(&mut setup, &lower, &["a.png", "b.png"]);
    open_files(&mut setup, &upper, &["c.png"]);
    let lower_rows = stored(&setup.db, &lower);
    assert_eq!(lower_rows.len(), 2);

    open_again(&mut setup, &upper, 1);

    assert_eq!(stored(&setup.db, &lower).len(), 2);
    for row in &lower_rows {
        assert!(cache_files(row).iter().all(|file| file.exists()));
    }
}

#[test]
fn a_file_deleted_during_its_render_keeps_no_row() {
    let dir = TempDir::new();
    let folder = dir.sub("pics");
    for name in ["a.png", "b.png"] {
        write_png(&folder.join(name), 255);
    }
    let (entered_tx, entered_rx) = crossbeam_channel::unbounded::<()>();
    let (release_tx, release_rx) = crossbeam_channel::unbounded::<()>();
    let render: RenderFn = Arc::new(move |path| {
        if path.file_name().is_some_and(|name| name == "b.png") {
            entered_tx.send(()).expect("signal");
            // A timeout only frees the worker if an assert failed first.
            let _ = release_rx.recv_timeout(WAIT);
        }
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(4, 4)))
    });
    let mut setup = start(dir, 2, render);
    setup.pipeline.open_folder(&folder).expect("open");
    entered_rx.recv_timeout(WAIT).expect("the render started");

    std::fs::remove_file(folder.join("b.png")).expect("delete the source");
    setup.pipeline.open_folder(&folder).expect("open again");
    collect(&setup.results, 1);
    release_tx.send(()).expect("release");
    setup.pipeline.drain();
    setup.pipeline.flush();

    let rows = stored(&setup.db, &folder);
    assert!(rows.iter().all(|row| !row.path.ends_with("b.png")));
    assert_eq!(rows.len(), 1);
}

#[test]
fn a_failed_row_delete_keeps_the_rows_and_the_cache_files() {
    let calls = Arc::default();
    let mut setup = start(TempDir::new(), 2, counting_render(&calls));
    let folder = setup.dir.sub("pics");
    open_files(&mut setup, &folder, &["a.png", "b.png"]);
    let before = stored(&setup.db, &folder);
    let gone = &before[0];
    let gone_files = cache_files(gone);
    // A second connection makes every row delete fail.
    let other = rusqlite::Connection::open(setup.dir.0.join("test.db")).expect("open");
    other
        .execute_batch(
            "CREATE TRIGGER keep BEFORE DELETE ON thumbnails \
             BEGIN SELECT RAISE(ABORT, 'kept'); END;",
        )
        .expect("add the trigger");

    std::fs::remove_file(&gone.path).expect("delete the source");
    open_again(&mut setup, &folder, 1);

    let after = stored(&setup.db, &folder);
    assert_eq!(after.len(), 2, "the failed delete keeps every row");
    assert!(
        gone_files.iter().all(|file| file.exists()),
        "the rows still point at the cache files, so they stay"
    );
}
