"""Tests for the cache"""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest
from PIL import Image


class TestSaveToCache:
    """Tests for saving to the cache"""

    def test_save_to_cache_writes_valid_png(self, tmp_path: Path) -> None:
        """save_to_cache writes a valid PNG file that can be re-opened."""
        from tarragon.renderers.cache import save_to_cache

        img = Image.new("RGBA", (100, 100), color="green")
        cache_path = tmp_path / "output.png"

        save_to_cache(img, cache_path)

        assert cache_path.is_file()
        assert cache_path.stat().st_size > 0

        loaded = Image.open(cache_path)
        assert loaded.size == (100, 100)
        assert loaded.mode == "RGBA"

    def test_save_to_cache_creates_parent_directories(self, tmp_path: Path) -> None:
        """save_to_cache creates intermediate directories when they don't exist."""
        from tarragon.renderers.cache import save_to_cache

        img = Image.new("RGBA", (50, 50), color="blue")
        cache_path = tmp_path / "deep" / "nested" / "output.png"

        save_to_cache(img, cache_path)

        assert cache_path.is_file()
        assert cache_path.parent.is_dir()
        assert cache_path.parent.parent.is_dir()

    def test_save_to_cache_jpeg_saves_rgb(self, tmp_path: Path) -> None:
        """save_to_cache with format_setting='jpeg' produces an RGB JPEG."""
        from tarragon.renderers.cache import save_to_cache

        img = Image.new("RGBA", (100, 100), (255, 0, 0, 128))
        cache_path = tmp_path / "output.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        assert cache_path.is_file()
        assert cache_path.stat().st_size > 0

        loaded = Image.open(cache_path)
        assert loaded.mode == "RGB"

    def test_save_to_cache_jpeg_flattens_alpha(self, tmp_path: Path) -> None:
        """save_to_cache with 'jpeg' replaces transparent areas with white."""
        from tarragon.renderers.cache import save_to_cache

        # Fully transparent red pixel on a red background — after flattening
        # on white, the result should be white (255, 255, 255).
        img = Image.new("RGBA", (100, 100), (255, 0, 0, 0))
        cache_path = tmp_path / "output.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        loaded = Image.open(cache_path)
        pixel = loaded.getpixel((50, 50))
        assert pixel == (255, 255, 255), f"Expected white pixel, got {pixel}"

    def test_save_to_cache_jpeg_handles_rgb_input(self, tmp_path: Path) -> None:
        """save_to_cache with 'jpeg' works when the input image is already RGB."""
        from tarragon.renderers.cache import save_to_cache

        img = Image.new("RGB", (50, 50), color="green")
        cache_path = tmp_path / "output.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        assert loaded.mode == "RGB"
        assert loaded.size == (50, 50)

    def test_save_to_cache_png_handles_rgba(self, tmp_path: Path) -> None:
        """save_to_cache with default PNG preserves RGBA transparency."""
        from tarragon.renderers.cache import save_to_cache

        # Semi-transparent red
        img = Image.new("RGBA", (50, 50), (255, 0, 0, 64))
        cache_path = tmp_path / "output.png"

        save_to_cache(img, cache_path)

        loaded = Image.open(cache_path)
        assert loaded.mode == "RGBA"
        # Check that alpha is preserved (not flattened)
        r, g, b, a = loaded.getpixel((25, 25))
        assert a == 64, f"Expected alpha 64, got {a}"


class TestSaveToCacheEdgeCases:
    def test_save_to_cache_unknown_format_defaults_to_png(self, tmp_path: Path) -> None:
        """save_to_cache with an unknown format_setting defaults to PNG saving behavior."""
        from tarragon.renderers.cache import save_to_cache

        img = Image.new("RGBA", (50, 50), (255, 0, 0, 128))
        cache_path = tmp_path / "output.unknown"

        # format_setting that is neither "png" nor "jpeg" falls through to else (PNG)
        save_to_cache(img, cache_path, format_setting="webp")

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        # PNG path was taken, so RGBA is preserved
        assert loaded.mode == "RGBA"

    def test_save_to_cache_parent_is_file_not_directory(self, tmp_path: Path) -> None:
        """save_to_cache raises an error when the parent of cache_path is a file, not a directory."""
        from tarragon.renderers.cache import save_to_cache

        # Create a file where we'd expect a directory
        parent_file = tmp_path / "i_am_a_file"
        parent_file.write_text("not a directory")
        cache_path = parent_file / "output.png"

        img = Image.new("RGB", (10, 10), color="red")

        with pytest.raises((OSError, NotADirectoryError)):
            save_to_cache(img, cache_path)

    def test_save_to_cache_jpeg_with_grayscale_image(self, tmp_path: Path) -> None:
        """save_to_cache with 'jpeg' saves a grayscale image as RGB JPEG."""
        from tarragon.renderers.cache import save_to_cache

        img = Image.new("L", (50, 50), color=128)
        cache_path = tmp_path / "grayscale.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        # JPEG save path takes L → .convert("RGB") → RGB JPEG
        assert loaded.mode == "RGB"

    def test_save_to_cache_jpeg_with_palette_image(self, tmp_path: Path) -> None:
        """save_to_cache with 'jpeg' converts palette image to RGB before saving."""
        from tarragon.renderers.cache import save_to_cache

        img = Image.new("P", (50, 50), color=0)
        cache_path = tmp_path / "palette.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        assert loaded.mode == "RGB"

    def test_save_to_cache_png_with_grayscale_image(self, tmp_path: Path) -> None:
        """save_to_cache with default PNG saves any image mode as PNG."""
        from tarragon.renderers.cache import save_to_cache

        img = Image.new("L", (50, 50), color=128)
        cache_path = tmp_path / "grayscale.png"

        save_to_cache(img, cache_path)

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        # PNG path saves whatever mode the image is in
        assert loaded.size == (50, 50)


