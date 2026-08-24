"""Tests for the cache"""

from __future__ import annotations

import logging
from pathlib import Path
from unittest.mock import patch

import pytest
from PIL import Image

from tarragon.renderers.cache import (
    RESOLUTION_PREVIEW,
    RESOLUTION_THUMBNAIL,
    _is_safe_cache_dir,
    clear_cache,
    clear_full_res_cache,
    compute_cache_size_bytes,
    derive_smaller_sizes,
    generate_cache_paths,
    generate_cache_uuid,
    save_to_cache,
)


class TestSaveToCache:
    """Saving rendered images to the cache."""

    def test_save_to_cache_writes_valid_png(self, tmp_path: Path) -> None:
        """save_to_cache writes a valid PNG file that can be re-opened."""
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
        img = Image.new("RGBA", (50, 50), color="blue")
        cache_path = tmp_path / "deep" / "nested" / "output.png"

        save_to_cache(img, cache_path)

        assert cache_path.is_file()
        assert cache_path.parent.is_dir()
        assert cache_path.parent.parent.is_dir()

    def test_save_to_cache_jpeg_flattens_alpha(self, tmp_path: Path) -> None:
        """save_to_cache with 'jpeg' replaces transparent areas with white."""
        # Fully transparent red pixel on a red background - after flattening
        # on white, the result should be white (255, 255, 255).
        img = Image.new("RGBA", (100, 100), (255, 0, 0, 0))
        cache_path = tmp_path / "output.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        loaded = Image.open(cache_path)
        pixel = loaded.getpixel((50, 50))
        assert pixel == (255, 255, 255), f"Expected white pixel, got {pixel}"

    def test_save_to_cache_jpeg_handles_rgb_input(self, tmp_path: Path) -> None:
        """save_to_cache with 'jpeg' works when the input image is already RGB."""
        img = Image.new("RGB", (50, 50), color="green")
        cache_path = tmp_path / "output.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        assert loaded.mode == "RGB"
        assert loaded.size == (50, 50)

    def test_save_to_cache_png_handles_rgba(self, tmp_path: Path) -> None:
        """save_to_cache with default PNG preserves RGBA transparency."""
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
    """Edge cases when saving to the cache."""

    def test_save_to_cache_unknown_format_defaults_to_png(self, tmp_path: Path) -> None:
        """save_to_cache with an unknown format_setting defaults to PNG saving behavior."""
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
        # Create a file where we'd expect a directory
        parent_file = tmp_path / "i_am_a_file"
        parent_file.write_text("not a directory")
        cache_path = parent_file / "output.png"

        img = Image.new("RGB", (10, 10), color="red")

        with pytest.raises((OSError, NotADirectoryError)):
            save_to_cache(img, cache_path)

    def test_save_to_cache_jpeg_with_grayscale_image(self, tmp_path: Path) -> None:
        """save_to_cache with 'jpeg' saves a grayscale image as RGB JPEG."""
        img = Image.new("L", (50, 50), color=128)
        cache_path = tmp_path / "grayscale.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        # JPEG save path takes L -> .convert("RGB") -> RGB JPEG
        assert loaded.mode == "RGB"

    def test_save_to_cache_jpeg_with_palette_image(self, tmp_path: Path) -> None:
        """save_to_cache with 'jpeg' converts palette image to RGB before saving."""
        img = Image.new("P", (50, 50), color=0)
        cache_path = tmp_path / "palette.jpg"

        save_to_cache(img, cache_path, format_setting="jpeg")

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        assert loaded.mode == "RGB"

    def test_save_to_cache_png_with_grayscale_image(self, tmp_path: Path) -> None:
        """save_to_cache with default PNG saves any image mode as PNG."""
        img = Image.new("L", (50, 50), color=128)
        cache_path = tmp_path / "grayscale.png"

        save_to_cache(img, cache_path)

        assert cache_path.is_file()
        loaded = Image.open(cache_path)
        # PNG path saves whatever mode the image is in
        assert loaded.size == (50, 50)


class TestGenerateCacheUUID:
    """Generating unique cache UUIDs."""

    def test_generate_cache_uuid_returns_8_char_hex(self) -> None:
        """generate_cache_uuid returns an 8-character lowercase hex string."""
        uuid = generate_cache_uuid()
        assert len(uuid) == 8
        assert all(c in "0123456789abcdef" for c in uuid)

    def test_generate_cache_uuid_unique(self) -> None:
        """generate_cache_uuid returns unique values across 100 calls."""
        uuids = [generate_cache_uuid() for _ in range(100)]
        assert len(set(uuids)) == 100  # All unique


