//! Tests of File, Open Folder and the cache folder Browse button. A fake
//! dialog answers, so no window opens; every frame is headless.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::Id;
use tarragon_db::settings::{SettingKey, SettingValue, Settings};

use super::super::preview_tests::Rig;
use super::open::{PICKER_ID, Shared};
use crate::app::{App, Startup};
use crate::logging::ring::LogRing;
use crate::picker::{FolderDialog, Picker};
use crate::thumbs::ExitWait;

/// How long a test waits for the dialog thread or an open before it fails.
const LIMIT: Duration = Duration::from_secs(10);

/// A dialog that answers at once with a fixed folder, or cancels.
struct FakeDialog(Option<PathBuf>);

impl FolderDialog for FakeDialog {
    fn pick(&self, _start: Option<&Path>) -> Option<PathBuf> {
        self.0.clone()
    }
}

/// Put a picker over a fake dialog into the context. The receiver gets one
/// signal each time the dialog has answered, so a test never guesses.
fn install(rig: &Rig, answer: Option<PathBuf>) -> Receiver<()> {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let ctx = rig.ctx.clone();
    let picker = Picker::with_dialog(Box::new(FakeDialog(answer)), move || {
        let _ = tx.lock().expect("signal lock").send(());
        ctx.request_repaint();
    });
    let shared: Shared = Arc::new(Mutex::new(picker));
    rig.ctx
        .data_mut(|data| data.insert_temp(Id::new(PICKER_ID), shared));
    rx
}

/// Wait for the dialog answer, then draw the frame that takes it.
fn take_answer(rig: &mut Rig, answered: &Receiver<()>) {
    answered.recv_timeout(LIMIT).expect("the dialog answers");
    rig.frame(Vec::new());
}

/// Draw frames until `done` holds, or fail after [`LIMIT`].
fn frames_until(rig: &mut Rig, what: &str, done: impl Fn(&mut App) -> bool) {
    let end = Instant::now() + LIMIT;
    while !done(&mut rig.app) {
        assert!(Instant::now() < end, "{what}");
        rig.frame(Vec::new());
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Make a folder with one picture in it.
fn make_folder(path: &Path) {
    std::fs::create_dir_all(path).expect("make the folder");
    image::RgbaImage::new(8, 8)
        .save(path.join("a.png"))
        .expect("write a png");
}

/// Choose File, Open Folder.
fn choose_open_folder(rig: &mut Rig) {
    rig.click("File", false);
    rig.click("Open Folder...", false);
}

fn roots_in_db(rig: &Rig) -> Vec<String> {
    rig.app.db.list_library_roots().expect("list the roots")
}

#[test]
fn a_folder_outside_home_is_stored_shown_and_opened() {
    let mut rig = Rig::new(&[], &[], 9);
    let art = rig.dir.0.join("art").join("sketches");
    make_folder(&art);
    let answered = install(&rig, Some(art.clone()));
    choose_open_folder(&mut rig);
    take_answer(&mut rig, &answered);
    assert_eq!(roots_in_db(&rig), [art.to_string_lossy()]);
    assert!(rig.app.shell.sidebar_mut().roots().contains(&art));
    frames_until(&mut rig, "the folder never opened", |app| {
        app.shell.sidebar_mut().open_folder() == Some(art.as_path())
    });
    let shown = rig.app.shell.gallery_mut().folder().map(Path::to_path_buf);
    assert_eq!(shown, art.canonicalize().ok());
}

#[test]
fn a_folder_under_home_adds_no_root() {
    let mut rig = Rig::new(&[], &[], 9);
    let home = rig.dir.0.join("home");
    let pics = home.join("pics");
    make_folder(&pics);
    rig.app.open_picked(&pics, Some(&home));
    assert_eq!(roots_in_db(&rig), Vec::<String>::new());
    frames_until(&mut rig, "the folder never opened", |app| {
        app.shell.sidebar_mut().open_folder() == Some(pics.as_path())
    });
}

#[test]
fn a_folder_under_a_saved_root_adds_nothing_and_opens() {
    let mut rig = Rig::new(&[], &[], 9);
    let art = rig.dir.0.join("art");
    let sub = art.join("sketches");
    make_folder(&sub);
    rig.app
        .db
        .add_library_root(&art.to_string_lossy())
        .expect("save the root");
    rig.app.open_picked(&sub, None);
    assert_eq!(roots_in_db(&rig), [art.to_string_lossy()]);
    frames_until(&mut rig, "the folder never opened", |app| {
        app.shell.sidebar_mut().open_folder() == Some(sub.as_path())
    });
}

#[test]
fn a_cancelled_open_folder_changes_nothing() {
    let mut rig = Rig::new(&[], &[], 9);
    let before = rig.app.shell.sidebar_mut().roots().to_vec();
    let answered = install(&rig, None);
    choose_open_folder(&mut rig);
    take_answer(&mut rig, &answered);
    assert_eq!(roots_in_db(&rig), Vec::<String>::new());
    assert_eq!(rig.app.shell.sidebar_mut().roots(), before);
    assert_eq!(rig.app.shell.sidebar_mut().open_folder(), None);
}

#[test]
fn the_saved_root_is_in_the_tree_after_a_restart() {
    let rig = Rig::new(&[], &[], 9);
    let art = rig.dir.0.join("art");
    std::fs::create_dir_all(&art).expect("make the root");
    rig.app
        .db
        .add_library_root(&art.to_string_lossy())
        .expect("save the root");
    let (_filter, control) = crate::logging::LogControl::new_filter();
    let startup = Startup {
        folder: None,
        close_when_loaded: false,
        exit_wait: ExitWait::default(),
        crash_dir: None,
    };
    let path = rig.dir.0.join("tarragon.db");
    let mut second = App::open(&path, LogRing::new(), control, None, startup).expect("reopen");
    assert!(second.shell.sidebar_mut().roots().contains(&art));
}

#[test]
fn the_cache_browse_answer_fills_the_field_and_saves_only_on_ok() {
    let mut rig = Rig::new(&[], &[], 9);
    let cache = rig.dir.0.join("cache");
    std::fs::create_dir_all(&cache).expect("make the folder");
    let values = crate::prefs::load(&rig.app.db);
    rig.app.shell.prefs_mut().open(values, "unknown".to_owned());
    for _ in 0..4 {
        rig.frame(Vec::new());
    }
    let answered = install(&rig, Some(cache.clone()));
    rig.click("Browse...", false);
    take_answer(&mut rig, &answered);
    let stored = |rig: &Rig| Settings::new(&rig.app.db).get(SettingKey::CacheDir);
    assert_eq!(stored(&rig).expect("read"), SettingValue::None);
    rig.click("OK", false);
    assert_eq!(
        stored(&rig).expect("read"),
        SettingValue::Text(cache.to_string_lossy().into_owned())
    );
}

#[test]
fn a_cancelled_cache_browse_leaves_the_field_alone() {
    let mut rig = Rig::new(&[], &[], 9);
    let values = crate::prefs::load(&rig.app.db);
    rig.app.shell.prefs_mut().open(values, "unknown".to_owned());
    for _ in 0..4 {
        rig.frame(Vec::new());
    }
    let answered = install(&rig, None);
    rig.click("Browse...", false);
    take_answer(&mut rig, &answered);
    rig.click("OK", false);
    let read = Settings::new(&rig.app.db).get(SettingKey::CacheDir);
    assert_eq!(read.expect("read"), SettingValue::None);
}
