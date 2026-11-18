//! Peak resident memory of a large scan that includes PSD and PSB files.
//!
//! The example builds a temp folder of many files, scans it, renders every
//! file through the registry on a small worker pool, and writes the three
//! cache tiers for each result. A gate keeps at most a few PSD and PSB
//! renders in flight at once, because those renders hold the most memory.
//! At the end it prints the peak resident size and compares it with 2 GiB.
//!
//! The PSD and PSB files are the committed fixtures, plus any file found in
//! the optional source folder and the optional large PSB. The other files are
//! small PNG and JPEG images made once and copied. The folder uses hard links
//! when it can, so a large corpus costs little disk.
//!
//! ```text
//! cargo run --release --example psd_rss_bench -p tarragon-render -- \
//!     [--files 10000] [--psd-percent 10] [--workers 4] [--psd-gate 3] \
//!     [--source-dir DIR] [--large-psb FILE] [--work-dir DIR] [--no-write] \
//!     [--help]
//! ```
//!
//! The peak comes from `/proc/self/status`, so it is only read on Linux. The
//! workspace forbids unsafe code, which rules out the system calls. Other
//! systems print `n/a` and the run passes on time and errors alone.
//!
//! The numeric casts are for a measurement harness, not for the library.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use rayon::prelude::*;
use tarragon_cache::store::CacheStore;
use tarragon_core::error::RenderError;
use tarragon_render::registry::render;
use tarragon_scan::scan::scan_folder;

/// The memory bound in bytes: 2 GiB.
const LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Bytes in one MiB.
const MIB: f64 = 1024.0 * 1024.0;

/// Settings read from the command line.
struct Options {
    files: usize,
    psd_percent: usize,
    workers: usize,
    psd_gate: usize,
    source_dir: Option<PathBuf>,
    large_psb: Option<PathBuf>,
    work_dir: Option<PathBuf>,
    write_cache: bool,
}

/// The text `--help` prints.
const USAGE: &str = "psd_rss_bench: peak memory of a scan with PSD and PSB files

Options:
  --files N          files in the temp folder (default 10000)
  --psd-percent N    share of PSD and PSB files, 0 to 100 (default 10)
  --workers N        worker threads (default 4)
  --psd-gate N       PSD and PSB renders in flight at once (default 3)
  --source-dir DIR   add the PSD and PSB files directly inside DIR
  --large-psb FILE   add one large PSB
  --work-dir DIR     parent folder for the temp folder (default: system temp)
  --no-write         render only, skip the cache tier writes
  --help             show this text";

/// What the command line asked for.
enum Command {
    Run(Options),
    Help,
}

impl Options {
    /// Read the options. A bad option gives a message for the caller to print.
    fn parse() -> Result<Command, String> {
        let mut options = Self {
            files: 10_000,
            psd_percent: 10,
            workers: 4,
            psd_gate: 3,
            source_dir: None,
            large_psb: None,
            work_dir: None,
            write_cache: true,
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--help" | "-h" => return Ok(Command::Help),
                "--no-write" => {
                    options.write_cache = false;
                    continue;
                }
                _ => {}
            }
            let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
            let number = || {
                value
                    .parse::<usize>()
                    .map_err(|_| format!("{flag} needs a whole number, got {value}"))
            };
            match flag.as_str() {
                "--files" => options.files = number()?,
                "--psd-percent" => options.psd_percent = number()?.min(100),
                "--workers" => options.workers = number()?.max(1),
                "--psd-gate" => options.psd_gate = number()?.max(1),
                "--source-dir" => options.source_dir = Some(PathBuf::from(&value)),
                "--large-psb" => options.large_psb = Some(PathBuf::from(&value)),
                "--work-dir" => options.work_dir = Some(PathBuf::from(&value)),
                _ => return Err(format!("unknown option {flag}")),
            }
        }
        Ok(Command::Run(options))
    }
}

/// A counting semaphore. It limits how many PSD renders run at once.
struct Gate {
    free: Mutex<usize>,
    wake: Condvar,
}

impl Gate {
    fn new(permits: usize) -> Self {
        Self {
            free: Mutex::new(permits),
            wake: Condvar::new(),
        }
    }

    /// Wait for a permit. The permit returns when the guard drops.
    fn enter(&self) -> Permit<'_> {
        let mut free = self.free.lock().unwrap_or_else(PoisonError::into_inner);
        while *free == 0 {
            free = self.wake.wait(free).unwrap_or_else(PoisonError::into_inner);
        }
        *free -= 1;
        Permit(self)
    }
}

