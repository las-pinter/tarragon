//! Tests of the live counts the status bar reads, and of the files that need
//! colors.

use super::*;

#[test]
fn a_folder_open_counts_the_files_it_finds_and_names_the_ones_that_need_colors() {
    let mut setup = start(TempDir::new(), 2, counting_render(&Arc::default()));
    let folder = setup.dir.sub("pics");
    for name in ["a.png", "b.png", "c.png"] {
        write_png(&folder.join(name), 255);
    }
    std::fs::write(folder.join("notes.txt"), b"x").expect("write a text file");
    let counters = setup.pipeline.counters();
    counters.found.store(99, Ordering::Relaxed);

    let opened = setup.pipeline.open_folder(&folder).expect("open");

    assert_eq!(
        counters.found.load(Ordering::Relaxed),
        3,
        "the old count is gone"
    );
    assert_eq!(opened.color_keys.len(), 3, "no file has colors yet");
    collect(&setup.results, 3);
    setup.pipeline.flush();

    // Every file has its colors now, so the next open names none.
    let again = setup.pipeline.open_folder(&folder).expect("open again");
    assert!(again.color_keys.is_empty(), "{:?}", again.color_keys);
    collect(&setup.results, 3);
}

#[test]
fn no_file_needs_colors_while_color_tagging_is_off() {
    let mut setup = start(TempDir::new(), 1, counting_render(&Arc::default()));
    set_colors_enabled(&setup.db, false);
    let folder = setup.dir.sub("pics");
    write_png(&folder.join("a.png"), 255);

    let opened = setup.pipeline.open_folder(&folder).expect("open");

    assert!(opened.color_keys.is_empty());
    collect(&setup.results, 1);
}

#[test]
fn a_purge_reports_its_bytes_from_zero() {
    let mut setup = start(TempDir::new(), 2, counting_render(&Arc::default()));
    let folder = setup.dir.sub("pics");
    for name in ["a.png", "b.png"] {
        write_png(&folder.join(name), 255);
    }
    setup.pipeline.open_folder(&folder).expect("open");
    collect(&setup.results, 2);
    setup.pipeline.flush();
    let counters = setup.pipeline.counters();

    let outcome = setup
        .pipeline
        .purge(&setup.dir.sub("data"))
        .expect("the purge works");

    assert!(outcome.bytes_removed > 0);
    assert_eq!(counters.purge.total(), outcome.bytes_removed);
    assert_eq!(counters.purge.done(), outcome.bytes_removed);

    // A second purge starts again from zero and finds nothing.
    setup
        .pipeline
        .purge(&setup.dir.sub("data"))
        .expect("the second purge works");
    assert_eq!((counters.purge.total(), counters.purge.done()), (0, 0));
}
