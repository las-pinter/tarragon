"""Tests for the plain image render pipeline (tarragon.thumbnail)."""

from __future__ import annotations

import threading
from concurrent.futures import CancelledError
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

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
    """render_plain_image opens an image and shrinks it to fit target_size when specified."""
    from tarragon.thumbnail import MASTER_LONG_EDGE, render_plain_image

    img_path = tmp_path / "large.jpg"
    img = Image.new("RGB", (4000, 3000), color="red")
    img.save(img_path)

    result = render_plain_image(img_path, target_size=MASTER_LONG_EDGE)

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
    """render_plain_image resizes a very large image (e.g. 6000x4000) when target_size is given."""
    from tarragon.thumbnail import MASTER_LONG_EDGE, render_plain_image

    img_path = tmp_path / "very_large.jpg"
    # 6000x4000 = 24 MP — large enough to stress the resize path
    img = Image.new("RGB", (6000, 4000), color="green")
    img.save(img_path, format="JPEG", quality=85)

    result = render_plain_image(img_path, target_size=MASTER_LONG_EDGE)

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


# =========================================================================
# PSD / PSB compositing tests
# =========================================================================


def test_compute_worker_count_default() -> None:
    """_compute_worker_count with no override returns a sensible value in [1, 8]."""
    from tarragon.thumbnail import _compute_worker_count

    result = _compute_worker_count()
    assert isinstance(result, int)
    assert 1 <= result <= 8


@pytest.mark.parametrize(
    ("override", "expected"),
    [
        (3, 3),
        (0, 1),  # below minimum — clamped to 1
        (10, 8),  # above maximum — clamped to 8
        (1, 1),  # at minimum
        (8, 8),  # at maximum
        (-5, 1),  # negative — clamped to 1
    ],
)
def test_compute_worker_count_manual_override(override: int, expected: int) -> None:
    """_compute_worker_count clamps manual override to [1, 8]."""
    from tarragon.thumbnail import _compute_worker_count

    assert _compute_worker_count(override) == expected


def test_compute_worker_count_minimum_one() -> None:
    """_compute_worker_count returns at least 1 even with zero available RAM."""
    from unittest.mock import patch

    from tarragon.thumbnail import _compute_worker_count

    with patch("tarragon.thumbnail.psutil.virtual_memory") as mock_vm:
        mock_vm.return_value.available = 0
        assert _compute_worker_count() == 1


def test_shared_executor_is_singleton() -> None:
    """Multiple calls to _get_executor return the same instance."""
    from tarragon.thumbnail import _get_executor

    exec1 = _get_executor()
    exec2 = _get_executor()
    assert exec1 is exec2


def test_render_psd_image_nonexistent_file(tmp_path: Path) -> None:
    """render_psd_image returns None when the file does not exist."""
    from unittest.mock import MagicMock, patch

    from tarragon.thumbnail import render_psd_image

    with patch("tarragon.thumbnail._get_executor") as mock_get_exec:
        mock_exec = MagicMock()
        mock_get_exec.return_value = mock_exec
        mock_future = MagicMock()
        mock_future.result.return_value = None  # worker returns None
        mock_exec.submit.return_value = mock_future

        result = render_psd_image(tmp_path / "nonexistent.psd", 20.0, 2, 2)
        assert result is None
        mock_exec.submit.assert_called_once()


def test_render_psd_image_corrupt_file(tmp_path: Path) -> None:
    """render_psd_image returns None when the worker encounters an error."""
    from unittest.mock import MagicMock, patch

    from tarragon.thumbnail import render_psd_image

    with patch("tarragon.thumbnail._get_executor") as mock_get_exec:
        mock_exec = MagicMock()
        mock_get_exec.return_value = mock_exec
        mock_future = MagicMock()
        mock_future.result.side_effect = Exception("Worker failure")
        mock_exec.submit.return_value = mock_future

        result = render_psd_image(tmp_path / "corrupt.psd", 20.0, 2, 2)
        assert result is None
        mock_exec.submit.assert_called_once()


