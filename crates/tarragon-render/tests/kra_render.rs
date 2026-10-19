//! Krita renderer behavior.
//!
//! The tests prove the behavior: the committed `.kra` fixture renders its
//! `mergedimage.png` member, a preview-only archive falls back to
//! `preview.png`, the merge-wins rule holds, only exact root-level member names
//! match, a deflated member decodes, an EXIF orientation swaps the size, a
//! wrong CRC and a corrupt deflate stream map to `Decode`, a canvas header
//! above the edge maps to `Decode`, a canvas header on the edge passes the
//! dimension gate, and a malformed or missing file takes the typed error path
//! instead of a panic.
//!
//! The fixture lives in `tests/fixtures/kra/`. Its builder is in
//! `tests/kra_fixture/`, and the check test keeps the committed bytes and the
//! builder in step.

mod kra_fixture;

use tarragon_core::error::RenderError;
use tarragon_render::krita::render_kra_image;
use tarragon_render::registry;

use kra_fixture as fixture;

/// Write bytes to a file in a temporary folder and return the path.
fn write_temp(folder: &fixture::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = folder.path().join(name);
    std::fs::write(&path, bytes).expect("the file is written");
    path
}

#[test]
fn the_committed_fixture_matches_the_builder() {
    let committed = std::fs::read(fixture::fixture("merged.kra")).expect("the fixture reads");
    let built = fixture::build_default();
    assert_eq!(committed, built, "the committed fixture drifted");
}

#[test]
#[ignore = "rewrites the committed kra fixture"]
fn rewrite_the_committed_fixture() {
    std::fs::write(fixture::fixture("merged.kra"), fixture::build_default())
        .expect("the fixture is written");
}

#[test]
fn the_committed_fixture_renders_the_merged_image() {
    let rendered = registry::render(&fixture::fixture("merged.kra")).expect("the fixture renders");
    assert_eq!(
        (rendered.width(), rendered.height()),
        (fixture::MERGED_WIDTH, fixture::MERGED_HEIGHT)
    );
    // PNG is lossless, so every pixel must come back unchanged.
    let pixels = rendered.to_rgba8();
    assert_eq!(pixels.as_raw(), fixture::merged_image().as_raw());
}

#[test]
fn a_preview_only_archive_renders_the_preview() {
    let folder = fixture::TempDir::new("preview-only");
    let path = write_temp(&folder, "preview.kra", &fixture::build_preview_only());
    let rendered = render_kra_image(&path).expect("the preview renders");
    assert_eq!(
        (rendered.width(), rendered.height()),
        (fixture::PREVIEW_WIDTH, fixture::PREVIEW_HEIGHT)
    );
    assert_eq!(
        rendered.to_rgba8().as_raw(),
        fixture::preview_image().as_raw()
    );
}

#[test]
fn a_deflated_member_renders_the_merged_image() {
    let folder = fixture::TempDir::new("deflated");
    let path = write_temp(&folder, "deflated.kra", &fixture::build_deflated());
    let rendered = render_kra_image(&path).expect("the deflated member renders");
    assert_eq!(
        (rendered.width(), rendered.height()),
        (fixture::MERGED_WIDTH, fixture::MERGED_HEIGHT)
    );
    assert_eq!(
        rendered.to_rgba8().as_raw(),
        fixture::merged_image().as_raw()
    );
}

#[test]
fn an_exif_orientation_swaps_the_size_and_moves_a_corner() {
    let folder = fixture::TempDir::new("oriented");
    let path = write_temp(&folder, "oriented.kra", &fixture::build_oriented());
    let rendered = render_kra_image(&path).expect("the oriented member renders");
    assert_eq!(
        (rendered.width(), rendered.height()),
        (fixture::ORIENTED_HEIGHT, fixture::ORIENTED_WIDTH)
    );
    // Orientation 6 rotates 90 degrees clockwise: the bottom-left source
    // corner lands at the top-left, and the top-left source corner lands at
    // the top-right.
    let pixels = rendered.to_rgba8();
    assert_eq!(
        pixels.get_pixel(0, 0),
        &image::Rgba(fixture::ORIENTED_BOTTOM_LEFT)
    );
    assert_eq!(
        pixels.get_pixel(1, 0),
        &image::Rgba(fixture::ORIENTED_TOP_LEFT)
    );
    assert_eq!(
        pixels.get_pixel(0, 3),
        &image::Rgba(fixture::ORIENTED_BOTTOM_RIGHT)
    );
    assert_eq!(
        pixels.get_pixel(1, 3),
        &image::Rgba(fixture::ORIENTED_TOP_RIGHT)
    );
}

