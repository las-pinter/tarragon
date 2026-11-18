//! Render registry and plain renderer behavior.
//!
//! The tests prove the behavior: JPEG, PNG, WebP, and TIFF render to an image,
//! the extension match ignores case, the EXIF orientation is applied, and an
//! unknown, corrupt, or missing file takes the typed error path.
//!
//! Every fixture is a small image generated in a temporary folder. The tests
//! never touch the real user data directory.

#![allow(clippy::cast_possible_truncation)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use tarragon_core::error::RenderError;
use tarragon_render::registry;

/// A temporary folder that removes itself on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Create a unique temp folder with the given tag.
    fn new(tag: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("tarragon-render-{tag}-{}-{id}", std::process::id()));
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

/// Build a deterministic RGB image. The top-left pixel is `(0, 0, 128)`.
fn sample_rgb(width: u32, height: u32) -> RgbImage {
    let mut image = RgbImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
    }
    image
}

/// Build a deterministic opaque RGBA image. The top-left pixel is
/// `(0, 0, 128, 255)`.
fn sample_rgba(width: u32, height: u32) -> RgbaImage {
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]);
    }
    image
}

/// Write a small fixture image with an explicit format.
fn write_fixture(folder: &Path, name: &str, format: ImageFormat, rgba: bool) {
    let path = folder.join(name);
    let image = if rgba {
        DynamicImage::ImageRgba8(sample_rgba(40, 30))
    } else {
        DynamicImage::ImageRgb8(sample_rgb(40, 30))
    };
    image
        .save_with_format(&path, format)
        .expect("write the fixture");
}

/// Write a 50x100 JPEG with an APP1 EXIF segment that sets orientation 6.
///
/// The `image` crate can write JPEG but not EXIF, so the test inserts the APP1
/// segment itself. Orientation 6 means rotate 90 degrees clockwise.
fn write_exif_orientation_6_jpeg(folder: &Path, name: &str) -> PathBuf {
    let path = folder.join(name);
    DynamicImage::ImageRgb8(sample_rgb(50, 100))
        .save_with_format(&path, ImageFormat::Jpeg)
        .expect("write the base jpeg");
    let jpeg = std::fs::read(&path).expect("read the base jpeg");
    // Big-endian TIFF header with one IFD entry: orientation tag 0x0112 = 6.
    let tiff: [u8; 26] = [
        0x4d, 0x4d, 0x00, 0x2a, // "MM" and 42, big endian
        0x00, 0x00, 0x00, 0x08, // offset to IFD0
        0x00, 0x01, // one entry
        0x01, 0x12, // tag Orientation
        0x00, 0x03, // format SHORT
        0x00, 0x00, 0x00, 0x01, // count 1
        0x00, 0x06, 0x00, 0x00, // value 6
        0x00, 0x00, 0x00, 0x00, // next IFD offset
    ];
    // APP1 marker, length 34 = 2 (length) + 6 ("Exif\0\0") + 26 (TIFF).
    let mut app1 = vec![0xff, 0xe1, 0x00, 0x22];
    app1.extend_from_slice(b"Exif\x00\x00");
    app1.extend_from_slice(&tiff);
    // Insert APP1 after the SOI marker (0xff 0xd8).
    let mut with_exif = Vec::with_capacity(jpeg.len() + app1.len());
    with_exif.extend_from_slice(&jpeg[..2]);
    with_exif.extend_from_slice(&app1);
    with_exif.extend_from_slice(&jpeg[2..]);
    std::fs::write(&path, with_exif).expect("write the exif jpeg");
    path
}

#[test]
fn jpeg_png_webp_and_tiff_each_render_to_an_image() {
    let temp = TempDir::new("formats");
    // JPEG has no alpha, so it starts as RGB; the rest keep alpha.
    let cases = [
        ("photo.jpg", ImageFormat::Jpeg, false),
        ("photo.png", ImageFormat::Png, true),
        ("photo.webp", ImageFormat::WebP, true),
        ("photo.tiff", ImageFormat::Tiff, true),
    ];
    for (name, format, rgba) in cases {
        write_fixture(temp.path(), name, format, rgba);
        let rendered = registry::render(&temp.path().join(name)).expect("render the image");
        assert_eq!(
            (rendered.width(), rendered.height()),
            (40, 30),
            "{name} has the wrong size"
        );
    }
}