# =========================================================================
# PSD / PSB compositing — edge case coverage expansion
# =========================================================================


def test_compute_worker_count_explicit_none_override() -> None:
    """_compute_worker_count(None) falls through to RAM-based calculation same as no arg."""
    from tarragon.thumbnail import _compute_worker_count

    default = _compute_worker_count()
    explicit_none = _compute_worker_count(None)

    assert isinstance(explicit_none, int)
    assert 1 <= explicit_none <= 8
    # Both paths go through the same RAM logic
    assert explicit_none == default


def test_compute_worker_count_multiple_calls_reevaluates_ram() -> None:
    """_compute_worker_count re-evaluates available RAM on each call (not cached)."""
    from unittest.mock import patch

    from tarragon.thumbnail import _compute_worker_count

    with patch("tarragon.thumbnail.psutil.virtual_memory") as mock_vm:
        # First call: 400 MB available → 400 // 200 = 2
        mock_vm.return_value.available = 400_000_000
        first = _compute_worker_count()
        assert first == 2

        # Second call: 1.6 GB available → 1600 // 200 = 8 → min(8,8) = 8
        mock_vm.return_value.available = 1_600_000_000
        second = _compute_worker_count()
        assert second == 8

        # Third call: 50 MB available → 50 // 200 = 0 → max(1, 0) = 1
        mock_vm.return_value.available = 50_000_000
        third = _compute_worker_count()
        assert third == 1


def test_compute_worker_count_max_ram_returns_at_most_8() -> None:
    """_compute_worker_count never exceeds 8 even with absurdly high RAM."""
    from unittest.mock import patch

    from tarragon.thumbnail import _compute_worker_count

    with patch("tarragon.thumbnail.psutil.virtual_memory") as mock_vm:
        mock_vm.return_value.available = 100_000_000_000  # 100 GB
        assert _compute_worker_count() == 8


def test_composite_psd_in_process_nonexistent_file_returns_none() -> None:
    """_composite_psd_in_process returns None for a file path that does not exist."""
    from tarragon.thumbnail import _composite_psd_in_process

    result = _composite_psd_in_process("/tmp/this_path_definitely_does_not_exist.psd", 20.0, 2, 2)
    assert result is None


def test_composite_psd_in_process_empty_file_returns_none(tmp_path: Path) -> None:
    """_composite_psd_in_process returns None when the file exists but is empty."""
    from tarragon.thumbnail import _composite_psd_in_process

    empty_path = tmp_path / "empty.psd"
    empty_path.write_text("")

    result = _composite_psd_in_process(str(empty_path), 20.0, 2, 2)
    assert result is None


def test_composite_psd_in_process_truncated_file_returns_none(tmp_path: Path) -> None:
    """_composite_psd_in_process returns None when the PSD file is truncated/invalid."""
    from tarragon.thumbnail import _composite_psd_in_process

    bad_path = tmp_path / "truncated.psd"
    # Write just the PSD header magic bytes but no valid layer data
    bad_path.write_bytes(b"8BPS\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00")

    result = _composite_psd_in_process(str(bad_path), 20.0, 2, 2)
    assert result is None


def test_composite_psd_in_process_missing_psdtools_returns_none(tmp_path: Path) -> None:
    """_composite_psd_in_process returns None when psd_tools is missing.

    The import is now inside the try/except block, so ImportError is caught
    and the function returns None gracefully.
    """
    import builtins

    from tarragon.thumbnail import _composite_psd_in_process

    original_import = builtins.__import__

    def mock_import(name: str, *args: Any, **kwargs: Any) -> Any:
        if name == "psd_tools":
            raise ImportError("No module named psd_tools")
        return original_import(name, *args, **kwargs)

    with patch("builtins.__import__", side_effect=mock_import):
        result = _composite_psd_in_process(str(tmp_path / "fake.psd"), 20.0, 2, 2)
        assert result is None