#[test]
fn an_archive_with_no_image_takes_the_error_path() {
    let folder = fixture::TempDir::new("no-image");
    let path = write_temp(&folder, "no-image.kra", &fixture::build_without_image());
    let result = render_kra_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_nested_member_name_takes_the_error_path() {
    let folder = fixture::TempDir::new("nested");
    let path = write_temp(&folder, "nested.kra", &fixture::build_nested());
    let result = render_kra_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn an_uppercase_member_name_takes_the_error_path() {
    let folder = fixture::TempDir::new("uppercase");
    let path = write_temp(&folder, "uppercase.kra", &fixture::build_uppercase());
    let result = render_kra_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_garbage_member_takes_the_error_path() {
    let folder = fixture::TempDir::new("garbage-member");
    let path = write_temp(
        &folder,
        "garbage-member.kra",
        &fixture::build_garbage_member(),
    );
    let result = render_kra_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_wrong_member_crc_takes_the_decode_error_path() {
    let folder = fixture::TempDir::new("corrupt-crc");
    let path = write_temp(&folder, "corrupt-crc.kra", &fixture::build_corrupt_crc());
    let result = render_kra_image(&path);
    match result {
        Err(RenderError::Decode { reason, .. }) => {
            assert!(
                reason.to_lowercase().contains("checksum"),
                "the reason must name the checksum: {reason}"
            );
        }
        other => panic!("the CRC mismatch must map to Decode: {other:?}"),
    }
}

#[test]
fn a_corrupt_deflate_stream_takes_the_decode_error_path() {
    let folder = fixture::TempDir::new("corrupt-deflate");
    let path = write_temp(
        &folder,
        "corrupt-deflate.kra",
        &fixture::build_corrupt_deflate_stream(),
    );
    let result = render_kra_image(&path);
    match result {
        Err(RenderError::Decode { reason, .. }) => {
            assert!(
                reason.to_lowercase().contains("deflate"),
                "the reason must name the deflate stream: {reason}"
            );
        }
        other => panic!("the corrupt deflate stream must map to Decode: {other:?}"),
    }
}

#[test]
fn an_oversized_canvas_header_takes_the_decode_error_path() {
    let folder = fixture::TempDir::new("oversized-canvas");
    let path = write_temp(
        &folder,
        "oversized-canvas.kra",
        &fixture::build_oversized_canvas(),
    );
    let result = render_kra_image(&path);
    match result {
        Err(RenderError::Decode { reason, .. }) => {
            assert!(
                reason.to_lowercase().contains("size exceeds"),
                "the reason must name the dimension gate: {reason}"
            );
        }
        other => panic!("the oversized canvas must map to Decode: {other:?}"),
    }
}

#[test]
fn a_canvas_on_the_edge_passes_the_dimension_gate() {
    let folder = fixture::TempDir::new("boundary-canvas");
    let path = write_temp(
        &folder,
        "boundary-canvas.kra",
        &fixture::build_boundary_canvas(),
    );
    let result = render_kra_image(&path);
    // The member holds no image data, so the decode must fail on the missing
    // IDAT chunk, not on an edge or a byte cap.
    match result {
        Err(RenderError::Decode { reason, .. }) => {
            let reason = reason.to_lowercase();
            assert!(
                reason.contains("idat or fdat chunk is missing"),
                "the decode must fail on the missing image data: {reason}"
            );
            assert!(
                !reason.contains("byte cap"),
                "the byte cap must not reject 8192 pixels: {reason}"
            );
        }
        other => panic!("the header-only member must fail at decode: {other:?}"),
    }
}

#[test]
fn an_empty_file_takes_the_error_path() {
    let folder = fixture::TempDir::new("empty");
    let path = write_temp(&folder, "empty.kra", &fixture::build_empty());
    let result = render_kra_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_directory_path_takes_the_io_error_path() {
    let folder = fixture::TempDir::new("directory");
    let result = render_kra_image(folder.path());
    assert!(
        matches!(result, Err(RenderError::Io { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_garbage_file_takes_the_error_path() {
    let folder = fixture::TempDir::new("garbage");
    let path = write_temp(&folder, "garbage.kra", &fixture::build_garbage());
    let result = render_kra_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_truncated_archive_takes_the_error_path() {
    let folder = fixture::TempDir::new("truncated");
    let path = write_temp(&folder, "truncated.kra", &fixture::build_truncated());
    let result = render_kra_image(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_missing_file_takes_the_error_path() {
    let folder = fixture::TempDir::new("missing");
    let result = render_kra_image(&folder.path().join("gone.kra"));
    assert!(
        matches!(result, Err(RenderError::Io { .. })),
        "unexpected result: {result:?}"
    );
}