/// One held permit of a [`Gate`].
struct Permit<'a>(&'a Gate);

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.0.wake.notify_one();
    }
}

/// Prefix of every folder this bench makes. A name is the prefix, the process
/// id, and a counter.
const FOLDER_PREFIX: &str = "tarragon-psd-rss-bench-";

/// A work folder that removes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    /// Make a fresh folder inside `parent`.
    fn new(parent: &Path) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!("{FOLDER_PREFIX}{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create the work folder");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// How old a bench folder must be before it can count as stale.
const STALE_AGE: Duration = Duration::from_hours(1);

/// Read `<pid>-<counter>` from the part of a folder name after the prefix.
fn parse_run_id(rest: &str) -> Option<u32> {
    let (pid, counter) = rest.split_once('-')?;
    let counter_value = counter.parse::<u64>().ok()?;
    let pid_value = pid.parse::<u32>().ok()?;
    // Only the exact form a run writes counts: no sign, no leading zeros.
    (format!("{pid_value}-{counter_value}") == rest).then_some(pid_value)
}

/// Remove folders an earlier run left behind in `parent` and return their
/// paths. Only a directory named `<prefix><pid>-<counter>` counts. A folder is
/// kept if its process still runs or if it changed in the last hour. Without
/// `/proc` every folder counts as alive and none is removed.
fn remove_stale_folders(parent: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix(FOLDER_PREFIX))
            .and_then(parse_run_id)
        else {
            continue;
        };
        // `file_type` does not follow a symlink, so a link is skipped.
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let recent = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_none_or(|age| age < STALE_AGE);
        let alive =
            Path::new("/proc").join(pid.to_string()).exists() || !Path::new("/proc").exists();
        if !recent && !alive && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed.push(entry.path());
        }
    }
    removed
}

/// The summed size of all files under `dir`, in bytes.
fn folder_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(metadata) if metadata.is_dir() => folder_size(&entry.path()),
            Ok(metadata) => metadata.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Shared counters the workers update.
struct Stats {
    render_nanos: AtomicU64,
    write_nanos: AtomicU64,
    first_write_error: Mutex<Option<String>>,
}

