"""Tests for the plain image render pipeline (tarragon.thumbnail)."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest
from PIL import Image

# =========================================================================
# _cache_file_path tests
# =========================================================================


def test_cache_file_path_returns_deterministic_hash(tmp_path: Path) -> None:
    """Calling _cache_file_path twice with the same input returns the same path."""
    from tarragon.thumbnail import _cache_file_path

    file_path = tmp_path / "some_image.jpg"
    file_path.write_text("dummy content so path resolves fine")

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        path_a, mime_a = _cache_file_path(file_path)
        path_b, mime_b = _cache_file_path(file_path)

    assert path_a == path_b
    assert mime_a == mime_b


def test_cache_file_path_default_extension_png(tmp_path: Path) -> None:
    """_cache_file_path returns a .png extension by default."""
    from tarragon.thumbnail import _cache_file_path

    file_path = tmp_path / "input.webp"
    file_path.write_text("dummy")

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, _ = _cache_file_path(file_path)

    assert result.suffix == ".png"


def test_cache_file_path_jpeg_extension(tmp_path: Path) -> None:
    """_cache_file_path returns .jpg extension when cache_format is 'jpeg'."""
    from tarragon.thumbnail import _cache_file_path

    file_path = tmp_path / "input.png"
    file_path.write_text("dummy")

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, _ = _cache_file_path(file_path, cache_format="jpeg")

    assert result.suffix == ".jpg"


@pytest.mark.parametrize(
    ("cache_format", "expected_ext", "expected_mime"),
    [
        ("png", ".png", "image/png"),
        ("jpeg", ".jpg", "image/jpeg"),
    ],
)
def test_cache_file_path_mime_type(tmp_path: Path, cache_format: str, expected_ext: str, expected_mime: str) -> None:
    """_cache_file_path returns the correct MIME type for each format."""
    from tarragon.thumbnail import _cache_file_path

    file_path = tmp_path / "test.tiff"
    file_path.write_text("dummy")

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, mime = _cache_file_path(file_path, cache_format=cache_format)

    assert result.suffix == expected_ext
    assert mime == expected_mime


def test_cache_file_path_hash_is_sha1_of_resolved_path(tmp_path: Path) -> None:
    """The filename stem is the SHA-1 hex digest of the resolved absolute path."""
    import hashlib

    from tarragon.thumbnail import _cache_file_path

    file_path = tmp_path / "photo.jpg"
    file_path.write_text("dummy")

    expected_hash = hashlib.sha1(str(file_path.resolve()).encode()).hexdigest()

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, _ = _cache_file_path(file_path)

    assert result.stem == expected_hash


# =========================================================================
# render_plain_image tests
# =========================================================================


def test_render_plain_image_opens_and_resizes(tmp_path: Path) -> None:
    """render_plain_image opens an image and shrinks it to fit MASTER_LONG_EDGE."""
    from tarragon.thumbnail import MASTER_LONG_EDGE, render_plain_image

    img_path = tmp_path / "large.jpg"
    img = Image.new("RGB", (4000, 3000), color="red")
    img.save(img_path)

    result = render_plain_image(img_path)

    assert result is not None
    # Longest side must be <= MASTER_LONG_EDGE
    assert max(result.size) <= MASTER_LONG_EDGE
    # Aspect ratio must be preserved (within floating-point tolerance)
    orig_ratio = 4000 / 3000
    result_ratio = result.size[0] / result.size[1]
    assert abs(result_ratio - orig_ratio) < 0.01, f"Aspect ratio changed: {result_ratio} != {orig_ratio}"


def test_render_plain_image_returns_none_for_nonexistent_file() -> None:
    """render_plain_image returns None when the file does not exist."""
    from tarragon.thumbnail import render_plain_image

    result = render_plain_image(Path("/tmp/this_file_does_not_exist_xyz.jpg"))
    assert result is None


def test_render_plain_image_returns_none_for_corrupt_file(tmp_path: Path) -> None:
    """render_plain_image returns None when the file is corrupt / unreadable."""
    from tarragon.thumbnail import render_plain_image

    corrupt_path = tmp_path / "corrupt.jpg"
    corrupt_path.write_bytes(b"this is not a valid image file")

    result = render_plain_image(corrupt_path)
    assert result is None


def test_render_plain_image_converts_grayscale_to_rgba(tmp_path: Path) -> None:
    """render_plain_image converts grayscale ('L') images to RGBA."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "grayscale.png"
    img = Image.new("L", (50, 50), color=128)
    img.save(img_path)

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGBA"


