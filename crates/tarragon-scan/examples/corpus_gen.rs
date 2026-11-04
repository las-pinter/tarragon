//! Write a folder of real images that stays on disk.
//!
//! Empty files do not decode, so a cold-cache scan and a real thumbnail run
//! need a folder of small valid images. File `i` is a 48 by 48 image whose
//! colour comes from `i`. Every fifth file is JPEG and the others are PNG.
//! Every 500th name is non-ASCII. With `--psd-file`, a share of the files are
//! copies of that PSD instead.
//!
//! ```text
//! cargo run --release --example corpus_gen -p tarragon-scan -- \
//!     --out DIR [--files 10000] [--psd-file FILE --psd-percent 10] [--help]
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use image::{Rgb, RgbImage};

/// Side of every image in pixels.
const SIDE: u32 = 48;
/// Every this many files the name is non-ASCII.
const CJK_EVERY: usize = 500;
/// Every this many files the image is JPEG.
const JPEG_EVERY: usize = 5;
/// Hue step in degrees. It is coprime to 360, so 360 files give 360 hues.
const HUE_STEP: usize = 37;

/// The text `--help` prints.
const USAGE: &str = "corpus_gen: write a folder of real small images

Options:
  --out DIR          folder to fill; it must be absent or empty (required)
  --files N          number of files (default 10000)
  --psd-file FILE    copy FILE for part of the files, named art<index>.psd
  --psd-percent N    share of PSD copies, 0 to 100 (default 10)
  --help             show this text";

/// Settings read from the command line.
#[derive(Debug, PartialEq, Eq)]
struct Options {
    /// Folder to fill.
    out: PathBuf,
    /// Number of files to write.
    files: usize,
    /// PSD file to copy, if any.
    psd_file: Option<PathBuf>,
    /// Share of files that are PSD copies, 0 to 100.
    psd_percent: usize,
}

/// What the command line asked for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Write the folder.
    Run(Options),
    /// Print the usage text.
    Help,
}

/// Read the options. A bad option gives a message for the caller to print.
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut out = None;
    let mut files = 10_000;
    let mut psd_file = None;
    let mut psd_percent = 10;
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            return Ok(Command::Help);
        }
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number = || {
            value
                .parse::<usize>()
                .map_err(|_| format!("{flag} needs a whole number, got {value}"))
        };
        match flag.as_str() {
            "--out" => out = Some(PathBuf::from(&value)),
            "--files" => files = number()?,
            "--psd-file" => psd_file = Some(PathBuf::from(&value)),
            "--psd-percent" => psd_percent = number()?.min(100),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    let out = out.ok_or_else(|| "--out is required".to_owned())?;
    Ok(Command::Run(Options {
        out,
        files,
        psd_file,
        psd_percent,
    }))
}

/// The file name of image `index`. Every CJK name ends in `.jpg`, because 500
/// is a multiple of 5.
fn name_for(index: usize) -> String {
    let extension = if index.is_multiple_of(JPEG_EVERY) {
        "jpg"
    } else {
        "png"
    };
    if index.is_multiple_of(CJK_EVERY) {
        format!("図書館_{index:05}.{extension}")
    } else {
        format!("img{index:05}.{extension}")
    }
}

/// The hue of image `index` in degrees, 0 to 359.
fn hue_for(index: usize) -> u32 {
    u32::try_from((index % 360) * HUE_STEP % 360).expect("a hue under 360 fits in u32")
}

/// The colour of image `index`: full saturation, hue from the index.
fn colour_for(index: usize) -> [u8; 3] {
    const VALUE: u32 = 200;
    let hue = hue_for(index);
    let rise = VALUE * (hue % 60) / 60;
    let fall = VALUE - rise;
    let (red, green, blue) = match hue / 60 {
        0 => (VALUE, rise, 0),
        1 => (fall, VALUE, 0),
        2 => (0, VALUE, rise),
        3 => (0, fall, VALUE),
        4 => (rise, 0, VALUE),
        _ => (VALUE, 0, fall),
    };
    [red, green, blue].map(|part| u8::try_from(part).expect("a channel under 256 fits in u8"))
}

/// Whether file `index` is a PSD copy. Over any run of files the count is
/// exactly `files * percent / 100`, spread evenly.
fn is_psd(index: usize, percent: usize) -> bool {
    (index * percent) / 100 != ((index + 1) * percent) / 100
}

