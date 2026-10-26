//! Proof that a document is read by file range and never whole.
//!
//! These tests count the reads the source serves. They live in the crate
//! because the counters exist only for unit tests.

#[path = "../tests/common/builder.rs"]
#[allow(dead_code, missing_docs)]
mod builder;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use builder::{DocumentSpec, LayerSpec, build_psd, push_u16, push_u32};

use crate::document::PsdDocument;
use crate::error::PsdError;
use crate::limits::ParseOptions;
use crate::parse::parse_with;
use crate::source::Source;
use crate::{Viewport, composite};

/// Gives every temp file its own directory, so tests that run on parallel
/// threads never share or remove one another's directory.
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

/// A temp file that is removed when it goes out of scope.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str, bytes: &[u8]) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "tarragon-range-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write temp file");
        Self(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
        if let Some(dir) = self.0.parent() {
            std::fs::remove_dir(dir).ok();
        }
    }
}

const EDGE: u32 = 2048;

/// A square RGB document with one full-canvas raw layer and a raw merged image.
fn large_document() -> Vec<u8> {
    let mut spec = DocumentSpec::new(EDGE, EDGE, 8, 3);
    let edge = i32::try_from(EDGE).expect("edge");
    spec.layers
        .push(LayerSpec::opaque((0, 0, edge, edge), [200, 60, 60]));
    build_psd(&spec)
}

#[test]
fn opening_and_rendering_a_large_document_reads_a_small_part_of_the_file() {
    let bytes = large_document();
    let size = bytes.len() as u64;
    let file = TempFile::new("large.psd", &bytes);
    drop(bytes);

    let document = PsdDocument::open(&file.0).expect("opens");
    let opened = document.source().stats();
    assert!(
        opened.bytes < 128 * 1024,
        "open read {} of {size} bytes",
        opened.bytes
    );

    let region = document
        .composite_region(Viewport::new(0, 0, 64, 64), &ParseOptions::none())
        .expect("composites");
    assert_eq!(region.width(), 64);
    let rendered = document.source().stats();
    assert!(
        rendered.bytes < size / 20,
        "a 64 px tile read {} of {size} bytes",
        rendered.bytes
    );
    // No single read is larger than one channel row or one read-ahead block.
    assert!(
        rendered.largest <= 64 * 1024,
        "largest read {}",
        rendered.largest
    );
}

#[test]
fn a_stepped_merged_read_reads_only_the_kept_rows() {
    let mut spec = DocumentSpec::new(EDGE, EDGE, 8, 3);
    spec.merged = Vec::new();
    let bytes = build_psd(&spec);
    let file = TempFile::new("merged.psd", &bytes);
    let document = PsdDocument::open(&file.0).expect("opens");

    let before = document.source().stats();
    let step = 8u32;
    let region = document
        .read_merged_strided(Viewport::full(EDGE, EDGE), step, &ParseOptions::none())
        .expect("reads");
    assert_eq!(region.viewport.height(), EDGE / step);
    let after = document.source().stats();
    let kept = u64::from(EDGE / step) * 3;
    assert_eq!(after.reads - before.reads, kept, "one read per kept row");
    assert_eq!(
        after.bytes - before.bytes,
        kept * u64::from(EDGE),
        "a kept row is read whole, a skipped row not at all"
    );
}

/// A merged section of three RLE planes. Every row is a run of one value.
fn rle_merged_section(edge: u32) -> Vec<u8> {
    let runs = edge / 128;
    let row_length = u16::try_from(runs * 2).expect("row length");
    let mut section = Vec::new();
    push_u16(&mut section, 1);
    for _ in 0..3 * edge {
        push_u16(&mut section, row_length);
    }
    for plane in 0..3u8 {
        for _ in 0..edge {
            for _ in 0..runs {
                // Control -127 repeats the next byte 128 times.
                section.extend_from_slice(&[0x81, plane + 10]);
            }
        }
    }
    section
}

#[test]
fn a_stepped_rle_read_reads_one_table_per_channel_and_the_kept_rows() {
    let mut spec = DocumentSpec::new(EDGE, EDGE, 8, 3);
    spec.merged_section = Some(rle_merged_section(EDGE));
    let bytes = build_psd(&spec);
    let file = TempFile::new("rle.psd", &bytes);
    let document = PsdDocument::open(&file.0).expect("opens");
    // The open reads each channel's row table once and nothing else of the
    // pixel data.
    let opened = document.source().stats();
    assert!(opened.bytes < 64 * 1024 + 3 * u64::from(EDGE) * 2);

    let step = 8u32;
    let region = document
        .read_merged_strided(Viewport::full(EDGE, EDGE), step, &ParseOptions::none())
        .expect("reads");
    assert_eq!(region.sample(1, 5, 5), Some(11));
    let after = document.source().stats();
    let kept = u64::from(EDGE / step) * 3;
    let table_reads = 3;
    assert_eq!(after.reads - opened.reads, kept + table_reads);
    let row = u64::from(EDGE / 128) * 2;
    assert_eq!(
        after.bytes - opened.bytes,
        kept * row + table_reads * u64::from(EDGE) * 2
    );
}

