//! Tests for `Pipeline::regenerate`.

use std::sync::atomic::AtomicUsize;

use crossbeam_channel::{Receiver, Sender};
use tarragon_core::error::RenderError;
use tarragon_db::repository::Thumbnail;

use super::drain::{DRAIN_WAIT, STILL_WAITING};
use super::*;

/// The longest a held render waits when a test fails before it releases.
const HOLD_LIMIT: Duration = Duration::from_secs(10);

/// A render function that holds at the call numbered `hold` until released
/// and fails at each call number in `fail`. Other calls work. The receiver
/// signals that the held call began.
fn scripted_render(
    hold: Option<usize>,
    fail: &'static [usize],
) -> (RenderFn, Receiver<()>, Sender<()>) {
    let (entered_tx, entered) = crossbeam_channel::unbounded::<()>();
    let (release, release_rx) = crossbeam_channel::unbounded::<()>();
    let calls = AtomicUsize::new(0);
    let render: RenderFn = Arc::new(move |path| {
        let call = calls.fetch_add(1, Ordering::SeqCst);
        if hold == Some(call) {
            let _ = entered_tx.send(());
            let _ = release_rx.recv_timeout(HOLD_LIMIT);
        }
        if fail.contains(&call) {
            return Err(RenderError::Decode {
                path: path.to_path_buf(),
                reason: "broken on purpose".to_owned(),
            });
        }
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))
    });
    (render, entered, release)
}

/// Open a folder of `names` and wait until its rows are stored.
fn open_stored(setup: &mut Setup, sub: &str, names: &[&str]) -> PathBuf {
    let folder = setup.dir.sub(sub);
    for name in names {
        write_png(&folder.join(name), 255);
    }
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, names.len());
    setup.pipeline.flush();
    folder
}

fn row(setup: &Setup, file: &Path) -> Thumbnail {
    setup
        .db
        .thumbnail(&path_text(file))
        .expect("read")
        .expect("the row")
}

/// The tier files a row points to.
fn tier_files(row: &Thumbnail) -> Vec<PathBuf> {
    [
        &row.thumbnail_cache_path,
        &row.preview_cache_path,
        &row.full_cache_path,
    ]
    .into_iter()
    .flatten()
    .map(PathBuf::from)
    .collect()
}

/// The cache files under `dir` that are not format markers.
fn tier_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(tier_files_under(&path));
        } else if path.file_name().is_some_and(|name| name != "format=1") {
            found.push(path);
        }
    }
    found
}

#[test]
fn regenerate_resets_one_row_deletes_its_files_and_queues_it() {
    // The third render holds, so the reset is visible before the new files.
    let (render, entered, release) = scripted_render(Some(2), &[]);
    let mut setup = start(TempDir::new(), 1, render);
    let folder = open_stored(&mut setup, "pics", &["a.png", "b.png"]);
    let (a, b) = (folder.join("a.png"), folder.join("b.png"));
    let (a_before, b_before) = (row(&setup, &a), row(&setup, &b));
    assert!(tier_files(&a_before).iter().all(|file| file.exists()));
    assert_eq!(tier_files(&a_before).len(), 3);

    let queued = setup.pipeline.regenerate(&HashSet::from([a.clone()]));

    assert_eq!(queued, 1);
    let a_after = row(&setup, &a);
    assert!(a_after.thumbnail_cache_path.is_none());
    assert!(a_after.preview_cache_path.is_none());
    assert!(a_after.full_cache_path.is_none());
    assert!(a_after.cache_uuid.is_none());
    assert!(tier_files(&a_before).iter().all(|file| !file.exists()));
    assert_eq!(row(&setup, &b), b_before, "the other row is untouched");
    assert!(tier_files(&b_before).iter().all(|file| file.exists()));

    entered.recv_timeout(HOLD_LIMIT).expect("the render began");
    let _ = release.send(());
    let result = setup.results.recv_timeout(HOLD_LIMIT).expect("a result");
    assert_eq!(result.key, a);
    assert!(result.outcome.is_ok());
}

#[test]
fn regenerate_renders_a_file_with_a_remembered_failure_again() {
    // The first render fails; the second holds, so the map can be read.
    let (render, entered, release) = scripted_render(Some(1), &[0]);
    let mut setup = start(TempDir::new(), 1, render);
    let folder = setup.dir.sub("bad");
    let file = folder.join("one.png");
    write_png(&file, 255);
    setup.pipeline.open_folder(&folder).expect("open");
    assert!(collect(&setup.results, 1)[0].outcome.is_err());
    let remembered = |setup: &Setup| {
        setup
            .pipeline
            .failures
            .lock()
            .expect("lock")
            .contains_key(&file)
    };
    assert!(remembered(&setup), "the failure is remembered");

    let queued = setup.pipeline.regenerate(&HashSet::from([file.clone()]));

    assert_eq!(queued, 1);
    assert!(!remembered(&setup), "the failure is forgotten");
    entered.recv_timeout(HOLD_LIMIT).expect("the render began");
    let _ = release.send(());
    let result = setup.results.recv_timeout(HOLD_LIMIT).expect("a result");
    assert!(result.outcome.is_ok(), "the file rendered again");
}