/// Check that `out` is absent or an empty folder.
fn check_out(out: &Path) -> Result<(), String> {
    if !out.exists() {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(out)
        .map_err(|error| format!("{} is not a readable folder: {error}", out.display()))?;
    if entries.next().is_some() {
        return Err(format!("{} is not empty", out.display()));
    }
    Ok(())
}

/// What a run wrote.
struct Summary {
    /// File count for each extension.
    per_extension: BTreeMap<String, usize>,
    /// Summed size of all files in bytes.
    bytes: u64,
}

/// Write the folder. It checks every input before it writes anything.
fn generate(options: &Options) -> Result<Summary, String> {
    check_out(&options.out)?;
    if let Some(psd) = &options.psd_file
        && !psd.is_file()
    {
        return Err(format!("{} is not a file", psd.display()));
    }
    std::fs::create_dir_all(&options.out).expect("make the output folder");
    let mut summary = Summary {
        per_extension: BTreeMap::new(),
        bytes: 0,
    };
    for index in 0..options.files {
        let psd = options
            .psd_file
            .as_ref()
            .filter(|_| is_psd(index, options.psd_percent));
        let target = if let Some(source) = psd {
            let target = options.out.join(format!("art{index:05}.psd"));
            // A real copy, not a link: each file needs its own inode so a
            // cold-cache run reads every one from disk.
            std::fs::copy(source, &target).expect("copy the PSD file");
            target
        } else {
            let target = options.out.join(name_for(index));
            RgbImage::from_pixel(SIDE, SIDE, Rgb(colour_for(index)))
                .save(&target)
                .expect("write the image");
            target
        };
        let extension = target
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_owned();
        *summary.per_extension.entry(extension).or_default() += 1;
        summary.bytes += std::fs::metadata(&target)
            .expect("read the file size")
            .len();
    }
    Ok(summary)
}

/// Run the command line. Any refusal exits with code 2.
fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Run(options)) => match generate(&options) {
            Ok(summary) => {
                for (extension, count) in &summary.per_extension {
                    println!(".{extension}: {count}");
                }
                println!("folder size: {} bytes", summary.bytes);
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("corpus_gen: {message}");
                ExitCode::from(2)
            }
        },
        Err(message) => {
            eprintln!("corpus_gen: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// A temp folder name that removes itself on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        /// Make a fresh folder in the system temp folder.
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("tarragon-corpus-gen-{}-{id}", std::process::id()));
            std::fs::create_dir_all(&path).expect("make the temp folder");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Turn string slices into the owned arguments `parse_args` takes.
    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|&item| item.to_owned()).collect()
    }

    /// Options for an image-only run into `out`.
    fn options(out: &Path, files: usize) -> Options {
        Options {
            out: out.to_path_buf(),
            files,
            psd_file: None,
            psd_percent: 10,
        }
    }

    #[test]
    fn parse_reads_every_option() {
        let command = parse_args(args(&[
            "--out",
            "/x",
            "--files",
            "7",
            "--psd-file",
            "a.psd",
            "--psd-percent",
            "30",
        ]));
        assert_eq!(
            command,
            Ok(Command::Run(Options {
                out: PathBuf::from("/x"),
                files: 7,
                psd_file: Some(PathBuf::from("a.psd")),
                psd_percent: 30,
            }))
        );
    }

    #[test]
    fn parse_defaults_and_caps_percent() {
        let Ok(Command::Run(options)) = parse_args(args(&["--out", "/x"])) else {
            panic!("expected a run");
        };
        assert_eq!((options.files, options.psd_percent), (10_000, 10));
        let Ok(Command::Run(options)) = parse_args(args(&["--out", "/x", "--psd-percent", "900"]))
        else {
            panic!("expected a run");
        };
        assert_eq!(options.psd_percent, 100);
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(parse_args(args(&["--files", "5"])).is_err(), "no --out");
        assert!(parse_args(args(&["--out"])).is_err(), "no value");
        assert!(parse_args(args(&["--out", "/x", "--files", "x"])).is_err());
        assert!(parse_args(args(&["--out", "/x", "--nope", "1"])).is_err());
        assert_eq!(parse_args(args(&["--help"])), Ok(Command::Help));
    }

    #[test]
    fn names_are_cjk_every_500th_and_plain_otherwise() {
        for index in [0, 500, 1000] {
            assert!(name_for(index).starts_with("図書館_"), "{index}");
        }
        assert_eq!(name_for(1), "img00001.png");
        assert_eq!(name_for(5), "img00005.jpg");
    }

    #[test]
    fn colours_cover_many_hues() {
        let hues: BTreeSet<u32> = (0..100).map(hue_for).collect();
        assert!(hues.len() >= 12, "{}", hues.len());
        let colours: BTreeSet<[u8; 3]> = (0..100).map(colour_for).collect();
        assert!(colours.len() >= 12, "{}", colours.len());
    }

    #[test]
    fn psd_share_is_exact() {
        for (files, percent) in [(100, 10), (10_000, 10), (37, 50), (100, 0), (100, 100)] {
            let count = (0..files).filter(|&index| is_psd(index, percent)).count();
            assert_eq!(count, files * percent / 100, "{files} at {percent}");
        }
    }

    #[test]
    fn run_writes_readable_images_with_exact_counts() {
        let work = TempDir::new();
        let out = work.0.join("corpus");
        let summary = generate(&options(&out, 100)).expect("generate");
        assert_eq!(summary.per_extension["png"], 80);
        assert_eq!(summary.per_extension["jpg"], 20);
        let mut seen = 0;
        for entry in std::fs::read_dir(&out).expect("list the folder") {
            let image = image::open(entry.expect("entry").path()).expect("decode");
            assert_eq!((image.width(), image.height()), (SIDE, SIDE));
            seen += 1;
        }
        assert_eq!(seen, 100);
    }

    #[test]
    fn second_run_on_a_filled_folder_is_refused_and_writes_nothing() {
        let work = TempDir::new();
        let out = work.0.join("corpus");
        generate(&options(&out, 5)).expect("first run");
        let result = generate(&options(&out, 20));
        assert!(result.is_err());
        assert_eq!(std::fs::read_dir(&out).expect("list").count(), 5);
    }

    #[test]
    fn scan_finds_every_file_of_a_ten_thousand_file_run() {
        let work = TempDir::new();
        let out = work.0.join("corpus");
        generate(&options(&out, 10_000)).expect("generate");
        let files = tarragon_scan::scan::scan_folder(&out).expect("scan");
        assert_eq!(files.len(), 10_000);
    }

    #[test]
    fn an_empty_existing_folder_is_accepted() {
        let work = TempDir::new();
        assert!(generate(&options(&work.0, 3)).is_ok());
    }

    #[test]
    fn psd_copies_equal_the_source_bytes() {
        let work = TempDir::new();
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/psd/rgb_8bit.psd");
        let out = work.0.join("corpus");
        let mut settings = options(&out, 100);
        settings.psd_file = Some(source.clone());
        let summary = generate(&settings).expect("generate");
        assert_eq!(summary.per_extension["psd"], 10);
        let expected = std::fs::read(&source).expect("read the source");
        let mut copies = 0;
        for entry in std::fs::read_dir(&out).expect("list the folder") {
            let path = entry.expect("entry").path();
            if path.extension().is_some_and(|extension| extension == "psd") {
                assert_eq!(std::fs::read(&path).expect("read the copy"), expected);
                copies += 1;
            }
        }
        assert_eq!(copies, 10);
    }

    #[cfg(unix)]
    #[test]
    fn psd_copies_have_distinct_inodes() {
        use std::os::unix::fs::MetadataExt;

        let work = TempDir::new();
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/psd/rgb_8bit.psd");
        let out = work.0.join("corpus");
        let mut settings = options(&out, 100);
        settings.psd_file = Some(source);
        generate(&settings).expect("generate");
        let inodes: BTreeSet<u64> = std::fs::read_dir(&out)
            .expect("list the folder")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "psd"))
            .map(|path| std::fs::metadata(path).expect("metadata").ino())
            .collect();
        assert_eq!(inodes.len(), 10);
    }

    #[test]
    fn a_missing_psd_file_is_refused_before_any_write() {
        let work = TempDir::new();
        let out = work.0.join("corpus");
        let mut settings = options(&out, 10);
        settings.psd_file = Some(work.0.join("missing.psd"));
        assert!(generate(&settings).is_err());
        assert!(!out.exists());
    }
}
