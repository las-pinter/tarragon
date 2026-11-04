//! The crash card of the app: it shows the newest unseen report and acts on
//! its buttons.

use tarragon_ui::crash_card::CrashCardEvent;

use super::App;
use crate::editor;
use crate::logging::crash::{mark_crash_report_seen, pending_crash_reports};

impl App {
    /// Act on the crash card buttons. Dismiss marks every pending report as
    /// seen, so an older one does not show at a later start. Open report keeps
    /// the card open.
    pub(super) fn apply_crash_events(&mut self) {
        for event in self.shell.take_crash_events() {
            match event {
                CrashCardEvent::Dismiss => {
                    self.crash_path = None;
                    self.mark_reports_seen();
                }
                CrashCardEvent::OpenReport => {
                    let Some(path) = &self.crash_path else {
                        continue;
                    };
                    if let Err(error) = editor::open(&self.db, path) {
                        tracing::warn!("cannot open the crash report: {error}");
                    }
                }
            }
        }
    }

    /// Rename every pending report to its seen name.
    fn mark_reports_seen(&self) {
        let Some(dir) = &self.crash_dir else {
            return;
        };
        for path in pending_crash_reports(dir) {
            if let Err(error) = mark_crash_report_seen(&path) {
                tracing::warn!("cannot mark the crash report as seen: {error}");
            }
        }
    }
}