def test_composite_psd_in_process_large_canvas_uses_tiled_path() -> None:
    """_composite_psd_in_process uses tiled compositing for canvases >20 MP.

    A 5000x5000 canvas (25 MP) triggers the tiled path, which calls
    composite() with viewport arguments for each of the 4 tiles.
    """
    from tarragon.thumbnail import _composite_psd_in_process

    # Mock PSDImage class with open() returning a mock instance
    mock_psd_cls = MagicMock()
    mock_psd_instance = MagicMock()
    mock_psd_instance.width = 5000
    mock_psd_instance.height = 5000
    # Each tile composite returns a small RGBA image
    tile_img = Image.new("RGBA", (2500, 2500), (255, 0, 0, 255))
    mock_psd_instance.composite.return_value = tile_img
    mock_psd_cls.open.return_value = mock_psd_instance

    with patch("psd_tools.PSDImage", mock_psd_cls):
        result = _composite_psd_in_process("/fake/large_canvas.psd", 20.0, 2, 2)

    # Should succeed with tiled path
    assert result is not None
    assert isinstance(result, bytes)
    assert len(result) > 0

    # Composite should have been called 4 times (2x2 grid) with viewport
    assert mock_psd_instance.composite.call_count == 4
    for call in mock_psd_instance.composite.call_args_list:
        _, kwargs = call
        assert "viewport" in kwargs, "Tiled path should pass viewport to composite()"
        assert kwargs.get("force") is True


def test_get_executor_creates_executor_on_first_call() -> None:
    """_get_executor lazily creates the executor — it is None before first call."""
    import tarragon.thumbnail as _tmod

    # Start clean
    saved = _tmod._shared_executor
    _tmod._shared_executor = None
    try:
        assert _tmod._shared_executor is None
        executor = _tmod._get_executor()
        assert executor is not None
        assert _tmod._shared_executor is executor
    finally:
        # Cleanup only if we created one
        if _tmod._shared_executor is not None and _tmod._shared_executor is not saved:
            _tmod._shutdown_executor()
        _tmod._shared_executor = saved


def test_get_executor_after_shutdown_creates_new_instance() -> None:
    """_get_executor after _shutdown_executor creates a brand new executor."""
    import tarragon.thumbnail as _tmod

    saved = _tmod._shared_executor
    _tmod._shared_executor = None
    try:
        first = _tmod._get_executor()
        assert first is not None

        # Shut it down
        _tmod._shutdown_executor()
        assert _tmod._shared_executor is None

        # Get again — should be a new instance
        second = _tmod._get_executor()
        assert second is not None
        assert second is not first
    finally:
        if _tmod._shared_executor is not None and _tmod._shared_executor is not saved:
            _tmod._shutdown_executor()
        _tmod._shared_executor = saved


def test_shutdown_executor_when_already_none_is_safe() -> None:
    """_shutdown_executor does not error when called with _shared_executor already None."""
    import tarragon.thumbnail as _tmod

    saved = _tmod._shared_executor
    _tmod._shared_executor = None
    try:
        # Should not raise
        _tmod._shutdown_executor()
        _tmod._shutdown_executor()
        _tmod._shutdown_executor()
        assert _tmod._shared_executor is None
    finally:
        _tmod._shared_executor = saved


def test_render_psd_image_timeout_returns_none() -> None:
    """render_psd_image returns None when the future does not complete within the timeout."""
    from tarragon.thumbnail import render_psd_image

    with patch("tarragon.thumbnail._get_executor") as mock_get_exec:
        mock_exec = MagicMock()
        mock_get_exec.return_value = mock_exec
        mock_future = MagicMock()
        # Simulate a future that never completes: done() returns False,
        # result() keeps raising TimeoutError, then eventually a generic Exception.
        mock_future.done.return_value = False
        mock_future.result.side_effect = [
            TimeoutError("poll 1"),
            TimeoutError("poll 2"),
            Exception("giving up"),
        ]
        mock_exec.submit.return_value = mock_future

        result = render_psd_image(Path("/fake/timeout_test.psd"), 20.0, 2, 2)
        assert result is None
        mock_exec.submit.assert_called_once()