class TestGenerateCacheUUID:
    def test_generate_cache_uuid_returns_8_char_hex(self) -> None:
        """generate_cache_uuid returns an 8-character lowercase hex string."""
        from tarragon.renderers.cache import generate_cache_uuid

        uuid = generate_cache_uuid()
        assert len(uuid) == 8
        assert all(c in "0123456789abcdef" for c in uuid)

    def test_generate_cache_uuid_unique(self) -> None:
        """generate_cache_uuid returns unique values across 100 calls."""
        from tarragon.renderers.cache import generate_cache_uuid

        uuids = [generate_cache_uuid() for _ in range(100)]
        assert len(set(uuids)) == 100  # All unique


class TestGenerateCachePaths:
    def test_generate_cache_paths_structure(self, tmp_path: Path) -> None:
        """generate_cache_paths returns correct paths and creates directories."""
        from tarragon.renderers.cache import RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL, generate_cache_paths

        source = Path("/photos/vacation/sunset.jpg")

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            paths = generate_cache_paths(source, "abc12345")

        assert str(RESOLUTION_THUMBNAIL) in paths
        assert str(RESOLUTION_PREVIEW) in paths
        assert "full" in paths

        # Check structure: cache/{resolution}/{folder}_{uuid}/{filename}.png
        assert (
            paths[str(RESOLUTION_THUMBNAIL)]
            == tmp_path / str(RESOLUTION_THUMBNAIL) / "vacation_abc12345" / "sunset.png"
        )
        assert paths[str(RESOLUTION_PREVIEW)] == tmp_path / str(RESOLUTION_PREVIEW) / "vacation_abc12345" / "sunset.png"
        assert paths["full"] == tmp_path / "full" / "vacation_abc12345" / "sunset.png"

        # Directories created
        assert (tmp_path / str(RESOLUTION_THUMBNAIL) / "vacation_abc12345").exists()
        assert (tmp_path / str(RESOLUTION_PREVIEW) / "vacation_abc12345").exists()
        assert (tmp_path / "full" / "vacation_abc12345").exists()


class TestDeriveSmallerSizes:
    def test_derive_smaller_sizes_no_upscaling(self) -> None:
        """derive_smaller_sizes includes small images as-is (no upscaling) for all target sizes."""
        from tarragon.renderers.cache import RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL, derive_smaller_sizes

        small_img = Image.new("RGB", (100, 100))
        result = derive_smaller_sizes(small_img, [RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW])
        # All target sizes are included — image is copied as-is, not upscaled
        assert set(result.keys()) == {RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW}
        assert result[RESOLUTION_THUMBNAIL].size == (100, 100)  # Original size preserved
        assert result[RESOLUTION_PREVIEW].size == (100, 100)  # Original size preserved
        # Verify they are copies, not the same object
        assert result[RESOLUTION_THUMBNAIL] is not small_img
        assert result[RESOLUTION_PREVIEW] is not small_img

    def test_derive_smaller_sizes_correct_sizes(self) -> None:
        """derive_smaller_sizes produces correctly sized images preserving aspect ratio."""
        from tarragon.renderers.cache import RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL, derive_smaller_sizes

        large_img = Image.new("RGB", (2000, 1500))
        result = derive_smaller_sizes(large_img, [RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW])

        assert RESOLUTION_THUMBNAIL in result
        assert RESOLUTION_PREVIEW in result
        assert result[RESOLUTION_THUMBNAIL].size == (256, 192)  # Aspect ratio preserved
        assert result[RESOLUTION_PREVIEW].size == (1024, 768)
