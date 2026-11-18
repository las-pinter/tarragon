//! Time a 10k-file folder scan with a four-thread `rayon` pool.
//!
//! The example makes a synthetic folder of 10,000 empty `.png` files, then
//! times:
//!
//! 1. [`scan_folder`] inside a pool capped at `min(4, logical CPUs)` threads.
//! 2. [`Database::write_scan_group`] for the same files (one transaction).
//!
//! `taskset` is not available in this environment, so the run caps the `rayon`
//! pool at four worker threads. This is the same alternative the grid example
//! uses.
//!
//! With `--folder DIR` the example scans DIR as it is and never deletes it.
//! A missing or non-directory DIR exits with code 2.
//!
//! ```text
//! cargo run --release --example scan_bench -p tarragon-scan [-- --folder DIR]
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tarragon_db::Database;
use tarragon_scan::scan::{build_pool, scan_folder};

/// Number of files in the synthetic folder.
const FILES: usize = 10_000;
/// The scan target in seconds.
const SCAN_TARGET_SECS: f64 = 120.0;

/// The text printed after a bad command line.
const USAGE: &str = "usage: scan_bench [--folder DIR]

  --folder DIR   scan DIR as it is and never delete it";

/// What one run measured.
struct Report {
    /// The folder that was scanned.
    folder: PathBuf,
    /// Whether the folder existed before the run.
    existing: bool,
    /// Files the scan returned.
    indexed: usize,
    /// Rows the database wrote.
    written: usize,
    /// Threads in the scan pool.
    threads: usize,
    /// Time of `scan_folder` alone.
    scan_time: Duration,
    /// Time of the database write.
    write_time: Duration,
}

/// Read the command line. The result is the folder to scan, if one was given.
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Option<PathBuf>, String> {
    let mut folder = None;
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if flag != "--folder" {
            return Err(format!("unknown option {flag}"));
        }
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        folder = Some(PathBuf::from(value));
    }
    Ok(folder)
}

/// Refuse a folder that is missing or is not a directory.
fn check_folder(folder: &Path) -> Result<(), String> {
    if folder.is_dir() {
        Ok(())
    } else {
        Err(format!("{} is not a folder", folder.display()))
    }
}

/// Scan `existing` as it is, or a generated folder when it is `None`. Only
/// the temp folder is removed at the end.
fn run(existing: Option<&Path>) -> Report {
    let work = TempDir::new();
    let folder = if let Some(path) = existing {
        path.to_path_buf()
    } else {
        let generated = work.path().join("library");
        std::fs::create_dir_all(&generated).expect("make the library folder");
        populate(&generated, FILES);
        generated
    };

    let pool = build_pool().expect("build the scan pool");
    let threads = pool.current_num_threads();
    let scan_started = Instant::now();
    let files = pool
        .install(|| scan_folder(&folder))
        .expect("scan the folder");
    let scan_time = scan_started.elapsed();

    let db_dir = work.path().join("db");
    std::fs::create_dir_all(&db_dir).expect("make the database folder");
    let db = Database::open(db_dir.join("tarragon.db")).expect("open the database");
    db.initialize().expect("create the schema");
    let write_started = Instant::now();
    let written = db.write_scan_group(&files).expect("write the scan group");
    let write_time = write_started.elapsed();

    Report {
        folder,
        existing: existing.is_some(),
        indexed: files.len(),
        written,
        threads,
        scan_time,
        write_time,
    }
}

/// Run the benchmark. A bad command line exits with code 2.
fn main() -> ExitCode {
    let existing = match parse_args(std::env::args().skip(1)) {
        Ok(existing) => existing,
        Err(message) => {
            eprintln!("scan_bench: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if let Some(path) = &existing
        && let Err(message) = check_folder(path)
    {
        eprintln!("scan_bench: {message}");
        return ExitCode::from(2);
    }

    let report = run(existing.as_deref());
    let total_secs = (report.scan_time + report.write_time).as_secs_f64();
    let cpus = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    println!("== tarragon scan benchmark ==");
    println!(
        "folder:           {} ({})",
        report.folder.display(),
        if report.existing {
            "existing"
        } else {
            "generated"
        }
    );
    println!("files indexed:    {}", report.indexed);
    println!("rows written:     {}", report.written);
    println!(
        "rayon pool:       {} threads (available parallelism {cpus})",
        report.threads
    );
    println!("scan elapsed:     {:.3} s", report.scan_time.as_secs_f64());
    println!("write elapsed:    {:.3} s", report.write_time.as_secs_f64());
    println!("total elapsed:    {total_secs:.3} s");
    println!("scan target:      < {SCAN_TARGET_SECS:.0} s");
    let pass = total_secs < SCAN_TARGET_SECS;
    println!("result:           {}", if pass { "PASS" } else { "FAIL" });
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Create `count` empty `.png` files in `folder`.
fn populate(folder: &Path, count: usize) {
    for index in 0..count {
        let path = folder.join(format!("img{index:05}.png"));
        std::fs::File::create(path).expect("create the file");
    }
}

/// A temporary folder that removes itself on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a unique temp folder.
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("tarragon-scan-bench-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create the temp folder");
        Self { path }
    }

    /// Return the folder path.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Turn string slices into the owned arguments `parse_args` takes.
    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|&item| item.to_owned()).collect()
    }

    #[test]
    fn parse_reads_no_argument_and_a_folder() {
        assert_eq!(parse_args(args(&[])), Ok(None));
        assert_eq!(
            parse_args(args(&["--folder", "/x"])),
            Ok(Some(PathBuf::from("/x")))
        );
    }

    #[test]
    fn parse_rejects_a_missing_value_and_an_unknown_option() {
        assert!(parse_args(args(&["--folder"])).is_err());
        assert!(parse_args(args(&["--nope", "1"])).is_err());
    }

    #[test]
    fn a_missing_or_plain_file_folder_is_refused() {
        let work = TempDir::new();
        assert!(check_folder(&work.path().join("absent")).is_err());
        let file = work.path().join("file.png");
        std::fs::File::create(&file).expect("create the file");
        assert!(check_folder(&file).is_err());
        assert!(check_folder(work.path()).is_ok());
        assert!(!work.path().join("absent").exists(), "nothing is created");
    }

    #[test]
    fn an_existing_folder_is_scanned_and_kept() {
        let work = TempDir::new();
        populate(work.path(), 25);
        let report = run(Some(work.path()));
        assert!(report.existing);
        assert_eq!(report.indexed, 25);
        assert_eq!(
            std::fs::read_dir(work.path()).expect("list").count(),
            25,
            "the folder keeps its files"
        );
    }

    #[test]
    fn the_default_run_generates_a_folder_and_removes_it() {
        let report = run(None);
        assert!(!report.existing);
        assert_eq!(report.indexed, FILES);
        assert!(!report.folder.exists(), "the temp folder is removed");
    }
}