def test_render_plain_image_keeps_rgb(tmp_path: Path) -> None:
    """render_plain_image keeps RGB images in RGB mode."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "rgb.png"
    img = Image.new("RGB", (50, 50), color="blue")
    img.save(img_path)

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGB"


def test_render_plain_image_keeps_rgba(tmp_path: Path) -> None:
    """render_plain_image keeps RGBA images in RGBA mode."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "rgba.png"
    img = Image.new("RGBA", (50, 50), (255, 0, 0, 128))
    img.save(img_path)

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGBA"


def test_render_plain_image_converts_palette_to_rgba(tmp_path: Path) -> None:
    """render_plain_image converts palette-mode ('P') images to RGBA."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "palette.png"
    img = Image.new("P", (50, 50), color=0)
    img.save(img_path)

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGBA"


# =========================================================================
# MASTER_LONG_EDGE constant
# =========================================================================


def test_master_long_edge_constant() -> None:
    """MASTER_LONG_EDGE constant equals 2048."""
    from tarragon.thumbnail import MASTER_LONG_EDGE

    assert MASTER_LONG_EDGE == 2048
    assert isinstance(MASTER_LONG_EDGE, int)


# =========================================================================
# save_to_cache tests
# =========================================================================


def test_save_to_cache_writes_valid_png(tmp_path: Path) -> None:
    """save_to_cache writes a valid PNG file that can be re-opened."""
    from tarragon.thumbnail import save_to_cache

    img = Image.new("RGBA", (100, 100), color="green")
    cache_path = tmp_path / "output.png"

    save_to_cache(img, cache_path)

    assert cache_path.is_file()
    assert cache_path.stat().st_size > 0

    loaded = Image.open(cache_path)
    assert loaded.size == (100, 100)
    assert loaded.mode == "RGBA"


def test_save_to_cache_creates_parent_directories(tmp_path: Path) -> None:
    """save_to_cache creates intermediate directories when they don't exist."""
    from tarragon.thumbnail import save_to_cache

    img = Image.new("RGBA", (50, 50), color="blue")
    cache_path = tmp_path / "deep" / "nested" / "output.png"

    save_to_cache(img, cache_path)

    assert cache_path.is_file()
    assert cache_path.parent.is_dir()
    assert cache_path.parent.parent.is_dir()


def test_save_to_cache_jpeg_saves_rgb(tmp_path: Path) -> None:
    """save_to_cache with format_setting='jpeg' produces an RGB JPEG."""
    from tarragon.thumbnail import save_to_cache

    img = Image.new("RGBA", (100, 100), (255, 0, 0, 128))
    cache_path = tmp_path / "output.jpg"

    save_to_cache(img, cache_path, format_setting="jpeg")

    assert cache_path.is_file()
    assert cache_path.stat().st_size > 0

    loaded = Image.open(cache_path)
    assert loaded.mode == "RGB"


def test_save_to_cache_jpeg_flattens_alpha(tmp_path: Path) -> None:
    """save_to_cache with 'jpeg' replaces transparent areas with white."""
    from tarragon.thumbnail import save_to_cache

    # Fully transparent red pixel on a red background — after flattening
    # on white, the result should be white (255, 255, 255).
    img = Image.new("RGBA", (100, 100), (255, 0, 0, 0))
    cache_path = tmp_path / "output.jpg"

    save_to_cache(img, cache_path, format_setting="jpeg")

    loaded = Image.open(cache_path)
    pixel = loaded.getpixel((50, 50))
    assert pixel == (255, 255, 255), f"Expected white pixel, got {pixel}"


def test_save_to_cache_jpeg_handles_rgb_input(tmp_path: Path) -> None:
    """save_to_cache with 'jpeg' works when the input image is already RGB."""
    from tarragon.thumbnail import save_to_cache

    img = Image.new("RGB", (50, 50), color="green")
    cache_path = tmp_path / "output.jpg"

    save_to_cache(img, cache_path, format_setting="jpeg")

    assert cache_path.is_file()
    loaded = Image.open(cache_path)
    assert loaded.mode == "RGB"
    assert loaded.size == (50, 50)


def test_save_to_cache_png_handles_rgba(tmp_path: Path) -> None:
    """save_to_cache with default PNG preserves RGBA transparency."""
    from tarragon.thumbnail import save_to_cache

    # Semi-transparent red
    img = Image.new("RGBA", (50, 50), (255, 0, 0, 64))
    cache_path = tmp_path / "output.png"

    save_to_cache(img, cache_path)

    loaded = Image.open(cache_path)
    assert loaded.mode == "RGBA"
    # Check that alpha is preserved (not flattened)
    r, g, b, a = loaded.getpixel((25, 25))
    assert a == 64, f"Expected alpha 64, got {a}"


