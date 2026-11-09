//! Real PSD and PSB files rendered through the registry.
//!
//! The files are not in the repository. The `TARRAGON_PSD_CORPUS` environment
//! variable names the folder that `tests/psd-corpus/fetch.sh` fills. Without
//! the variable the test prints one skip line and passes. With the variable, a
//! missing listed file fails the test.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tarragon_core::error::RenderError;
use tarragon_psd::limits::PARSE_DEADLINE_SECS;
use tarragon_psd::{ParseOptions, PsdDocument};
use tarragon_render::registry;

/// The deadline of the default parse options. The registry takes no options,
/// so the test measures the elapsed time against the same limit.
const DEADLINE: Duration = Duration::from_secs(PARSE_DEADLINE_SECS);

/// What the registry must return for a corpus file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// An image with the size of the file header.
    Renders,
    /// A typed decode error, not a panic.
    DecodeError,
}

/// The corpus PSD and PSB files with the expected outcome of each.
const FILES: [(&str, Outcome); 9] = [
    ("16bits.psd", Outcome::Renders),
    ("32bits.psd", Outcome::DecodeError),
    ("cmyk.psd", Outcome::Renders),
    ("psb.psb", Outcome::Renders),
    ("adjustment-layers.psd", Outcome::Renders),
    ("effects.psd", Outcome::Renders),
    ("vector_mask.psd", Outcome::Renders),
    ("masks.psd", Outcome::Renders),
    ("pattern-fill.psd", Outcome::Renders),
];

/// The raw scan result for the layer-info keys that carry 16 and 32 bit data.
///
/// The parser does not expose which key a file uses, so the bytes are searched.
fn raw_scan(bytes: &[u8]) -> String {
    let has = |key: &[u8]| bytes.windows(key.len()).any(|window| window == key);
    let mut found = Vec::new();
    if has(b"8BIMLr16") || has(b"8B64Lr16") {
        found.push("Lr16");
    }
    if has(b"8BIMLr32") || has(b"8B64Lr32") {
        found.push("Lr32");
    }
    if found.is_empty() {
        "none".to_owned()
    } else {
        found.join(" and ")
    }
}

/// Decide whether one render broke a rule. The result carries the image size
/// on success. A pure function, so synthetic values can prove each rule.
fn judge(
    name: &str,
    expected: Outcome,
    result: &Result<(u32, u32), RenderError>,
    elapsed: Duration,
    expected_size: (u32, u32),
) -> Option<String> {
    if elapsed >= DEADLINE {
        return Some(format!(
            "{name}: took {elapsed:?}, the limit is {DEADLINE:?}"
        ));
    }
    match (expected, result) {
        (Outcome::Renders, Ok(actual_size)) => (*actual_size != expected_size).then(|| {
            format!(
                "{name}: image is {}x{}, the header says {}x{}",
                actual_size.0, actual_size.1, expected_size.0, expected_size.1
            )
        }),
        (Outcome::DecodeError, Err(RenderError::Decode { .. })) => None,
        (expected, Ok(_)) => Some(format!("{name}: expected {expected:?}, got an image")),
        (expected, Err(error)) => Some(format!("{name}: expected {expected:?}, got {error}")),
    }
}

/// Render one file, print its line, and return the failure text if it broke a rule.
fn check_file(directory: &Path, name: &str, expected: Outcome) -> Option<String> {
    let path = directory.join(name);
    let bytes = std::fs::read(&path).expect("the corpus file reads");
    let document = PsdDocument::open_lenient(&path, &ParseOptions::default())
        .expect("the corpus file has a readable header");
    let file = document.file();
    let header = &file.header;

    let started = Instant::now();
    let result = registry::render(&path).map(|image| (image.width(), image.height()));
    let elapsed = started.elapsed();

    let actual = match &result {
        Ok(_) => "renders".to_owned(),
        Err(RenderError::Decode { .. }) => "decode error".to_owned(),
        Err(other) => format!("other error: {other}"),
    };
    println!(
        "{name}: {}x{} depth {} {:?} has_real_merged_data {:?} raw byte scan: {} outcome {actual} {} ms",
        header.width,
        header.height,
        header.depth,
        header.color_mode,
        file.has_real_merged_data,
        raw_scan(&bytes),
        elapsed.as_millis(),
    );

    judge(
        name,
        expected,
        &result,
        elapsed,
        (header.width, header.height),
    )
}

/// Render every corpus file and check the outcome, the size and the deadline.
#[test]
fn every_corpus_file_renders_inside_the_deadline_without_a_panic() {
    let Some(directory) = std::env::var_os("TARRAGON_PSD_CORPUS") else {
        println!("skip: TARRAGON_PSD_CORPUS is not set");
        return;
    };
    let directory = PathBuf::from(directory);

    let missing: Vec<&str> = FILES
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !directory.join(name).is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "missing in {}: {missing:?}; run tests/psd-corpus/fetch.sh to download the corpus",
        directory.display()
    );

    let failures: Vec<String> = FILES
        .iter()
        .filter_map(|(name, expected)| check_file(&directory, name, *expected))
        .collect();
    assert!(
        failures.is_empty(),
        "corpus failures:\n{}",
        failures.join("\n")
    );
}

/// Each rule of `judge` holds with synthetic values, so CI proves them with no corpus.
#[test]
fn judge_fails_each_broken_rule_and_passes_a_good_render() {
    let path = PathBuf::from("a.psd");
    let fast = Duration::from_millis(5);
    let size = (4, 3);
    let renders = |result: Result<(u32, u32), RenderError>, elapsed| {
        judge("a.psd", Outcome::Renders, &result, elapsed, size)
    };

    assert_eq!(renders(Ok(size), fast), None);
    assert!(renders(Ok(size), DEADLINE).is_some(), "the deadline fails");
    assert!(renders(Ok((5, 3)), fast).is_some(), "a wrong size fails");
    let panicked = RenderError::Panicked {
        path: path.clone(),
        message: "boom".to_owned(),
    };
    assert!(renders(Err(panicked), fast).is_some(), "a panic fails");
    let timeout = RenderError::Timeout { path: path.clone() };
    assert!(renders(Err(timeout), fast).is_some(), "a timeout fails");
    let decode = RenderError::Decode {
        path: path.clone(),
        reason: "x".to_owned(),
    };
    assert!(renders(Err(decode), fast).is_some(), "a decode error fails");

    let expect_error = |result: Result<(u32, u32), RenderError>| {
        judge("a.psd", Outcome::DecodeError, &result, fast, size)
    };
    let decode = RenderError::Decode {
        path: path.clone(),
        reason: "x".to_owned(),
    };
    assert_eq!(expect_error(Err(decode)), None);
    assert!(expect_error(Ok(size)).is_some(), "an image fails");
    let panicked = RenderError::Panicked {
        path,
        message: "boom".to_owned(),
    };
    assert!(expect_error(Err(panicked)).is_some(), "a panic fails");
}