class TestGenerateCachePaths:
    """Generating cache file paths and directories."""

    def test_generate_cache_paths_structure(self, tmp_path: Path) -> None:
        """generate_cache_paths returns correct paths and creates directories."""
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
    """Deriving smaller resolution variants of an image."""

    def test_derive_smaller_sizes_no_upscaling(self) -> None:
        """derive_smaller_sizes includes small images as-is (no upscaling) for all target sizes."""
        small_img = Image.new("RGB", (100, 100))
        result = derive_smaller_sizes(small_img, [RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW])
        # All target sizes are included - image is copied as-is, not upscaled
        assert set(result.keys()) == {RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW}
        assert result[RESOLUTION_THUMBNAIL].size == (100, 100)  # Original size preserved
        assert result[RESOLUTION_PREVIEW].size == (100, 100)  # Original size preserved
        # Verify they are copies, not the same object
        assert result[RESOLUTION_THUMBNAIL] is not small_img
        assert result[RESOLUTION_PREVIEW] is not small_img

    def test_derive_smaller_sizes_correct_sizes(self) -> None:
        """derive_smaller_sizes produces correctly sized images preserving aspect ratio."""
        large_img = Image.new("RGB", (2000, 1500))
        result = derive_smaller_sizes(large_img, [RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW])

        assert RESOLUTION_THUMBNAIL in result
        assert RESOLUTION_PREVIEW in result
        assert result[RESOLUTION_THUMBNAIL].size == (256, 192)  # Aspect ratio preserved
        assert result[RESOLUTION_PREVIEW].size == (1024, 768)


class TestClearFullResCache:
    """Clearing the full-resolution cache tier."""

    def test_clear_full_res_cache_deletes_full_tree(self, tmp_path: Path) -> None:
        """clear_full_res_cache deletes all files under the full tier."""
        full_dir = tmp_path / "full"
        subdir = full_dir / "folder_abc12345"
        subdir.mkdir(parents=True)
        (subdir / "image.png").write_bytes(b"data")
        (full_dir / "stray.png").write_bytes(b"data")

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_full_res_cache()

        assert not (subdir / "image.png").exists()
        assert not (full_dir / "stray.png").exists()

    def test_clear_full_res_cache_skips_when_disabled(self, tmp_path: Path) -> None:
        """clear_full_res_cache does nothing when disabled."""
        full_dir = tmp_path / "full"
        full_dir.mkdir(parents=True)
        (full_dir / "image.png").write_bytes(b"data")

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_full_res_cache(enabled=False)

        assert (full_dir / "image.png").exists()

    def test_clear_full_res_cache_does_not_touch_other_tiers(self, tmp_path: Path) -> None:
        """clear_full_res_cache leaves other resolution tiers intact."""
        full_dir = tmp_path / "full"
        full_dir.mkdir(parents=True)
        (full_dir / "image.png").write_bytes(b"full")
        thumb_dir = tmp_path / str(RESOLUTION_THUMBNAIL)
        thumb_dir.mkdir(parents=True)
        (thumb_dir / "image.png").write_bytes(b"thumb")

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_full_res_cache()

        assert not (full_dir / "image.png").exists()
        assert (thumb_dir / "image.png").exists()

    def test_clear_full_res_cache_prunes_empty_subdirs(self, tmp_path: Path) -> None:
        """clear_full_res_cache removes now-empty subdirectories bottom-up."""
        full_dir = tmp_path / "full"
        subdir = full_dir / "folder_abc12345"
        subdir.mkdir(parents=True)
        (subdir / "image.png").write_bytes(b"data")

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_full_res_cache()

        assert not subdir.exists()
        assert full_dir.exists()

    def test_clear_full_res_cache_missing_dir_is_noop(self, tmp_path: Path) -> None:
        """clear_full_res_cache does nothing when the full dir does not exist."""
        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_full_res_cache()

        assert not (tmp_path / "full").exists()

    def test_clear_full_res_cache_refuses_non_full_dir(self, tmp_path: Path) -> None:
        """clear_full_res_cache refuses to clear when the resolved dir is not named 'full'."""
        target = tmp_path / "notfull"
        target.mkdir()
        (target / "image.png").write_bytes(b"data")
        (tmp_path / "full").symlink_to(target, target_is_directory=True)

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_full_res_cache()

        assert (target / "image.png").exists()

    def test_clear_full_res_cache_removes_symlink_but_keeps_target(self, tmp_path: Path) -> None:
        """clear_full_res_cache removes symlinks under full but leaves target files untouched."""
        full_dir = tmp_path / "full"
        full_dir.mkdir(parents=True)
        target = tmp_path / "outside.png"
        target.write_bytes(b"precious")
        link = full_dir / "link.png"
        link.symlink_to(target)

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_full_res_cache()

        assert not link.is_symlink()
        assert target.exists()
        assert target.read_bytes() == b"precious"

    def test_clear_full_res_cache_continues_after_permission_error(
        self, tmp_path: Path, caplog: pytest.LogCaptureFixture
    ) -> None:
        """clear_full_res_cache logs a warning and continues when unlink raises PermissionError."""
        full_dir = tmp_path / "full"
        subdir = full_dir / "folder_abc12345"
        subdir.mkdir(parents=True)
        blocked = subdir / "blocked.png"
        blocked.write_bytes(b"data")
        other = full_dir / "other.png"
        other.write_bytes(b"data")

        real_unlink = Path.unlink

        def flaky_unlink(path: Path, missing_ok: bool = False) -> None:
            """Raise PermissionError for blocked.png, otherwise delegate to the real unlink."""
            if path == blocked:
                raise PermissionError("Permission denied")
            real_unlink(path, missing_ok=missing_ok)

        with (
            caplog.at_level(logging.WARNING, logger="tarragon.renderers.cache"),
            patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path),
            patch.object(Path, "unlink", flaky_unlink),
        ):
            clear_full_res_cache()

        assert blocked.exists()
        assert not other.exists()
        assert "Failed to remove cache file" in caplog.text


