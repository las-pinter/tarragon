//! Read the newest unseen crash report for the crash card.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use tarragon_ui::crash_card::CrashReport;

use crate::logging::crash::pending_crash_reports;

/// How many lines of a report the card shows: version, system, message, place.
const SUMMARY_LINES: usize = 8;

/// The newest pending report in `dir` with its path, or `None` when there is
/// none. A file that cannot be read still gives a report, with its name and an
/// empty summary, so the user learns that a crash happened.
pub fn newest_report(dir: &Path) -> Option<(PathBuf, CrashReport)> {
    let path = pending_crash_reports(dir).into_iter().next()?;
    let file_name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let summary = summary_of(&path).unwrap_or_else(|error| {
        tracing::debug!("cannot read the crash report {file_name}: {error}");
        String::new()
    });
    let report = CrashReport {
        file_name,
        path: path.display().to_string(),
        summary,
    };
    Some((path, report))
}

/// The first lines of the file at `path`, joined by line breaks.
fn summary_of(path: &Path) -> std::io::Result<String> {
    let file = std::fs::File::open(path)?;
    let lines = BufReader::new(file)
        .lines()
        .take(SUMMARY_LINES)
        .collect::<std::io::Result<Vec<_>>>()?;
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests;