def test_render_psd_image_cancelled_future_returns_none() -> None:
    """render_psd_image returns None when the future is cancelled."""
    from tarragon.thumbnail import render_psd_image

    with patch("tarragon.thumbnail._get_executor") as mock_get_exec:
        mock_exec = MagicMock()
        mock_get_exec.return_value = mock_exec
        mock_future = MagicMock()
        mock_future.result.side_effect = CancelledError()
        mock_exec.submit.return_value = mock_future

        result = render_psd_image(Path("/fake/cancelled_test.psd"), 20.0, 2, 2)
        assert result is None
        mock_exec.submit.assert_called_once()


def test_render_psd_image_after_executor_shutdown_succeeds(tmp_path: Path) -> None:
    """render_psd_image still works if the shared executor was previously shut down.

    _get_executor should create a new executor transparently.
    """
    from tarragon.thumbnail import render_psd_image

    with patch("tarragon.thumbnail._get_executor") as mock_get_exec:
        # Simulate: first call returns an executor, shutdown sets it to None,
        # second call returns a new one
        first_exec = MagicMock()
        second_exec = MagicMock()

        mock_get_exec.side_effect = [first_exec, second_exec]

        first_future = MagicMock()
        first_future.result.side_effect = Exception("First executor dead")
        first_exec.submit.return_value = first_future

        second_future = MagicMock()
        second_future.result.return_value = None  # worker returns None
        second_exec.submit.return_value = second_future

        # First call — executor is dead (simulating shutdown between calls)
        result1 = render_psd_image(tmp_path / "test_first.psd", 20.0, 2, 2)
        assert result1 is None

        # Second call — new executor (should work)
        result2 = render_psd_image(tmp_path / "test_second.psd", 20.0, 2, 2)
        assert result2 is None

        # Verify both submits were called on their respective executors
        first_exec.submit.assert_called_once()
        second_exec.submit.assert_called_once()


def test_render_psd_image_concurrent_calls_are_safe(tmp_path: Path) -> None:
    """Multiple concurrent calls to render_psd_image do not crash.

    Each call gets its own future from the shared executor.
    """
    from tarragon.thumbnail import render_psd_image

    call_count = 0
    call_lock = threading.Lock()
    barrier = threading.Barrier(5, timeout=5)
    results: list[Exception | object | None] = [None] * 5

    with patch("tarragon.thumbnail._get_executor") as mock_get_exec:
        mock_exec = MagicMock()
        mock_get_exec.return_value = mock_exec

        def make_future(*args: object) -> MagicMock:
            mock_future = MagicMock()
            mock_future.result.return_value = None
            return mock_future

        mock_exec.submit.side_effect = make_future

        def worker(idx: int) -> None:
            nonlocal call_count
            try:
                barrier.wait()
                result = render_psd_image(tmp_path / f"concurrent_{idx}.psd", 20.0, 2, 2)
                results[idx] = result
                with call_lock:
                    call_count += 1
            except Exception as exc:
                results[idx] = exc
                with call_lock:
                    call_count += 1

        threads = [threading.Thread(target=worker, args=(i,), daemon=True) for i in range(5)]

        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=10)

    # All 5 should have completed
    assert call_count == 5, f"Only {call_count}/5 threads completed"
    # All results should be None (worker returned None)
    for i, r in enumerate(results):
        assert r is None, f"Thread {i} got unexpected result: {r!r}"
    # submit should have been called 5 times
    assert mock_exec.submit.call_count == 5