fn main() -> ExitCode {
    match Options::parse() {
        Ok(Command::Run(options)) => run(&options),
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("psd_rss_bench: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(options: &Options) -> ExitCode {
    let psd_sources = collect_psd_sources(options);
    if psd_sources.is_empty() && options.psd_percent > 0 {
        eprintln!("psd_rss_bench: no PSD or PSB source file found");
        return ExitCode::from(2);
    }

    println!("== tarragon psd rss benchmark ==");
    time_each_source(&psd_sources);

    let parent = options.work_dir.clone().unwrap_or_else(std::env::temp_dir);
    for path in remove_stale_folders(&parent) {
        println!("removed stale:    {}", path.display());
    }
    let work = TempDir::new(&parent);
    println!("work folder:      {} (removed on exit)", work.0.display());
    let library = work.0.join("library");
    std::fs::create_dir_all(&library).expect("make the library folder");
    let psd_count = options.files * options.psd_percent / 100;
    let copies = populate(&library, options.files, psd_count, &psd_sources);
    if copies > 0 {
        println!(
            "warning:          {copies} files were copied because a hard link failed; \
             the work folder needs more disk"
        );
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.workers)
        .build()
        .expect("build the worker pool");
    let started = Instant::now();
    let files = pool
        .install(|| scan_folder(&library))
        .expect("scan the folder");
    let scan_time = started.elapsed();

    let cache = CacheStore::new(work.0.join("cache"));
    let gate = Gate::new(options.psd_gate);
    let counters = Stats {
        render_nanos: AtomicU64::new(0),
        write_nanos: AtomicU64::new(0),
        first_write_error: Mutex::new(None),
    };
    let render_started = Instant::now();
    let outcomes: Vec<Outcome> = pool.install(|| {
        files
            .par_iter()
            .enumerate()
            .map(|(index, file)| {
                render_one(
                    &cache,
                    &gate,
                    &counters,
                    options.write_cache,
                    index,
                    &file.path,
                )
            })
            .collect()
    });
    let render_time = render_started.elapsed();
    let wall = started.elapsed();

    let mut formats: BTreeMap<String, usize> = BTreeMap::new();
    let mut errors: BTreeMap<&'static str, usize> = BTreeMap::new();
    for outcome in &outcomes {
        *formats.entry(outcome.extension.clone()).or_default() += 1;
        if let Some(kind) = outcome.error {
            *errors.entry(kind).or_default() += 1;
        }
    }
    let error_total: usize = errors.values().sum();

    println!("files requested:  {}", options.files);
    println!("files scanned:    {}", files.len());
    println!("files rendered:   {}", outcomes.len() - error_total);
    println!(
        "workers:          {} (PSD gate {})",
        options.workers, options.psd_gate
    );
    for (extension, count) in &formats {
        println!("  format .{extension}: {count}");
    }
    println!("errors:           {error_total}");
    for (kind, count) in &errors {
        println!("  {kind}: {count}");
    }
    println!("scan time:        {:.2} s", scan_time.as_secs_f64());
    print_timing(options.write_cache, render_time, &counters, &work.0);
    println!("wall time:        {:.2} s", wall.as_secs_f64());

    let status = ProcessStatus::read();
    println!("VmHWM (peak):     {}", mib_text(status.peak));
    println!("VmRSS (current):  {}", mib_text(status.current));
    let limit_mib = LIMIT_BYTES as f64 / MIB;
    let fail = error_total > 0 || status.peak.is_some_and(|peak| peak >= LIMIT_BYTES);
    println!(
        "limit:            {limit_mib:.0} MiB, result: {}",
        if fail { "FAIL" } else { "PASS" }
    );
    if fail {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Print the render time, the cache write time, and the cache size.
fn print_timing(write_cache: bool, render_time: Duration, counters: &Stats, work: &Path) {
    let worker_secs = |nanos: &AtomicU64| nanos.load(Ordering::Relaxed) as f64 / 1e9;
    println!("render+write:     {:.2} s", render_time.as_secs_f64());
    println!(
        "  render (summed over workers): {:.2} s",
        worker_secs(&counters.render_nanos)
    );
    if write_cache {
        println!(
            "  cache write (summed over workers): {:.2} s",
            worker_secs(&counters.write_nanos)
        );
        let cache_bytes = folder_size(&work.join("cache"));
        println!("cache size:       {:.1} MiB", cache_bytes as f64 / MIB);
    } else {
        println!("cache write:      skipped (--no-write)");
    }
    if let Some(text) = counters
        .first_write_error
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_deref()
    {
        println!("first cache write error: {text}");
    }
}

/// What one file gave: its extension and the error kind, if any.
struct Outcome {
    extension: String,
    error: Option<&'static str>,
}

/// Render one file and write its tiers. PSD and PSB files wait at the gate.
fn render_one(
    cache: &CacheStore,
    gate: &Gate,
    stats: &Stats,
    write_cache: bool,
    index: usize,
    path: &Path,
) -> Outcome {
    let extension = extension_of(path);
    let heavy = extension == "psd" || extension == "psb";
    let result = {
        let _permit = heavy.then(|| gate.enter());
        let started = Instant::now();
        let result = render(path);
        stats
            .render_nanos
            .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        result
    };
    let error = match result {
        Ok(_) if !write_cache => None,
        Ok(image) => {
            let name = path.file_name().and_then(|name| name.to_str());
            let started = Instant::now();
            let written = name.map_or_else(
                || Err("the file name is not UTF-8".to_owned()),
                |name| {
                    cache
                        .write_all(&image, "bench", &format!("{index:06}"), name)
                        .map_err(|error| error.to_string())
                },
            );
            stats
                .write_nanos
                .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
            match written {
                Ok(_) => None,
                Err(text) => {
                    stats
                        .first_write_error
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .get_or_insert(text);
                    Some("CacheWrite")
                }
            }
        }
        Err(error) => Some(error_kind(&error)),
    };
    Outcome { extension, error }
}

/// The variant name of a render error, for the error table.
fn error_kind(error: &RenderError) -> &'static str {
    match error {
        RenderError::UnsupportedFormat(_) => "UnsupportedFormat",
        RenderError::Io { .. } => "Io",
        RenderError::Decode { .. } => "Decode",
        RenderError::TooLarge { .. } => "TooLarge",
        RenderError::Panicked { .. } => "Panicked",
        RenderError::Timeout { .. } => "Timeout",
        RenderError::Cancelled { .. } => "Cancelled",
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// The PSD and PSB files the folder cycles through: the valid fixtures, the
/// files in the source folder, and the large PSB.
fn collect_psd_sources(options: &Options) -> Vec<PathBuf> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/psd");
    let mut found = Vec::new();
    gather(&fixtures, &mut found);
    // The truncated fixture is an error case. It would add a fixed error to
    // every run, so the bench leaves it out.
    found.retain(|path| path.file_name().is_none_or(|name| name != "truncated.psd"));
    if let Some(dir) = &options.source_dir {
        gather(dir, &mut found);
    }
    if let Some(large) = &options.large_psb {
        found.push(large.clone());
    }
    found.sort();
    found
}

/// Add the PSD and PSB files directly inside `dir` to `found`.
fn gather(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && matches!(extension_of(&path).as_str(), "psd" | "psb") {
            found.push(path);
        }
    }
}

/// Print the open-and-render time of each distinct source once. The slowest
/// layered file shows the cost of a document with many layers.
fn time_each_source(sources: &[PathBuf]) {
    let mut slowest: Option<(f64, &Path)> = None;
    for path in sources {
        let started = Instant::now();
        let ok = render(path).is_ok();
        let millis = started.elapsed().as_secs_f64() * 1000.0;
        if ok && slowest.is_none_or(|(best, _)| millis > best) {
            slowest = Some((millis, path));
        }
    }
    println!("sources:          {} distinct PSD/PSB files", sources.len());
    if let Some((millis, path)) = slowest {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?");
        println!("slowest single:   {millis:.1} ms ({name}, cold start of the process)");
    }
}

/// Fill `library` with `total` files. The first `psd_count` link to a PSD or
/// PSB source in turn. The rest are small PNG and JPEG files copied from two
/// images made once. It returns how many files were copied because a hard
/// link failed.
fn populate(library: &Path, total: usize, psd_count: usize, psd_sources: &[PathBuf]) -> usize {
    let png = library.join("seed.png");
    let jpg = library.join("seed.jpg");
    let (png_bytes, jpg_bytes) = (make_png(), make_jpeg());
    std::fs::write(&png, png_bytes).expect("write the seed png");
    std::fs::write(&jpg, jpg_bytes).expect("write the seed jpeg");
    let mut copies = 0;
    for index in 0..total {
        let target;
        let source: &Path = if index < psd_count {
            let from = &psd_sources[index % psd_sources.len()];
            let extension = extension_of(from);
            target = library.join(format!("doc{index:05}.{extension}"));
            from
        } else if index % 2 == 0 {
            target = library.join(format!("img{index:05}.png"));
            &png
        } else {
            target = library.join(format!("img{index:05}.jpg"));
            &jpg
        };
        if std::fs::hard_link(source, &target).is_err() {
            std::fs::copy(source, &target).expect("copy a source file");
            copies += 1;
        }
    }
    // The seeds are extra files in the folder. Remove them so the count is exact.
    let _ = (std::fs::remove_file(png), std::fs::remove_file(jpg));
    copies
}

/// A small PNG with a visible alpha gradient, so it writes PNG tiers.
fn make_png() -> Vec<u8> {
    let image = RgbaImage::from_fn(96, 64, |x, y| {
        Rgba([
            (x * 2) as u8,
            (y * 3) as u8,
            128,
            (100 + x + y).min(255) as u8,
        ])
    });
    encode(&DynamicImage::ImageRgba8(image), ImageFormat::Png)
}

/// A small opaque JPEG, so it writes JPEG tiers.
fn make_jpeg() -> Vec<u8> {
    let image = RgbaImage::from_fn(128, 80, |x, y| {
        Rgba([(x * 2) as u8, (y * 3) as u8, 64, 255])
    });
    encode(
        &DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(image).to_rgb8()),
        ImageFormat::Jpeg,
    )
}

fn encode(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).expect("encode the seed");
    bytes.into_inner()
}

/// Memory figures from `/proc/self/status`, in bytes.
struct ProcessStatus {
    peak: Option<u64>,
    current: Option<u64>,
}

impl ProcessStatus {
    fn read() -> Self {
        let text = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let field = |name: &str| -> Option<u64> {
            let line = text.lines().find(|line| line.starts_with(name))?;
            let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            Some(kib * 1024)
        };
        Self {
            peak: field("VmHWM:"),
            current: field("VmRSS:"),
        }
    }
}

fn mib_text(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || "n/a".to_owned(),
        |bytes| format!("{:.1} MiB", bytes as f64 / MIB),
    )
}
