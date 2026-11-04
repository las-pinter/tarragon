//! Tests that the app shows the newest unseen crash report once and acts on
//! the card buttons. Every frame is headless.

use eframe::App as _;
use eframe::Frame;
use eframe::egui::Context;

use tarragon_ui::crash_card::CrashReport;

use super::tests::{TempDir, new_dir};
use super::*;
use crate::test_ui;

/// The title line of the card.
const TITLE: &str = "Tarragon closed unexpectedly";

const OLD: &str = "crash-20260101-000001.txt";
const NEW: &str = "crash-20260101-000002.txt";

struct Rig {
    dir: TempDir,
    app: App,
    ctx: Context,
    frame: Frame,
}

impl Rig {
    /// An app whose crash folder holds an older and a newer pending report.
    fn new() -> Self {
        let dir = new_dir();
        let crash = dir.0.join("crashes");
        std::fs::create_dir_all(&crash).expect("create the crash folder");
        std::fs::write(crash.join(OLD), "old version\n").expect("write the old report");
        std::fs::write(crash.join(NEW), "new version\nboom\n").expect("write the new report");
        let crash = Some(crash);
        Self::start(dir, crash)
    }

    /// Open an app on `dir` with `crash_dir` as the crash folder.
    fn start(dir: TempDir, crash_dir: Option<PathBuf>) -> Self {
        let (_filter, control) = LogControl::new_filter();
        let startup = Startup {
            folder: None,
            close_when_loaded: false,
            exit_wait: ExitWait::default(),
            crash_dir,
        };
        let app = App::open(&dir.0.join("t.db"), LogRing::new(), control, None, startup)
            .expect("open the app");
        let ctx = Context::default();
        tarragon_ui::install(&ctx);
        tarragon_ui::apply_theme(&ctx);
        let mut rig = Self {
            dir,
            app,
            ctx,
            frame: Frame::_new_kittest(),
        };
        // A window is hidden on its first frame, while egui measures it.
        rig.texts();
        rig
    }

    /// Close the app without a Dismiss and open another on the same folders.
    fn restart(self) -> Self {
        let Self { dir, app, .. } = self;
        drop(app);
        let crash = Some(dir.0.join("crashes"));
        Self::start(dir, crash)
    }

    fn texts(&mut self) -> Vec<String> {
        let (app, frame) = (&mut self.app, &mut self.frame);
        let painted = test_ui::frame(&self.ctx, Vec::new(), &mut |ui| app.ui(ui, frame));
        painted.texts.into_iter().map(|t| t.0).collect()
    }

    fn card_shown(&mut self) -> bool {
        self.texts().iter().any(|t| t == TITLE)
    }

    fn press(&mut self, label: &str) {
        let (app, frame) = (&mut self.app, &mut self.frame);
        test_ui::click_text(&self.ctx, label, &mut |ui| app.ui(ui, frame));
    }

    fn crashes(&self, name: &str) -> PathBuf {
        self.dir.0.join("crashes").join(name)
    }
}

#[test]
fn the_card_shows_the_newer_report_with_its_first_line() {
    let mut rig = Rig::new();
    let texts = rig.texts();
    assert!(texts.iter().any(|t| t == TITLE));
    assert!(texts.iter().any(|t| t.starts_with("new version")));
    assert!(!texts.iter().any(|t| t.contains("old version")));
    assert_eq!(rig.app.crash_path, Some(rig.crashes(NEW)));
}

#[test]
fn dismiss_marks_every_report_seen_and_the_next_start_shows_no_card() {
    let mut rig = Rig::new();
    rig.press("Dismiss");
    assert!(!rig.card_shown());
    assert!(rig.app.crash_path.is_none());
    for name in [OLD, NEW] {
        assert!(!rig.crashes(name).exists(), "{name} was renamed");
    }
    assert!(rig.crashes("crash-20260101-000001.seen.txt").exists());
    assert!(rig.crashes("crash-20260101-000002.seen.txt").exists());
    let mut rig = rig.restart();
    assert!(!rig.card_shown());
}

#[test]
fn closing_without_dismiss_shows_the_card_again() {
    let mut rig = Rig::new();
    assert!(rig.card_shown());
    let mut rig = rig.restart();
    assert!(rig.card_shown());
    assert!(rig.crashes(NEW).exists());
}

#[test]
fn no_crash_folder_or_an_empty_one_shows_no_card() {
    let dir = new_dir();
    let crash = Some(dir.0.join("crashes"));
    let mut rig = Rig::start(dir, crash);
    assert!(!rig.card_shown(), "the folder is missing");
    std::fs::create_dir_all(rig.crashes("")).expect("create the crash folder");
    let mut rig = rig.restart();
    assert!(!rig.card_shown(), "the folder is empty");
    assert!(rig.app.crash_path.is_none());
}

#[test]
fn dismiss_without_a_crash_folder_closes_the_card() {
    let mut rig = Rig::start(new_dir(), None);
    let report = CrashReport {
        file_name: "crash-1.txt".into(),
        path: "crash-1.txt".into(),
        summary: "one".into(),
    };
    rig.app.shell.show_crash_report(report);
    rig.texts();
    assert!(rig.card_shown());
    rig.press("Dismiss");
    assert!(!rig.card_shown());
}

#[cfg(unix)]
#[test]
fn a_report_that_cannot_be_renamed_does_not_keep_the_card_open() {
    let mut rig = Rig::new();
    // A non-empty folder at the seen name makes the rename of the older report fail.
    let blocker = rig.crashes("crash-20260101-000001.seen.txt");
    std::fs::create_dir_all(&blocker).expect("create the blocker");
    std::fs::write(blocker.join("x"), "x").expect("fill the blocker");
    rig.press("Dismiss");
    assert!(!rig.card_shown());
    assert!(rig.crashes(OLD).exists(), "the older report stays");
    assert!(rig.crashes("crash-20260101-000002.seen.txt").exists());
}

#[cfg(unix)]
#[test]
fn open_report_runs_the_editor_for_the_file_and_keeps_the_card_open() {
    let mut rig = Rig::new();
    let out = rig.dir.0.join("opened.txt");
    let template = format!("sh -c \"echo $0 > {}\" {{file}}", out.display());
    rig.app
        .db
        .upsert_editor_association("txt", &template)
        .expect("store the template");
    rig.press("Open report");
    let want = format!("{}\n", rig.crashes(NEW).display());
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_to_string(&out).map_or(true, |text| text != want) {
        assert!(std::time::Instant::now() < end, "timed out: editor ran");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(rig.card_shown());
    assert!(rig.crashes(NEW).exists());
}
