//! Time a 10k-file folder scan with a four-thread `rayon` pool (task 8, Q10).
//!
//! The example makes a synthetic folder of 10,000 empty `.png` files, then
//! times:
//!
//! 1. [`scan_folder`] inside a pool capped at `min(4, cores)` threads.
//! 2. [`Database::write_scan_group`] for the same files (one transaction).
//!
//! `taskset` is not available in this environment, so the four-core pin is a
//! `rayon` pool cap. This is the same alternative the grid spike uses.
//!
//! ```text
//! cargo run --release --example scan_bench -p tarragon-scan
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tarragon_db::Database;
use tarragon_scan::scan::{build_pool, scan_folder};

/// Number of files in the synthetic folder.
const FILES: usize = 10_000;
/// The Q6 scan target in seconds.
const Q6_TARGET_SECS: f64 = 120.0;

fn main() {
    let work = TempDir::new();
    let folder = work.path().join("library");
    std::fs::create_dir_all(&folder).expect("make the library folder");
    populate(&folder, FILES);

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

    let total = scan_time + write_time;
    let total_secs = total.as_secs_f64();
    println!("== tarragon scan benchmark (study task 8) ==");
    println!("files in folder:  {FILES}");
    println!("files indexed:    {}", files.len());
    println!("rows written:     {written}");
    println!("rayon pool:       {threads} threads (Q10 four-core cap)");
    println!("scan elapsed:     {:.3} s", scan_time.as_secs_f64());
    println!("write elapsed:    {:.3} s", write_time.as_secs_f64());
    println!("total elapsed:    {total_secs:.3} s");
    println!("Q6 target:        < {Q6_TARGET_SECS:.0} s");
    println!(
        "result:           {}",
        if total_secs < Q6_TARGET_SECS {
            "PASS"
        } else {
            "FAIL"
        }
    );
    assert!(total_secs < Q6_TARGET_SECS, "the Q6 scan target is missed");
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