#[test]
fn the_lossless_formats_keep_the_pixel_values() {
    let temp = TempDir::new("pixels");
    for (name, format) in [
        ("photo.png", ImageFormat::Png),
        ("photo.webp", ImageFormat::WebP),
        ("photo.tiff", ImageFormat::Tiff),
    ] {
        write_fixture(temp.path(), name, format, true);
        let rendered = registry::render(&temp.path().join(name)).expect("render the image");
        // The render boundary returns a DynamicImage; convert for pixel checks.
        let rgba = rendered.to_rgba8();
        assert_eq!(rgba.get_pixel(0, 0).0, [0, 0, 128, 255], "{name}");
        assert_eq!(rgba.get_pixel(39, 29).0, [39, 29, 128, 255], "{name}");
    }
}

#[test]
fn a_jpeg_with_exif_orientation_6_is_rotated() {
    let temp = TempDir::new("exif");
    let path = write_exif_orientation_6_jpeg(temp.path(), "portrait.jpg");
    let rendered = registry::render(&path).expect("render the exif jpeg");
    // EXIF orientation 6 swaps the sides.
    assert_eq!((rendered.width(), rendered.height()), (100, 50));
    // The source top-left pixel is (0, 0, 128). Orientation 6 turns the image
    // clockwise, so that pixel lands at the top-right of the 100x50 output.
    // Orientation 5 keeps it at the top-left, so this pixel would be (0, 99,
    // 128) instead. JPEG is lossy, so allow 8 per channel.
    let [red, green, blue, _alpha] = rendered.to_rgba8().get_pixel(99, 0).0;
    assert!(
        red <= 8 && green <= 8 && blue.abs_diff(128) <= 8,
        "orientation 6 did not move (0, 0) to the top-right: ({red}, {green}, {blue})"
    );
}

#[test]
fn the_extension_match_is_case_insensitive() {
    let temp = TempDir::new("case");
    for (name, format, rgba) in [
        ("PHOTO.PNG", ImageFormat::Png, true),
        ("PHOTO.JPG", ImageFormat::Jpeg, false),
        ("PHOTO.WEBP", ImageFormat::WebP, true),
        ("PHOTO.TIFF", ImageFormat::Tiff, true),
    ] {
        write_fixture(temp.path(), name, format, rgba);
        let rendered = registry::render(&temp.path().join(name))
            .unwrap_or_else(|error| panic!("{name} was refused: {error}"));
        assert_eq!((rendered.width(), rendered.height()), (40, 30), "{name}");
    }
}

#[test]
fn an_unknown_extension_takes_the_error_path() {
    let temp = TempDir::new("unknown");
    // A valid PNG with a name the registry does not know.
    write_fixture(temp.path(), "photo.xyz", ImageFormat::Png, true);
    let result = registry::render(&temp.path().join("photo.xyz"));
    assert!(
        matches!(&result, Err(RenderError::UnsupportedFormat(ext)) if ext == ".xyz"),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_corrupt_file_takes_the_error_path() {
    let temp = TempDir::new("corrupt");
    let path = temp.path().join("broken.png");
    // A valid PNG signature, then garbage. The decode must fail, not panic.
    std::fs::write(&path, b"\x89PNG\r\n\x1a\nGARBAGE").expect("write the corrupt file");
    let result = registry::render(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn an_empty_file_takes_the_error_path() {
    let temp = TempDir::new("empty");
    let path = temp.path().join("empty.png");
    std::fs::write(&path, b"").expect("write the empty file");
    let result = registry::render(&path);
    assert!(
        matches!(result, Err(RenderError::Decode { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_missing_file_takes_the_error_path() {
    let temp = TempDir::new("missing");
    let result = registry::render(&temp.path().join("gone.png"));
    assert!(
        matches!(result, Err(RenderError::Io { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn a_name_with_no_extension_takes_the_error_path() {
    let temp = TempDir::new("noext");
    let path = temp.path().join("photo");
    std::fs::write(&path, b"not an image").expect("write the file");
    let result = registry::render(&path);
    assert!(
        matches!(&result, Err(RenderError::UnsupportedFormat(name)) if name == "photo"),
        "unexpected result: {result:?}"
    );
}