# =========================================================================
# _cache_file_path — edge cases
# =========================================================================


def test_cache_file_path_with_empty_string_resolves_to_cwd(tmp_path: Path) -> None:
    """_cache_file_path with empty string path resolves to CWD and returns a hash."""
    from tarragon.thumbnail import _cache_file_path

    # Path("") resolves to the current working directory
    empty_path = Path("")

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, mime = _cache_file_path(empty_path)

    # Should not crash — returns a deterministic hash path
    assert result.suffix == ".png"
    assert mime == "image/png"
    assert result.parent == tmp_path


def test_cache_file_path_with_relative_path_uses_resolved_path(tmp_path: Path) -> None:
    """_cache_file_path resolves relative paths before hashing."""
    import hashlib

    from tarragon.thumbnail import _cache_file_path

    rel_path = Path("relative_dir/some_image.jpg")
    expected_hash = hashlib.sha1(str(rel_path.resolve()).encode()).hexdigest()

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, _ = _cache_file_path(rel_path)

    assert result.stem == expected_hash
    assert result.suffix == ".png"


def test_cache_file_path_with_special_characters_in_path(tmp_path: Path) -> None:
    """_cache_file_path handles unicode and special characters in the path."""
    import hashlib

    from tarragon.thumbnail import _cache_file_path

    special_path = tmp_path / "föö bär 🖼️ image!.webp"
    special_path.write_text("dummy content")

    expected_hash = hashlib.sha1(str(special_path.resolve()).encode()).hexdigest()

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, _ = _cache_file_path(special_path)

    assert result.stem == expected_hash
    assert result.suffix == ".png"


def test_cache_file_path_non_existent_path_still_hashes(tmp_path: Path) -> None:
    """_cache_file_path works even if the file path does not exist on disk (no I/O)."""
    import hashlib

    from tarragon.thumbnail import _cache_file_path

    non_existent = tmp_path / "i_do_not_exist.jpg"
    expected_hash = hashlib.sha1(str(non_existent.resolve()).encode()).hexdigest()

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, _ = _cache_file_path(non_existent)

    # resolve() still works on non-existent paths
    assert result.stem == expected_hash


def test_cache_file_path_symlink_resolves_to_real_path(tmp_path: Path) -> None:
    """_cache_file_path resolves symlinks so a symlink and its target share the same hash."""
    import hashlib

    from tarragon.thumbnail import _cache_file_path

    real = tmp_path / "real_target.jpg"
    real.write_text("real file")
    link = tmp_path / "link_to_real.jpg"
    link.symlink_to(real)

    expected_hash = hashlib.sha1(str(real.resolve()).encode()).hexdigest()

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        result, _ = _cache_file_path(link)

    assert result.stem == expected_hash


def test_cache_file_path_unknown_format_raises_value_error(tmp_path: Path) -> None:
    """_cache_file_path raises ValueError when cache_format is unknown."""
    from tarragon.thumbnail import _cache_file_path

    file_path = tmp_path / "input.png"
    file_path.write_text("dummy")

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        with pytest.raises(ValueError, match="Unknown cache_format"):
            _cache_file_path(file_path, cache_format="gif")


# =========================================================================
# render_plain_image — edge cases
# =========================================================================


def test_render_plain_image_one_by_one_pixel(tmp_path: Path) -> None:
    """render_plain_image handles 1x1 pixel images without error."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "one_by_one.png"
    img = Image.new("RGB", (1, 1), color="red")
    img.save(img_path)

    result = render_plain_image(img_path)

    assert result is not None
    # 1x1 should remain 1x1 since it's already <= MASTER_LONG_EDGE
    assert result.size == (1, 1)
    assert result.mode == "RGB"


def test_render_plain_image_smaller_than_master_long_edge_is_not_upscaled(tmp_path: Path) -> None:
    """render_plain_image does NOT upscale images smaller than MASTER_LONG_EDGE."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "small.png"
    small_size = (100, 200)
    img = Image.new("RGB", small_size, color="blue")
    img.save(img_path)

    result = render_plain_image(img_path)

    assert result is not None
    # Thumbnail only shrinks, never enlarges
    assert result.size == small_size