class TestComputeCacheSize:
    """Computing the total on-disk size of the cache."""

    def test_compute_cache_size_bytes_sums_all_files(self, tmp_path: Path) -> None:
        """compute_cache_size_bytes sums file sizes across all resolution tiers."""
        for tier, size in (("256", 100), ("1024", 200), ("full", 300)):
            tier_dir = tmp_path / tier / "folder_abc12345"
            tier_dir.mkdir(parents=True)
            (tier_dir / "image.png").write_bytes(b"x" * size)

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            result = compute_cache_size_bytes()

        assert result == 600

    def test_compute_cache_size_bytes_empty_dir_returns_zero(self, tmp_path: Path) -> None:
        """compute_cache_size_bytes returns 0 for an existing but empty cache dir."""
        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            result = compute_cache_size_bytes()

        assert result == 0

    def test_compute_cache_size_bytes_missing_dir_returns_zero(self, tmp_path: Path) -> None:
        """compute_cache_size_bytes returns 0 when the cache dir does not exist."""
        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path / "missing"):
            result = compute_cache_size_bytes()

        assert result == 0


class TestClearCache:
    """Clearing the whole thumbnail cache."""

    def test_clear_cache_deletes_all_tiers(self, tmp_path: Path) -> None:
        """clear_cache deletes files in all three resolution tiers."""
        for tier in ("256", "1024", "full"):
            tier_dir = tmp_path / tier / "folder_abc12345"
            tier_dir.mkdir(parents=True)
            (tier_dir / "image.png").write_bytes(b"data")

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_cache()

        for tier in ("256", "1024", "full"):
            assert not (tmp_path / tier / "folder_abc12345" / "image.png").exists()

    def test_clear_cache_prunes_empty_subdirs(self, tmp_path: Path) -> None:
        """clear_cache removes now-empty subdirectories bottom-up but keeps the cache root."""
        subdir = tmp_path / "256" / "folder_abc12345"
        subdir.mkdir(parents=True)
        (subdir / "image.png").write_bytes(b"data")

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path):
            clear_cache()

        assert not subdir.exists()
        assert tmp_path.exists()

    def test_clear_cache_missing_dir_is_noop(self, tmp_path: Path) -> None:
        """clear_cache does nothing when the cache dir does not exist."""
        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path / "missing"):
            clear_cache()

        assert not (tmp_path / "missing").exists()

    def test_clear_cache_refuses_home_dir(self, tmp_path: Path) -> None:
        """clear_cache refuses to clear when the cache dir resolves to the home directory."""
        (tmp_path / "image.png").write_bytes(b"data")

        with (
            patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path),
            patch("tarragon.renderers.cache.Path.home", return_value=tmp_path),
        ):
            clear_cache()

        assert (tmp_path / "image.png").exists()

    def test_clear_cache_refuses_data_dir(self, tmp_path: Path) -> None:
        """clear_cache refuses to clear when the cache dir resolves to the data directory."""
        (tmp_path / "image.png").write_bytes(b"data")

        with (
            patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path),
            patch("tarragon.renderers.cache.data_dir", return_value=tmp_path),
        ):
            clear_cache()

        assert (tmp_path / "image.png").exists()

    def test_clear_cache_refuses_filesystem_root(self, tmp_path: Path) -> None:
        """clear_cache refuses to clear the filesystem root without error."""
        with (
            patch("tarragon.renderers.cache.cache_dir", return_value=Path("/")),
            patch("tarragon.renderers.cache._delete_tree_contents") as mock_delete,
        ):
            clear_cache()

        mock_delete.assert_not_called()

    def test_clear_cache_removes_symlink_but_keeps_target(self, tmp_path: Path) -> None:
        """clear_cache removes symlinks under the cache but leaves target files untouched."""
        cache_root = tmp_path / "cache"
        cache_root.mkdir()
        target = tmp_path / "outside.png"
        target.write_bytes(b"precious")
        link = cache_root / "link.png"
        link.symlink_to(target)

        with patch("tarragon.renderers.cache.cache_dir", return_value=cache_root):
            clear_cache()

        assert not link.is_symlink()
        assert target.exists()
        assert target.read_bytes() == b"precious"

    def test_clear_cache_continues_after_permission_error(
        self, tmp_path: Path, caplog: pytest.LogCaptureFixture
    ) -> None:
        """clear_cache logs a warning and continues when unlink raises PermissionError."""
        subdir = tmp_path / "256" / "folder_abc12345"
        subdir.mkdir(parents=True)
        blocked = subdir / "blocked.png"
        blocked.write_bytes(b"data")
        other = tmp_path / "other.png"
        other.write_bytes(b"data")

        real_unlink = Path.unlink

        def flaky_unlink(path: Path, missing_ok: bool = False) -> None:
            """Raise PermissionError for blocked.png, otherwise delegate to the real unlink."""
            if path == blocked:
                raise PermissionError("Permission denied")
            real_unlink(path, missing_ok=missing_ok)

        with (
            caplog.at_level(logging.WARNING, logger="tarragon.renderers.cache"),
            patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path),
            patch.object(Path, "unlink", flaky_unlink),
        ):
            clear_cache()

        assert blocked.exists()
        assert not other.exists()
        assert "Failed to remove cache file" in caplog.text


