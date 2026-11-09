//! Real PSD and PSB files rendered through the registry.
//!
//! The files are not in the repository. The `TARRAGON_PSD_CORPUS` environment
//! variable names the folder that `tests/psd-corpus/fetch.sh` fills. Without
//! the variable the test prints one skip line and passes. With the variable, a
//! missing listed file fails the test.

#[path = "../../tarragon-psd/tests/common/mod.rs"]
mod psd_builder;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use image::{DynamicImage, RgbaImage};
use psd_builder::{GOLDEN_TOLERANCE, assert_matches_golden};
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
    let Some(directory) = corpus_directory() else {
        return;
    };
    let names: Vec<&str> = FILES.iter().map(|(name, _)| *name).collect();
    require_files(&directory, &names);

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

/// Pairs of a corpus file and the reference PNG that can be trusted.
///
/// The references of `16bits.psd` and `32bits.psd` are blank white, so they
/// prove nothing. `cmyk.png` differs by colour conversion and has its own log
/// test.
const TRUSTED_REFERENCES: [(&str, &str); 4] = [
    ("psb.psb", "psb.png"),
    ("masks.psd", "masks.png"),
    ("adjustment-layers.psd", "adjustment-layers.png"),
    ("effects.psd", "effects.png"),
];

/// Premultiply the colour of every pixel by its alpha, in place.
///
/// The reference PNGs come from premultiplied 8-bit data, so their colour is
/// noise where alpha is small. Both sides are compared premultiplied.
fn premultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = u8::try_from((u32::from(*channel) * alpha + 127) / 255).unwrap_or(u8::MAX);
        }
    }
}

/// The premultiplied RGBA8 copy of an image.
fn premultiplied(image: &DynamicImage) -> RgbaImage {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut raw = rgba.into_raw();
    premultiply(&mut raw);
    RgbaImage::from_raw(width, height, raw).expect("the size is kept")
}

/// The corpus folder, or `None` after one skip line when the variable is unset.
fn corpus_directory() -> Option<PathBuf> {
    let Some(directory) = std::env::var_os("TARRAGON_PSD_CORPUS") else {
        println!("skip: TARRAGON_PSD_CORPUS is not set");
        return None;
    };
    Some(PathBuf::from(directory))
}

/// Fail with the fetch hint when a listed file is missing from the corpus folder.
fn require_files(directory: &Path, names: &[&str]) {
    let missing: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| !directory.join(name).is_file())
        .collect();
    assert!(
        missing.is_empty(),
        "missing in {}: {missing:?}; run tests/psd-corpus/fetch.sh to download the corpus",
        directory.display()
    );
}

/// Render a corpus file through the registry. A failure here is a test failure.
fn render_premultiplied(directory: &Path, name: &str) -> RgbaImage {
    let image = registry::render(&directory.join(name))
        .unwrap_or_else(|error| panic!("{name}: the render failed: {error}"));
    premultiplied(&image)
}

/// The largest colour-channel delta and the count of channels over `tolerance`.
///
/// Alpha is left out, as in `assert_matches_golden`.
fn channel_deltas(got: &[u8], want: &[u8], tolerance: u8) -> (u8, usize) {
    let mut max = 0u8;
    let mut over = 0usize;
    for (a, b) in got.as_chunks::<4>().0.iter().zip(want.as_chunks::<4>().0) {
        for (x, y) in a[..3].iter().zip(&b[..3]) {
            let delta = x.abs_diff(*y);
            max = max.max(delta);
            over += usize::from(delta > tolerance);
        }
    }
    (max, over)
}

/// Where a reference PNG can be trusted, the render matches it at the golden tolerance.
#[test]
fn the_trusted_references_match_the_render() {
    let Some(directory) = corpus_directory() else {
        return;
    };
    let names: Vec<&str> = TRUSTED_REFERENCES
        .iter()
        .flat_map(|(name, reference)| [*name, *reference])
        .collect();
    require_files(&directory, &names);
    for (name, reference_name) in TRUSTED_REFERENCES {
        let reference_path = directory.join(reference_name);
        let reference = image::open(&reference_path)
            .unwrap_or_else(|error| panic!("{}: {error}", reference_path.display()));
        let reference = premultiplied(&reference);
        let rendered = render_premultiplied(&directory, name);
        let (width, height) = rendered.dimensions();
        assert_matches_golden(
            rendered.as_raw(),
            width,
            height,
            &reference,
            GOLDEN_TOLERANCE,
            name,
        );
    }
}