def test_render_plain_image_very_large_image_still_resizes(tmp_path: Path) -> None:
    """render_plain_image resizes a very large image (e.g. 6000x4000) successfully."""
    from tarragon.thumbnail import MASTER_LONG_EDGE, render_plain_image

    img_path = tmp_path / "very_large.jpg"
    # 6000x4000 = 24 MP — large enough to stress the resize path
    img = Image.new("RGB", (6000, 4000), color="green")
    img.save(img_path, format="JPEG", quality=85)

    result = render_plain_image(img_path)

    assert result is not None
    assert max(result.size) <= MASTER_LONG_EDGE
    # Aspect ratio should be preserved
    assert abs(result.size[0] / result.size[1] - 6000 / 4000) < 0.01


def test_render_plain_image_animated_gif_uses_first_frame(tmp_path: Path) -> None:
    """render_plain_image opens only the first frame of an animated GIF (mode P → RGBA)."""
    from tarragon.thumbnail import render_plain_image

    frames = []
    for color in ("red", "blue"):
        frame = Image.new("P", (100, 100))
        # Set a palette entry to the desired color
        frame.info["transparency"] = 0
        frames.append(frame)

    img_path = tmp_path / "animated.gif"
    frames[0].save(
        img_path,
        save_all=True,
        append_images=frames[1:],
        duration=100,
        loop=0,
    )

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGBA"  # P converted to RGBA
    assert result.size == (100, 100)


def test_render_plain_image_converts_cmyk_to_rgba(tmp_path: Path) -> None:
    """render_plain_image converts CMYK images to RGBA."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "cmyk.tiff"
    img = Image.new("CMYK", (100, 100), (0, 0, 0, 0))
    img.save(img_path, format="TIFF")

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGBA", f"Expected RGBA, got {result.mode}"


def test_render_plain_image_with_icc_profile(tmp_path: Path) -> None:
    """render_plain_image handles images with embedded ICC profiles."""
    from PIL import ImageCms
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "icc_profile.jpg"
    img = Image.new("RGB", (200, 150), color="red")

    # Create and attach an sRGB ICC profile
    srgb_profile = ImageCms.createProfile("sRGB")
    img.save(img_path, format="JPEG", icc_profile=ImageCms.ImageCmsProfile(srgb_profile).tobytes())

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGB"
    assert max(result.size) <= 2048


def test_render_plain_image_with_exif_orientation_is_auto_rotated(tmp_path: Path) -> None:
    """render_plain_image auto-rotates based on EXIF orientation tag."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "exif_portrait.jpg"
    # Create a portrait-orientation image (tall, not wide)
    img = Image.new("RGB", (50, 100), color="red")

    # Set EXIF orientation: 6 = rotate 90 CW (meaning the camera was rotated)
    exif = img.getexif()
    exif[0x0112] = 6
    img.save(img_path, format="JPEG", exif=exif)

    result = render_plain_image(img_path)

    assert result is not None
    # render_plain_image NOW auto-rotates via ImageOps.exif_transpose,
    # so dimensions swap from (50, 100) to (100, 50)
    assert result.size == (100, 50), f"Expected (100, 50) after auto-rotation, got {result.size}"


def test_render_plain_image_corrupt_jpeg_valid_header_returns_none(tmp_path: Path) -> None:
    """render_plain_image returns None for a JPEG with valid magic bytes but garbage data."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "corrupt_header.jpg"
    # JPEG starts with FF D8 FF — valid header, then garbage
    corrupt_data = b"\xff\xd8\xff\xe0" + b"\x00" * 100 + b"GARBAGE" * 50
    img_path.write_bytes(corrupt_data)

    result = render_plain_image(img_path)

    assert result is None


def test_render_plain_image_follows_symlinks(tmp_path: Path) -> None:
    """render_plain_image follows symlinks to valid image files."""
    from tarragon.thumbnail import render_plain_image

    real_img = tmp_path / "real_image.png"
    img = Image.new("RGB", (100, 100), color="green")
    img.save(real_img)

    link_path = tmp_path / "link_image.png"
    link_path.symlink_to(real_img)

    result = render_plain_image(link_path)

    assert result is not None
    assert result.mode == "RGB"
    assert max(result.size) <= 2048


def test_render_plain_image_converts_one_bit_to_rgba(tmp_path: Path) -> None:
    """render_plain_image converts mode '1' (1-bit) images to RGBA."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "one_bit.png"
    from PIL import Image

    img = Image.new("1", (50, 50), 1)  # 1-bit: 1 = white
    img.save(img_path, format="PNG")

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGBA", f"Expected RGBA, got {result.mode}"