#[test]
fn regenerate_skips_an_unknown_key_and_does_not_count_it() {
    let (render, _entered, _release) = scripted_render(None, &[]);
    let mut setup = start(TempDir::new(), 1, render);
    let first = open_stored(&mut setup, "one", &["x.png"]);
    let x = first.join("x.png");
    let x_before = row(&setup, &x);
    // The second open replaces the known files, so `x.png` is unknown now.
    open_stored(&mut setup, "two", &["y.png"]);
    let nowhere = setup.dir.sub("two").join("nope.png");

    let queued = setup
        .pipeline
        .regenerate(&HashSet::from([x.clone(), nowhere]));

    assert_eq!(queued, 0);
    assert_eq!(row(&setup, &x), x_before, "the unknown row is untouched");
    assert!(tier_files(&x_before).iter().all(|file| file.exists()));
    assert!(
        setup
            .results
            .recv_timeout(Duration::from_millis(200))
            .is_none(),
        "nothing was queued"
    );
}

#[test]
fn regenerate_returns_while_the_results_channel_is_full_and_nobody_reads() {
    let started = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&started);
    let render: RenderFn = Arc::new(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(DynamicImage::ImageRgba8(RgbaImage::new(8, 8)))
    });
    let mut setup = start(TempDir::new(), 1, render);
    let folder = setup.dir.sub("full");
    let names: Vec<String> = (0..=RESULT_BOUND).map(|n| format!("f{n:03}.png")).collect();
    for name in &names {
        write_png(&folder.join(name), 255);
    }
    setup.pipeline.open_folder(&folder).expect("open");
    // The channel fills, then the render of one more file begins. The slack
    // lets that render end and its worker wait to send.
    let end = std::time::Instant::now() + HOLD_LIMIT;
    while started.load(Ordering::SeqCst) <= RESULT_BOUND && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(started.load(Ordering::SeqCst), RESULT_BOUND + 1);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(setup.results.receiver.len(), RESULT_BOUND);
    let keys: HashSet<PathBuf> = names.iter().map(|name| folder.join(name)).collect();
    let queue = Arc::clone(&setup.pipeline.queue);
    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            setup.pipeline.regenerate(&keys);
            done_tx.send(()).expect("signal");
        });
        let returned = done_rx.recv_timeout(DRAIN_WAIT).is_ok();
        if !returned {
            // Without this the scope would wait for the stuck call.
            queue.shutdown();
        }
        assert!(returned, "regenerate returns with no reader");
    });
}

#[test]
fn a_render_in_flight_cannot_link_old_files_after_regenerate() {
    // The first render holds and works. The new render fails, so only an old
    // write could leave files or links behind.
    let (render, entered, release) = scripted_render(Some(0), &[1]);
    let mut setup = start(TempDir::new(), 1, render);
    let folder = setup.dir.sub("flight");
    let file = folder.join("one.png");
    write_png(&file, 255);
    setup.pipeline.open_folder(&folder).expect("open");
    entered.recv_timeout(HOLD_LIMIT).expect("the render began");
    let queue = Arc::clone(&setup.pipeline.queue);
    let keys = HashSet::from([file.clone()]);
    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let queued = setup.pipeline.regenerate(&keys);
            done_tx.send(()).expect("signal");
            assert_eq!(queued, 1);
        });
        let early = done_rx.recv_timeout(STILL_WAITING).is_ok();
        // The release comes before any assert, so a failed check cannot leave
        // the render held and the scope waiting for the thread.
        let _ = release.send(());
        let late = early || done_rx.recv_timeout(DRAIN_WAIT).is_ok();
        if !late {
            queue.shutdown();
        }
        assert!(!early, "regenerate waits for the running render");
        assert!(late, "regenerate returns after the release");
    });

    let old = setup.results.recv_timeout(HOLD_LIMIT).expect("old result");
    assert!(old.outcome.is_ok());
    let new = setup.results.recv_timeout(HOLD_LIMIT).expect("new result");
    assert!(new.outcome.is_err());
    setup.pipeline.flush();
    let after = row(&setup, &file);
    assert!(after.thumbnail_cache_path.is_none(), "no old link");
    assert!(
        tier_files_under(&setup.cache_root).is_empty(),
        "no old file"
    );
}