/// The CMYK render differs from its reference by colour conversion. The test
/// logs the size of the gap and asserts only that the sizes agree.
#[test]
fn the_cmyk_render_is_compared_for_the_log() {
    let Some(directory) = corpus_directory() else {
        return;
    };
    require_files(&directory, &["cmyk.psd", "cmyk.png"]);
    let reference_path = directory.join("cmyk.png");
    let reference = image::open(&reference_path)
        .unwrap_or_else(|error| panic!("{}: {error}", reference_path.display()));
    let reference = premultiplied(&reference);
    let rendered = render_premultiplied(&directory, "cmyk.psd");
    assert_eq!(rendered.dimensions(), reference.dimensions(), "cmyk size");
    let (max, over) = channel_deltas(rendered.as_raw(), reference.as_raw(), GOLDEN_TOLERANCE);
    println!(
        "cmyk.psd: max delta {max}; {over} of {} channels over {GOLDEN_TOLERANCE}",
        rendered.as_raw().len()
    );
}

/// Pixels of `16bits.psd` measured with a reference decoder on 2026-11-09, as
/// canvas position and RGBA8. Alpha is 0 or 255, so premultiplied equals straight.
const SAMPLE_16BIT_PIXELS: [((u32, u32), [u8; 4]); 5] = [
    ((0, 0), [0, 0, 0, 0]),
    ((499, 499), [0, 0, 0, 0]),
    ((250, 250), [95, 39, 2, 255]),
    ((100, 100), [33, 35, 20, 255]),
    ((400, 300), [60, 70, 13, 255]),
];

/// The count of pixels with alpha above 0 in `16bits.psd`, from the same measurement.
const SAMPLE_16BIT_VISIBLE: usize = 180_972;

/// The real 16-bit file draws the measured pixels and about the measured area.
///
/// A blank-only check cannot catch wrong channel data, so the colours and the
/// covered area are pinned.
#[test]
fn the_16bit_sample_matches_the_measured_pixels() {
    let Some(directory) = corpus_directory() else {
        return;
    };
    require_files(&directory, &["16bits.psd"]);
    let rendered = render_premultiplied(&directory, "16bits.psd");
    let visible = rendered
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .count();
    assert!(visible > 0, "16bits.psd: every pixel has alpha 0");

    for ((x, y), want) in SAMPLE_16BIT_PIXELS {
        let got = rendered.get_pixel(x, y).0;
        assert_eq!(got[3], want[3], "16bits.psd ({x}, {y}): alpha {got:?}");
        for (channel, (g, w)) in got[..3].iter().zip(&want[..3]).enumerate() {
            assert!(
                g.abs_diff(*w) <= GOLDEN_TOLERANCE,
                "16bits.psd ({x}, {y}): channel {channel} is {g}, expected {w}; got {got:?}"
            );
        }
    }

    let slack = SAMPLE_16BIT_VISIBLE / 1000;
    assert!(
        visible.abs_diff(SAMPLE_16BIT_VISIBLE) <= slack,
        "16bits.psd: {visible} pixels have alpha above 0, expected {SAMPLE_16BIT_VISIBLE} within {slack}"
    );
}

/// `premultiply` and `channel_deltas` hold their rules with synthetic pixels.
#[test]
fn premultiply_and_channel_deltas_follow_their_rules() {
    let mut pixels = [200, 100, 50, 0, 200, 100, 50, 255, 255, 255, 255, 128];
    premultiply(&mut pixels);
    assert_eq!(pixels[..4], [0, 0, 0, 0], "alpha 0 clears the colour");
    assert_eq!(pixels[4..8], [200, 100, 50, 255], "alpha 255 keeps it");
    assert_eq!(pixels[8..], [128, 128, 128, 128], "alpha 128 halves it");

    let got = [10, 20, 30, 255, 0, 0, 0, 0];
    let want = [10, 23, 30, 0, 1, 0, 0, 255];
    assert_eq!(channel_deltas(&got, &want, 2), (3, 1), "alpha is ignored");
    assert_eq!(channel_deltas(&got, &want, 0), (3, 2));
}