def test_render_psd_image_worker_returns_valid_bytes(tmp_path: Path) -> None:
    """render_psd_image returns a PIL Image when the worker returns valid PNG bytes."""
    import io

    from PIL import Image as PILImage
    from tarragon.thumbnail import render_psd_image

    # Create a real PNG image as bytes
    dummy_img = PILImage.new("RGBA", (50, 50), (255, 0, 0, 128))
    buf = io.BytesIO()
    dummy_img.save(buf, "PNG")
    png_bytes = buf.getvalue()

    with patch("tarragon.thumbnail._get_executor") as mock_get_exec:
        mock_exec = MagicMock()
        mock_get_exec.return_value = mock_exec
        mock_future = MagicMock()
        # Simulate a future that completes on first poll
        mock_future.done.return_value = False
        mock_future.result.return_value = png_bytes
        mock_exec.submit.return_value = mock_future

        result = render_psd_image(tmp_path / "success.psd", 20.0, 2, 2)

        assert result is not None
        assert isinstance(result, PILImage.Image)
        assert result.size == (50, 50)
        assert result.mode == "RGBA"
        mock_exec.submit.assert_called_once()


def test_render_psd_image_with_tiny_psd_file(tmp_path: Path) -> None:
    """render_psd_image composits a real tiny PSD file successfully.

    This is an integration test that uses the actual ProcessPoolExecutor
    and psd-tools library.
    """
    from psd_tools import PSDImage
    from tarragon.thumbnail import render_psd_image

    psd_path = tmp_path / "tiny_test.psd"
    psd = PSDImage.new(mode="RGBA", size=(10, 10))
    psd.save(str(psd_path))

    result = render_psd_image(psd_path, 20.0, 2, 2)

    assert result is not None, "render_psd_image returned None for a valid tiny PSD"
    assert result.size == (10, 10), f"Expected (10,10) got {result.size}"
    assert result.mode == "RGBA", f"Expected RGBA got {result.mode}"

    # Clean up the executor that was created
    from tarragon.thumbnail import _shutdown_executor

    _shutdown_executor()


def test_render_psd_image_resizes_large_composite(tmp_path: Path) -> None:
    """render_psd_image resizes composited output when target_size is specified."""
    from psd_tools import PSDImage
    from tarragon.thumbnail import MASTER_LONG_EDGE, render_psd_image

    psd_path = tmp_path / "large_test.psd"
    # Create a PSD larger than MASTER_LONG_EDGE (2048)
    psd = PSDImage.new(mode="RGBA", size=(4000, 3000))
    psd.save(str(psd_path))

    result = render_psd_image(psd_path, 20.0, 2, 2, target_size=MASTER_LONG_EDGE)

    assert result is not None, "render_psd_image returned None for a large PSD"
    assert max(result.size) <= MASTER_LONG_EDGE, f"Result too large: {result.size} > {MASTER_LONG_EDGE}"
    # Aspect ratio should be preserved
    orig_ratio = 4000 / 3000
    result_ratio = result.size[0] / result.size[1]
    assert abs(result_ratio - orig_ratio) < 0.01, f"Aspect ratio changed: {result_ratio} != {orig_ratio}"

    # Clean up the executor
    from tarragon.thumbnail import _shutdown_executor

    _shutdown_executor()


def test_render_psd_image_invalid_path_in_subprocess(tmp_path: Path) -> None:
    """render_psd_image returns None when the sub-process worker gets an invalid path."""
    from tarragon.thumbnail import render_psd_image

    # Non-existent file — the worker (in subprocess) will try to open it and fail
    result = render_psd_image(tmp_path / "i_do_not_exist_at_all.psd", 20.0, 2, 2)
    assert result is None

    from tarragon.thumbnail import _shutdown_executor

    _shutdown_executor()