/// Every `.rs` file under `dir`, including subdirectories.
fn rust_files(dir: &std::path::Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn no_source_file_reads_a_whole_file_at_once() {
    // The needles are built in two parts so this file does not match itself.
    let needles = [
        ["fs::", "read("].concat(),
        ["fs::", "read_to_string"].concat(),
        ["read_to", "_end"].concat(),
    ];
    let mut files = Vec::new();
    rust_files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() > 10, "the scan found {} files", files.len());
    for path in files {
        if path.ends_with("range_reads_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("source file");
        for needle in &needles {
            assert!(
                !text.contains(needle),
                "{} reads a whole file ({needle})",
                path.display()
            );
        }
    }
}

/// A small layered file in several layouts, to cut at every length.
fn sweep_documents() -> Vec<Vec<u8>> {
    let mut documents = Vec::new();
    for psb in [false, true] {
        let mut plain = DocumentSpec::new(6, 4, 8, 3);
        plain.psb = psb;
        plain
            .layers
            .push(LayerSpec::opaque((0, 0, 4, 6), [10, 20, 30]));
        documents.push(build_psd(&plain));

        let mut wide = DocumentSpec::new(6, 4, 16, 3);
        wide.psb = psb;
        wide.layer_info_block = Some(*b"Lr16");
        wide.layers
            .push(LayerSpec::opaque((0, 0, 4, 6), [1000, 2000, 3000]));
        documents.push(build_psd(&wide));
    }
    let mut rle = DocumentSpec::new(128, 128, 8, 3);
    rle.merged_section = Some(rle_merged_section(128));
    documents.push(build_psd(&rle));
    documents
}

/// The same outcome from a file and from memory, for one prefix.
fn assert_same_outcome(prefix: &[u8], lenient: bool, file: &TempFile) {
    let from_file = Source::open(&file.0)
        .and_then(|source| parse_with(&source, &ParseOptions::none(), lenient));
    let from_memory = parse_with(
        &Source::memory(prefix.to_vec()),
        &ParseOptions::none(),
        lenient,
    );
    match (from_file, from_memory) {
        (Ok(a), Ok(b)) => {
            assert_eq!(a.layer_count, b.layer_count, "length {}", prefix.len());
            assert_eq!(a.merged.channels.len(), b.merged.channels.len());
        }
        (Err(a), Err(b)) => {
            assert!(!matches!(a, PsdError::Io { .. }), "length {}", prefix.len());
            assert_eq!(a.to_string(), b.to_string(), "length {}", prefix.len());
        }
        (a, b) => panic!(
            "length {}: file {:?}, memory {:?}",
            prefix.len(),
            a.map(|_| ()),
            b.map(|_| ())
        ),
    }
}

#[test]
fn every_truncation_gives_the_same_typed_error_from_a_file_and_from_memory() {
    for full in sweep_documents() {
        for length in 0..=full.len() {
            let file = TempFile::new("cut.psd", &full[..length]);
            assert_same_outcome(&full[..length], false, &file);
            assert_same_outcome(&full[..length], true, &file);
        }
    }
}

#[test]
fn concurrent_renders_of_one_file_document_agree() {
    let bytes = large_document();
    let file = TempFile::new("shared.psd", &bytes);
    drop(bytes);
    let document = std::sync::Arc::new(PsdDocument::open(&file.0).expect("opens"));
    let view = Viewport::new(100, 100, 228, 228);
    let outputs: Vec<Vec<u8>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let document = std::sync::Arc::clone(&document);
                scope.spawn(move || {
                    let region: composite::CompositeRegion = document
                        .composite_region(view, &ParseOptions::none())
                        .expect("composites");
                    region.rgba8()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect()
    });
    assert!(outputs.iter().all(|output| *output == outputs[0]));
    assert_eq!(&outputs[0][..4], &[200, 60, 60, 255]);
}

fn angle_bytes(degrees: i32) -> Vec<u8> {
    degrees.to_be_bytes().to_vec()
}

#[test]
fn a_large_resource_block_before_the_global_light_is_never_read() {
    let mut spec = DocumentSpec::new(64, 64, 8, 3);
    // An odd length, so the walk also crosses a pad byte.
    spec.resources = vec![
        (2000, vec![7u8; 8 * 1024 * 1024 + 1]),
        (1037, angle_bytes(120)),
        (1038, angle_bytes(35)),
    ];
    let file = TempFile::new("resources.psd", &build_psd(&spec));
    let document = PsdDocument::open(&file.0).expect("opens");
    let light = document.file().global_light;
    assert!((light.angle - 120.0).abs() < f32::EPSILON);
    assert!((light.altitude - 35.0).abs() < f32::EPSILON);
    let opened = document.source().stats();
    assert!(
        opened.bytes < 256 * 1024,
        "open read {} bytes past an 8 MiB resource",
        opened.bytes
    );
}

#[test]
fn a_bad_resource_section_keeps_the_values_read_before_it() {
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.resources = vec![(1037, angle_bytes(90))];
    let mut bytes = build_psd(&spec);
    // Add a block head that declares 1000 data bytes the section lacks.
    let junk: &[u8] = b"8BIM\x03\xff\0\0\0\0\x03\xe8";
    let section = 26 + 4;
    let length = u32::from_be_bytes(bytes[section..section + 4].try_into().expect("length"));
    let tail = section + 4 + length as usize;
    let grown = length + u32::try_from(junk.len()).expect("junk length");
    bytes[section..section + 4].copy_from_slice(&grown.to_be_bytes());
    bytes.splice(tail..tail, junk.iter().copied());
    let document = PsdDocument::from_bytes(bytes, &ParseOptions::none()).expect("opens");
    assert!((document.file().global_light.angle - 90.0).abs() < f32::EPSILON);
}

#[test]
fn a_pattern_table_over_the_limit_leaves_a_note() {
    use crate::layer_tail::{TaggedTail, read_tagged_tail};
    use crate::window::Window;

    let plain = PsdDocument::from_bytes(
        build_psd(&DocumentSpec::new(8, 8, 8, 3)),
        &ParseOptions::none(),
    )
    .expect("opens");
    assert_eq!(plain.pattern_note(), None);

    // A tiny limit runs the skip path without a huge table.
    let mut bytes = b"8BIMPatt".to_vec();
    push_u32(&mut bytes, 64);
    bytes.resize(bytes.len() + 64, 0);
    let source = Source::memory(bytes);
    let mut window = Window::new(&source, 0, source.len());
    let mut tail = TaggedTail::default();
    read_tagged_tail(
        &mut window,
        &plain.file().header,
        &ParseOptions::none(),
        false,
        16,
        &mut tail,
    )
    .expect("walks");
    assert_eq!(tail.patterns.len(), 0);
    let note = tail.pattern_note.expect("a note");
    assert!(note.contains("skipped"), "{note}");
}

#[test]
fn a_record_that_claims_too_much_extra_data_is_refused_before_a_read() {
    let mut spec = DocumentSpec::new(8, 8, 8, 3);
    spec.layers.push(LayerSpec::opaque((0, 0, 8, 8), [1, 2, 3]));
    let mut bytes = build_psd(&spec);
    // The blend signature and key start the fixed part; the extra length
    // follows the opacity, clipping, flags and filler bytes.
    let at = bytes
        .windows(8)
        .position(|window| window == b"8BIMnorm")
        .expect("blend signature");
    let claimed = u32::try_from(crate::source::MAX_RECORD_EXTRA).expect("cap") + 1;
    bytes[at + 12..at + 16].copy_from_slice(&claimed.to_be_bytes());
    let result = PsdDocument::from_bytes(bytes, &ParseOptions::none());
    assert!(
        matches!(result, Err(PsdError::LengthTooLarge { length }) if length == u64::from(claimed)),
        "{:?}",
        result.map(|_| ())
    );
}

#[test]
fn a_file_that_shrinks_after_the_open_gives_a_typed_error_on_render() {
    let bytes = large_document();
    let file = TempFile::new("shrunk.psd", &bytes);
    let document = PsdDocument::open(&file.0).expect("opens");
    let shrunk = std::fs::OpenOptions::new()
        .write(true)
        .open(&file.0)
        .expect("reopen");
    shrunk.set_len(bytes.len() as u64 / 4).expect("shrink");
    drop(shrunk);
    let result = document.composite_region(Viewport::new(0, 0, 64, 64), &ParseOptions::none());
    assert!(
        matches!(result, Err(PsdError::Truncated { .. })),
        "{:?}",
        result.map(|_| ())
    );
}