class TestIsSafeCacheDir:
    """Direct tests for the _is_safe_cache_dir safety checks."""

    def test_refuses_filesystem_root(self) -> None:
        """_is_safe_cache_dir returns False for the filesystem root."""
        assert _is_safe_cache_dir(Path("/")) is False

    def test_refuses_home_dir(self, tmp_path: Path) -> None:
        """_is_safe_cache_dir returns False for the home directory itself."""
        home = tmp_path / "home"
        with (
            patch("tarragon.renderers.cache.Path.home", return_value=home),
            patch("tarragon.renderers.cache.data_dir", return_value=tmp_path / "data"),
        ):
            assert _is_safe_cache_dir(home) is False

    def test_refuses_ancestor_of_home(self, tmp_path: Path) -> None:
        """_is_safe_cache_dir returns False for an ancestor of the home directory."""
        home = tmp_path / "home"
        with (
            patch("tarragon.renderers.cache.Path.home", return_value=home),
            patch("tarragon.renderers.cache.data_dir", return_value=tmp_path / "data"),
        ):
            assert _is_safe_cache_dir(tmp_path) is False

    def test_refuses_data_dir(self, tmp_path: Path) -> None:
        """_is_safe_cache_dir returns False for the data directory itself."""
        data = tmp_path / "data"
        with (
            patch("tarragon.renderers.cache.Path.home", return_value=tmp_path / "home"),
            patch("tarragon.renderers.cache.data_dir", return_value=data),
        ):
            assert _is_safe_cache_dir(data) is False

    def test_refuses_ancestor_of_data_dir(self, tmp_path: Path) -> None:
        """_is_safe_cache_dir returns False for an ancestor of the data directory."""
        data = tmp_path / "data"
        with (
            patch("tarragon.renderers.cache.Path.home", return_value=tmp_path / "home"),
            patch("tarragon.renderers.cache.data_dir", return_value=data),
        ):
            assert _is_safe_cache_dir(tmp_path) is False

    def test_allows_default_cache_dir(self, tmp_path: Path) -> None:
        """_is_safe_cache_dir allows the default data_dir()/cache path."""
        data = tmp_path / "data"
        with (
            patch("tarragon.renderers.cache.Path.home", return_value=tmp_path / "home"),
            patch("tarragon.renderers.cache.data_dir", return_value=data),
        ):
            assert _is_safe_cache_dir(data / "cache") is True

    def test_allows_child_of_home(self, tmp_path: Path) -> None:
        """_is_safe_cache_dir allows a child directory of the home directory."""
        home = tmp_path / "home"
        with (
            patch("tarragon.renderers.cache.Path.home", return_value=home),
            patch("tarragon.renderers.cache.data_dir", return_value=tmp_path / "data"),
        ):
            assert _is_safe_cache_dir(home / "custom_cache") is True

    def test_allows_deep_children(self, tmp_path: Path) -> None:
        """_is_safe_cache_dir allows deep nested cache directories."""
        with (
            patch("tarragon.renderers.cache.Path.home", return_value=tmp_path / "home"),
            patch("tarragon.renderers.cache.data_dir", return_value=tmp_path / "data"),
        ):
            assert _is_safe_cache_dir(tmp_path / "a" / "b" / "c" / "cache") is True