def test_render_plain_image_handles_ycbcr_jpeg(tmp_path: Path) -> None:
    """render_plain_image handles YCbCr JPEGs — Pillow auto-converts to RGB on open."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "ycbcr.jpg"
    img = Image.new("YCbCr", (100, 100), (128, 128, 128))
    img.save(img_path, format="JPEG")

    result = render_plain_image(img_path)

    assert result is not None
    # Pillow's JPEG decoder converts YCbCr to RGB when opening, so mode is "RGB"
    assert result.mode == "RGB", f"Expected RGB (Pillow auto-converts YCbCr JPEG), got {result.mode}"


def test_render_plain_image_empty_file_returns_none(tmp_path: Path) -> None:
    """render_plain_image returns None for an empty file."""
    from tarragon.thumbnail import render_plain_image

    empty_path = tmp_path / "empty.png"
    empty_path.write_text("")

    result = render_plain_image(empty_path)

    assert result is None


def test_render_plain_image_rgba_source(tmp_path: Path) -> None:
    """render_plain_image keeps RGBA source images in RGBA mode."""
    from tarragon.thumbnail import render_plain_image

    img_path = tmp_path / "rgba_source.png"
    # Create RGBA image and save as PNG (which supports alpha)
    img = Image.new("RGBA", (100, 100), (255, 0, 0, 128))
    img.save(img_path, format="PNG")

    result = render_plain_image(img_path)

    assert result is not None
    assert result.mode == "RGBA"
    assert result.size == (100, 100)


# =========================================================================
# save_to_cache — edge cases
# =========================================================================


def test_save_to_cache_with_none_image_raises_type_error(tmp_path: Path) -> None:
    """save_to_cache raises TypeError when img is None."""
    from tarragon.thumbnail import save_to_cache

    cache_path = tmp_path / "output.png"

    with pytest.raises(TypeError, match="save_to_cache"):
        save_to_cache(None, cache_path)


def test_save_to_cache_unknown_format_defaults_to_png(tmp_path: Path) -> None:
    """save_to_cache with an unknown format_setting defaults to PNG saving behavior."""
    from tarragon.thumbnail import save_to_cache

    img = Image.new("RGBA", (50, 50), (255, 0, 0, 128))
    cache_path = tmp_path / "output.unknown"

    # format_setting that is neither "png" nor "jpeg" falls through to else (PNG)
    save_to_cache(img, cache_path, format_setting="webp")

    assert cache_path.is_file()
    loaded = Image.open(cache_path)
    # PNG path was taken, so RGBA is preserved
    assert loaded.mode == "RGBA"


def test_save_to_cache_parent_is_file_not_directory(tmp_path: Path) -> None:
    """save_to_cache raises an error when the parent of cache_path is a file, not a directory."""
    from tarragon.thumbnail import save_to_cache

    # Create a file where we'd expect a directory
    parent_file = tmp_path / "i_am_a_file"
    parent_file.write_text("not a directory")
    cache_path = parent_file / "output.png"

    img = Image.new("RGB", (10, 10), color="red")

    with pytest.raises((OSError, NotADirectoryError)):
        save_to_cache(img, cache_path)


def test_save_to_cache_jpeg_with_grayscale_image(tmp_path: Path) -> None:
    """save_to_cache with 'jpeg' saves a grayscale image as RGB JPEG."""
    from tarragon.thumbnail import save_to_cache

    img = Image.new("L", (50, 50), color=128)
    cache_path = tmp_path / "grayscale.jpg"

    save_to_cache(img, cache_path, format_setting="jpeg")

    assert cache_path.is_file()
    loaded = Image.open(cache_path)
    # JPEG save path takes L → .convert("RGB") → RGB JPEG
    assert loaded.mode == "RGB"


def test_save_to_cache_jpeg_with_palette_image(tmp_path: Path) -> None:
    """save_to_cache with 'jpeg' converts palette image to RGB before saving."""
    from tarragon.thumbnail import save_to_cache

    img = Image.new("P", (50, 50), color=0)
    cache_path = tmp_path / "palette.jpg"

    save_to_cache(img, cache_path, format_setting="jpeg")

    assert cache_path.is_file()
    loaded = Image.open(cache_path)
    assert loaded.mode == "RGB"


def test_save_to_cache_png_with_grayscale_image(tmp_path: Path) -> None:
    """save_to_cache with default PNG saves any image mode as PNG."""
    from tarragon.thumbnail import save_to_cache

    img = Image.new("L", (50, 50), color=128)
    cache_path = tmp_path / "grayscale.png"

    save_to_cache(img, cache_path)

    assert cache_path.is_file()
    loaded = Image.open(cache_path)
    # PNG path saves whatever mode the image is in
    assert loaded.size == (50, 50)