def test_atexit_handler_not_crashing_when_executor_was_never_created() -> None:
    """_shutdown_executor (registered via atexit) is safe when executor never started."""
    import tarragon.thumbnail as _tmod

    saved = _tmod._shared_executor
    _tmod._shared_executor = None
    try:
        # Simulate atexit calling shutdown when executor was never created
        _tmod._shutdown_executor()
        assert _tmod._shared_executor is None
    finally:
        _tmod._shared_executor = saved


# =========================================================================
# generate_cache_uuid tests
# =========================================================================


def test_generate_cache_uuid_returns_8_char_hex() -> None:
    """generate_cache_uuid returns an 8-character lowercase hex string."""
    from tarragon.thumbnail import generate_cache_uuid

    uuid = generate_cache_uuid()
    assert len(uuid) == 8
    assert all(c in "0123456789abcdef" for c in uuid)


def test_generate_cache_uuid_unique() -> None:
    """generate_cache_uuid returns unique values across 100 calls."""
    from tarragon.thumbnail import generate_cache_uuid

    uuids = [generate_cache_uuid() for _ in range(100)]
    assert len(set(uuids)) == 100  # All unique


# =========================================================================
# generate_cache_paths tests
# =========================================================================


def test_generate_cache_paths_structure(tmp_path: Path) -> None:
    """generate_cache_paths returns correct paths and creates directories."""
    from tarragon.thumbnail import RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL, generate_cache_paths

    source = Path("/photos/vacation/sunset.jpg")

    with patch("tarragon.thumbnail.cache_dir", return_value=tmp_path):
        paths = generate_cache_paths(source, "abc12345")

    assert str(RESOLUTION_THUMBNAIL) in paths
    assert str(RESOLUTION_PREVIEW) in paths
    assert "full" in paths

    # Check structure: cache/{resolution}/{folder}_{uuid}/{filename}.png
    assert paths[str(RESOLUTION_THUMBNAIL)] == tmp_path / str(RESOLUTION_THUMBNAIL) / "vacation_abc12345" / "sunset.png"
    assert paths[str(RESOLUTION_PREVIEW)] == tmp_path / str(RESOLUTION_PREVIEW) / "vacation_abc12345" / "sunset.png"
    assert paths["full"] == tmp_path / "full" / "vacation_abc12345" / "sunset.png"

    # Directories created
    assert (tmp_path / str(RESOLUTION_THUMBNAIL) / "vacation_abc12345").exists()
    assert (tmp_path / str(RESOLUTION_PREVIEW) / "vacation_abc12345").exists()
    assert (tmp_path / "full" / "vacation_abc12345").exists()


# =========================================================================
# derive_smaller_sizes tests
# =========================================================================


def test_derive_smaller_sizes_no_upscaling() -> None:
    """derive_smaller_sizes includes small images as-is (no upscaling) for all target sizes."""
    from tarragon.thumbnail import RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL, derive_smaller_sizes

    small_img = Image.new("RGB", (100, 100))
    result = derive_smaller_sizes(small_img, [RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW])
    # All target sizes are included — image is copied as-is, not upscaled
    assert set(result.keys()) == {RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW}
    assert result[RESOLUTION_THUMBNAIL].size == (100, 100)  # Original size preserved
    assert result[RESOLUTION_PREVIEW].size == (100, 100)  # Original size preserved
    # Verify they are copies, not the same object
    assert result[RESOLUTION_THUMBNAIL] is not small_img
    assert result[RESOLUTION_PREVIEW] is not small_img


def test_derive_smaller_sizes_correct_sizes() -> None:
    """derive_smaller_sizes produces correctly sized images preserving aspect ratio."""
    from tarragon.thumbnail import RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL, derive_smaller_sizes

    large_img = Image.new("RGB", (2000, 1500))
    result = derive_smaller_sizes(large_img, [RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW])

    assert RESOLUTION_THUMBNAIL in result
    assert RESOLUTION_PREVIEW in result
    assert result[RESOLUTION_THUMBNAIL].size == (256, 192)  # Aspect ratio preserved
    assert result[RESOLUTION_PREVIEW].size == (1024, 768)
