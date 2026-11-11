//! Tests for the grid menu choices that the app carries out. No test runs a
//! real editor, file manager, or clipboard tool.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{Context, Event, Key, Modifiers, OutputCommand};
use tarragon_db::Database;

use super::*;
use crate::app::preview_tests::Rig;

/// Room for a thread to finish; each wait ends at this limit.
const LIMIT: Duration = Duration::from_secs(10);

/// The Dimensions row the Preview shows for the stored rows.
const DIMENSIONS: &str = "64 x 48";

fn path_of(rig: &Rig, name: &str) -> PathBuf {
    rig.dir.0.join(name)
}

/// Draw frames until a painted text holds `text`, or fail after [`LIMIT`].
fn wait_for_text(rig: &mut Rig, text: &str) {
    let end = Instant::now() + LIMIT;
    loop {
        if rig.frame(Vec::new()).0.iter().any(|t| t.contains(text)) {
            return;
        }
        assert!(Instant::now() < end, "timed out: no text {text:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn copy_path_puts_the_path_text_in_copied_text() {
    let mut rig = Rig::new(&[], &[], 9);
    let event = GridMenuEvent::CopyPath(PathBuf::from("/pics/one.png"));
    rig.app.apply_menu_event(&rig.ctx, event);
    let commands = rig.ctx.output(|output| output.commands.clone());
    assert!(
        commands.iter().any(
            |command| matches!(command, OutputCommand::CopyText(text) if text == "/pics/one.png")
        ),
        "{commands:?}"
    );
}

#[test]
fn tag_paths_writes_the_tag_to_every_path_and_refreshes_the_preview() {
    let mut rig = Rig::new(&["a.png", "b.png"], &[], 9);
    // One file is selected, so the Preview shows its pills.
    rig.click("a.png", false);
    assert!(rig.frame(Vec::new()).0.iter().any(|t| t == DIMENSIONS));
    assert!(!rig.frame(Vec::new()).0.iter().any(|t| t == "cats"));

    let paths = vec![path_of(&rig, "a.png"), path_of(&rig, "b.png")];
    let event = GridMenuEvent::TagPaths {
        paths: paths.clone(),
        tag: "cats".to_owned(),
    };
    rig.app.apply_menu_event(&rig.ctx, event);

    let db = Database::open(rig.dir.0.join("tarragon.db")).expect("open");
    let stored: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let by_file = db.tags_for_files(&stored).expect("read the tags");
    for path in &stored {
        let names: Vec<&str> = by_file[path].iter().map(|tag| tag.name.as_str()).collect();
        assert_eq!(names, ["cats"], "{path}");
    }
    // The selection did not change, so only the handler could refresh the pills.
    assert!(rig.frame(Vec::new()).0.iter().any(|t| t == "cats"));
}

#[cfg(unix)]
#[test]
fn open_starts_the_editor_once_for_each_path() {
    let mut rig = Rig::new(&[], &[], 9);
    let out = rig.dir.0.join("out.txt");
    // Only double quotes group a template part, so the script has none inside.
    let template = format!("sh -c \"echo $0 >> {}\" {{file}}", out.display());
    rig.app
        .db
        .upsert_editor_association("*", &template)
        .expect("store the template");
    let paths = vec![path_of(&rig, "a.png"), path_of(&rig, "b.png")];
    rig.app
        .apply_menu_event(&rig.ctx, GridMenuEvent::Open(paths.clone()));

    let end = Instant::now() + LIMIT;
    let text = loop {
        let text = std::fs::read_to_string(&out).unwrap_or_default();
        if text.lines().count() >= 2 {
            break text;
        }
        assert!(Instant::now() < end, "timed out: the editors wrote");
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut lines: Vec<&Path> = text.lines().map(Path::new).collect();
    lines.sort();
    assert_eq!(lines, paths);
}

#[cfg(unix)]
#[test]
fn a_failing_editor_shows_the_error_notice() {
    let mut rig = Rig::new(&[], &[], 9);
    rig.app
        .db
        .upsert_editor_association("*", "tarragon-no-such-editor-program {file}")
        .expect("store the template");
    let event = GridMenuEvent::Open(vec![path_of(&rig, "a.png")]);
    rig.app.apply_menu_event(&rig.ctx, event);
    wait_for_text(&mut rig, "tarragon-no-such-editor-program");
}

#[test]
fn a_missing_clipboard_tool_shows_the_install_notice() {
    let mut rig = Rig::new(&[], &[], 9);
    let paths = vec![path_of(&rig, "a.png")];
    rig.app.start_copy(&rig.ctx, paths, |_| {
        Err(ClipboardError::ToolMissing("xclip"))
    });
    wait_for_text(&mut rig, "Install xclip to copy files");
}

#[test]
fn a_copy_does_not_block_the_caller_while_the_tool_runs() {
    let mut rig = Rig::new(&[], &[], 9);
    let (release, wait) = crossbeam_channel::bounded::<()>(1);
    let start = Instant::now();
    rig.app.start_copy(&rig.ctx, Vec::new(), move |_| {
        let _ = wait.recv_timeout(Duration::from_secs(3));
        Ok(())
    });
    let returned_in = start.elapsed();
    let _ = release.send(());
    assert!(
        returned_in < Duration::from_secs(1),
        "start_copy took {returned_in:?}"
    );
}

#[test]
fn the_end_of_a_regenerate_shows_the_count_and_asks_for_a_repaint() {
    let mut rig = Rig::new(&[], &[], 9);
    // The first frame takes the start-up eviction off the queue.
    rig.frame(Vec::new());
    let event = GridMenuEvent::RegenerateThumbnails(vec![path_of(&rig, "a.png")]);
    rig.app.apply_menu_event(&rig.ctx, event);

    // This context never draws, so only a notice can ask it for a repaint.
    let probe = Context::default();
    // A new context asks for a repaint during its first passes.
    for _ in 0..4 {
        crate::test_ui::frame(&probe, Vec::new(), &mut |_| {});
    }
    assert!(!probe.has_requested_repaint());
    let end = Instant::now() + LIMIT;
    while !probe.has_requested_repaint() {
        rig.app
            .thumbs
            .before_draw(&rig.ctx, rig.app.shell.gallery_mut());
        rig.app.poll_menu_work(&probe);
        assert!(Instant::now() < end, "timed out: the work never ended");
        std::thread::sleep(Duration::from_millis(10));
    }
    // The file is not in any open folder, so no thumbnail was queued.
    wait_for_text(&mut rig, "Queued 0 thumbnails to render again");
}

#[test]
fn a_frame_shows_the_regenerate_notice_and_the_prefs_poll_does_not_take_it() {
    let mut rig = Rig::new(&[], &[], 9);
    rig.frame(Vec::new());
    let event = GridMenuEvent::RegenerateThumbnails(vec![path_of(&rig, "a.png")]);
    rig.app.apply_menu_event(&rig.ctx, event);
    // Only whole frames run, so the prefs poll sees the result as any frame does.
    wait_for_text(&mut rig, "Queued 0 thumbnails to render again");
}

#[test]
fn a_regenerate_is_refused_with_a_notice_while_a_purge_waits() {
    let mut rig = Rig::new(&[], &[], 9);
    rig.frame(Vec::new());
    let data_root = rig.dir.0.join("data");
    rig.app.thumbs.purge(data_root);
    let event = GridMenuEvent::RegenerateThumbnails(vec![path_of(&rig, "a.png")]);
    rig.app.apply_menu_event(&rig.ctx, event);
    wait_for_text(&mut rig, "Another task is waiting");
}

#[test]
fn a_copy_is_ignored_with_a_notice_while_another_copy_runs() {
    let mut rig = Rig::new(&[], &[], 9);
    let (release, wait) = crossbeam_channel::bounded::<()>(1);
    rig.app.start_copy(&rig.ctx, Vec::new(), move |_| {
        let _ = wait.recv_timeout(Duration::from_secs(3));
        Ok(())
    });
    // No paths, so a copy that ran by mistake would start no tool.
    rig.app
        .apply_menu_event(&rig.ctx, GridMenuEvent::Copy(Vec::new()));
    let _ = release.send(());
    wait_for_text(&mut rig, "Copy in progress");
}

#[test]
fn the_notice_names_how_many_thumbnails_were_queued() {
    assert_eq!(regenerate_text(Ok(1)), "Queued 1 thumbnail to render again");
    assert_eq!(
        regenerate_text(Ok(3)),
        "Queued 3 thumbnails to render again"
    );
    assert!(regenerate_text(Err("boom".to_owned())).contains("boom"));
}

#[test]
fn a_frame_carries_out_the_choice_the_gallery_raised() {
    let mut rig = Rig::new(&["a.png"], &[], 9);
    rig.click("a.png", false);
    let press = |pressed| Event::Key {
        key: Key::F2,
        physical_key: Some(Key::F2),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    rig.frame(vec![press(true)]);
    rig.frame(vec![press(false)]);
    wait_for_text(&mut rig, "Rename");
}
