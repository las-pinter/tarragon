//! Tests of the jobs the status bar shows.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use tarragon_ui::status::Job;

use super::tests::Rig;
use super::*;

/// Put the pipeline in a fake open of `target` and return it, so the test
/// can put it back.
fn fake_open(rig: &mut Rig, target: Target) -> Pipeline {
    let (_, done) = crossbeam_channel::bounded(1);
    match std::mem::replace(&mut rig.thumbs.slot, Slot::Opening { target, done }) {
        Slot::Idle(pipeline) => pipeline,
        _ => panic!("the pipeline is not idle"),
    }
}

/// Put the pipeline in a fake run of `work` and return it.
fn fake_work(rig: &mut Rig, work: Work) -> Pipeline {
    let (_, done) = crossbeam_channel::bounded(1);
    match std::mem::replace(&mut rig.thumbs.slot, Slot::Working { work, done }) {
        Slot::Idle(pipeline) => pipeline,
        _ => panic!("the pipeline is not idle"),
    }
}

#[test]
fn an_idle_pipeline_with_settled_tiles_shows_no_job() {
    let mut rig = Rig::new("jobs-idle", None);
    rig.thumbs.skip_library();
    assert_eq!(rig.thumbs.jobs(), []);
}

#[test]
fn a_folder_scan_shows_the_files_it_has_found() {
    let mut rig = Rig::new("jobs-scan", None);
    let folder = rig.folder("one", &["a.png"]);
    rig.thumbs.navigate(folder.clone());
    assert_eq!(rig.thumbs.jobs(), [Job::Scan { found: 0 }], "it waits");
    let pipeline = fake_open(&mut rig, Target::Folder(folder));
    rig.thumbs.pending = None;
    rig.thumbs.counters.found.store(1204, Ordering::Relaxed);

    assert_eq!(rig.thumbs.jobs(), [Job::Scan { found: 1204 }]);

    rig.thumbs.slot = Slot::Idle(pipeline);
}

#[test]
fn a_library_open_shows_while_it_waits_and_runs() {
    let mut rig = Rig::new("jobs-library", None);
    rig.thumbs.show_library();
    assert_eq!(rig.thumbs.jobs(), [Job::Library]);
    let pipeline = fake_open(&mut rig, Target::Library);
    rig.thumbs.library = Library::Absent;

    assert_eq!(rig.thumbs.jobs(), [Job::Library]);

    rig.thumbs.slot = Slot::Idle(pipeline);
}

#[test]
fn thumbnails_then_colors_show_until_their_tiles_settle() {
    let mut rig = Rig::new("jobs-feed", None);
    rig.thumbs.skip_library();
    let keys: Vec<PathBuf> = ["a", "b", "c"].map(PathBuf::from).into();
    rig.thumbs.feed.reset(1, keys.clone());
    rig.thumbs.feed.set_color_keys(keys[..2].to_vec());

    assert_eq!(
        rig.thumbs.jobs(),
        [
            Job::Thumbnails { done: 0, total: 3 },
            Job::Colors { done: 0, total: 2 }
        ]
    );

    // An empty color set shows no color job.
    rig.thumbs.feed.reset(2, keys);
    assert_eq!(rig.thumbs.jobs(), [Job::Thumbnails { done: 0, total: 3 }]);
}

#[test]
fn the_tile_counts_wait_while_an_open_or_other_work_runs() {
    let mut rig = Rig::new("jobs-hidden", None);
    rig.thumbs.skip_library();
    let folder = rig.folder("one", &["a.png"]);
    rig.thumbs.feed.reset(1, [PathBuf::from("a")]);
    let pipeline = fake_open(&mut rig, Target::Folder(folder));
    assert_eq!(rig.thumbs.jobs(), [Job::Scan { found: 0 }]);
    rig.thumbs.slot = Slot::Idle(pipeline);

    let pipeline = fake_work(&mut rig, Work::Regenerate);

    assert_eq!(rig.thumbs.jobs(), []);

    rig.thumbs.slot = Slot::Idle(pipeline);
}

#[test]
fn a_purge_shows_while_it_waits_and_then_its_bytes() {
    let mut rig = Rig::new("jobs-purge", None);
    rig.thumbs.skip_library();
    let progress = Arc::clone(&rig.thumbs.counters);
    // Bytes of an older purge do not show while the new one waits.
    progress.purge.reset();
    rig.thumbs.purge(rig.dir.0.join("data"));
    assert_eq!(rig.thumbs.jobs(), [Job::Purge { done: 0, total: 0 }]);
    rig.thumbs.queued = None;
    let pipeline = fake_work(&mut rig, Work::Purge);

    assert_eq!(rig.thumbs.jobs(), [Job::Purge { done: 0, total: 0 }]);

    rig.thumbs.slot = Slot::Idle(pipeline);
}

#[test]
fn an_eviction_shows_no_job() {
    let mut rig = Rig::new("jobs-upkeep", None);
    rig.thumbs.skip_library();
    let pipeline = fake_work(&mut rig, Work::Upkeep);

    assert_eq!(rig.thumbs.jobs(), []);

    rig.thumbs.slot = Slot::Idle(pipeline);
}
