//! Tests that the card reads the newest pending report.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::newest_report;

/// A temporary folder that removes itself on drop, also after a failed assert.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tarragon-crashcard-test-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create the temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_newest_pending_report_wins_and_the_seen_one_is_skipped() {
    let dir = TempDir::new();
    std::fs::write(dir.0.join("crash-20260101-000001.txt"), "old\n").expect("write");
    std::fs::write(dir.0.join("crash-20260101-000002.txt"), "new\nsecond\n").expect("write");
    std::fs::write(dir.0.join("crash-20260101-000003.seen.txt"), "seen\n").expect("write");
    let (path, report) = newest_report(&dir.0).expect("a report");
    assert_eq!(path, dir.0.join("crash-20260101-000002.txt"));
    assert_eq!(report.file_name, "crash-20260101-000002.txt");
    assert_eq!(report.path, path.display().to_string());
    assert_eq!(report.summary, "new\nsecond");
}

#[test]
fn the_summary_stops_after_eight_lines() {
    let dir = TempDir::new();
    let text = ["line"; 12].join("\n");
    std::fs::write(dir.0.join("crash-20260101-000001.txt"), text).expect("write");
    let (_, report) = newest_report(&dir.0).expect("a report");
    assert_eq!(report.summary.lines().count(), 8);
}

#[test]
fn a_missing_or_empty_folder_gives_no_report() {
    let dir = TempDir::new();
    assert!(newest_report(&dir.0).is_none());
    assert!(newest_report(&dir.0.join("absent")).is_none());
}

#[test]
fn text_that_is_not_utf8_gives_the_name_and_an_empty_summary() {
    let dir = TempDir::new();
    std::fs::write(dir.0.join("crash-20260101-000001.txt"), [0xff, 0xfe, b'\n']).expect("write");
    let (_, report) = newest_report(&dir.0).expect("a report");
    assert_eq!(report.file_name, "crash-20260101-000001.txt");
    assert_eq!(report.summary, "");
}
