"""Tests for the Krita (.kra) renderer"""

from __future__ import annotations

import io
import zipfile
from pathlib import Path

from PIL import Image

from tarragon.renderers.krita import render_kra_image


def _png_bytes(
    size: tuple[int, int] = (100, 80),
    color: str | int | tuple[int, ...] = "red",
    mode: str = "RGBA",
) -> bytes:
    """Encode a solid-colour image as PNG bytes."""
    img = Image.new(mode, size, color=color)
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue()


def _make_kra_file(dest: Path, entries: dict[str, bytes] | None = None) -> Path:
    """Create a synthetic .kra ZIP archive at dest containing the given entries."""
    if entries is None:
        entries = {}
    with zipfile.ZipFile(dest, "w") as zf:
        for name, data in entries.items():
            zf.writestr(name, data)
    return dest


class TestValidKraFiles:
    """Valid .kra archives render the embedded merged image."""

    def test_render_kra_image_extracts_merged_image(self, tmp_path: Path) -> None:
        """A .kra with both image entries yields the merged image."""
        kra_path = _make_kra_file(
            tmp_path / "valid.kra",
            {
                "mergedimage.png": _png_bytes((100, 80), "red"),
                "preview.png": _png_bytes((50, 40), "blue"),
            },
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert isinstance(result, Image.Image)
        assert result.size == (100, 80)
        assert result.mode == "RGBA"

    def test_render_kra_image_resize(self, tmp_path: Path) -> None:
        """The target_size option shrinks the image while preserving the aspect ratio."""
        kra_path = _make_kra_file(
            tmp_path / "resize.kra",
            {
                "mergedimage.png": _png_bytes((200, 160), "green"),
                "preview.png": _png_bytes((50, 40), "blue"),
            },
        )

        result = render_kra_image(kra_path, target_size=50)

        assert result is not None
        assert max(result.size) == 50
        ratio = result.size[0] / result.size[1]
        assert abs(ratio - 200 / 160) < 0.05


class TestModeNormalization:
    """render_kra_image normalizes image modes to RGB or RGBA."""

    def test_render_kra_image_keeps_rgba(self, tmp_path: Path) -> None:
        """An RGBA merged image stays RGBA."""
        kra_path = _make_kra_file(
            tmp_path / "rgba.kra",
            {
                "mergedimage.png": _png_bytes((20, 20), (255, 0, 0, 128), "RGBA"),
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.mode == "RGBA"

    def test_render_kra_image_keeps_rgb(self, tmp_path: Path) -> None:
        """An RGB merged image stays RGB."""
        kra_path = _make_kra_file(
            tmp_path / "rgb.kra",
            {
                "mergedimage.png": _png_bytes((20, 20), "red", "RGB"),
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.mode == "RGB"

    def test_render_kra_image_converts_grayscale_to_rgba(self, tmp_path: Path) -> None:
        """A grayscale merged image converts to RGBA."""
        kra_path = _make_kra_file(
            tmp_path / "grayscale.kra",
            {
                "mergedimage.png": _png_bytes((20, 20), 128, "L"),
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.mode == "RGBA"

    def test_render_kra_image_converts_grayscale_alpha_to_rgba(self, tmp_path: Path) -> None:
        """A grayscale-with-alpha merged image converts to RGBA."""
        kra_path = _make_kra_file(
            tmp_path / "grayscale_alpha.kra",
            {
                "mergedimage.png": _png_bytes((20, 20), (128, 128), "LA"),
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.mode == "RGBA"

    def test_render_kra_image_converts_palette_to_rgba(self, tmp_path: Path) -> None:
        """A palette merged image converts to RGBA."""
        kra_path = _make_kra_file(
            tmp_path / "palette.kra",
            {
                "mergedimage.png": _png_bytes((20, 20), 0, "P"),
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.mode == "RGBA"

    def test_render_kra_image_palette_with_transparency_keeps_alpha(self, tmp_path: Path) -> None:
        """A palette image with transparency converts to RGBA and keeps its alpha."""
        img = Image.new("P", (20, 20), 0)
        img.info["transparency"] = 0
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        kra_path = _make_kra_file(
            tmp_path / "palette_alpha.kra",
            {
                "mergedimage.png": buf.getvalue(),
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.mode == "RGBA"
        pixel = result.getpixel((0, 0))
        assert isinstance(pixel, tuple)
        assert pixel[3] == 0


class TestExifOrientation:
    """render_kra_image applies EXIF orientation correction."""

    def test_render_kra_image_applies_exif_orientation(self, tmp_path: Path) -> None:
        """An image with EXIF orientation 6 is rotated so the dimensions swap."""
        img = Image.new("RGB", (50, 100), color="red")
        exif = img.getexif()
        exif[0x0112] = 6
        buf = io.BytesIO()
        img.save(buf, format="PNG", exif=exif)
        kra_path = _make_kra_file(
            tmp_path / "exif.kra",
            {
                "mergedimage.png": buf.getvalue(),
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.size == (100, 50)


class TestUnusableFiles:
    """Missing or structurally invalid .kra files yield None."""

    def test_render_kra_image_missing_file(self, tmp_path: Path) -> None:
        """A missing file yields None."""
        result = render_kra_image(tmp_path / "does_not_exist.kra")

        assert result is None

    def test_render_kra_image_empty_file(self, tmp_path: Path) -> None:
        """An empty file yields None."""
        empty_path = tmp_path / "empty.kra"
        empty_path.write_text("")

        result = render_kra_image(empty_path)

        assert result is None

    def test_render_kra_image_not_a_zip(self, tmp_path: Path) -> None:
        """A file with random bytes that is not a ZIP yields None."""
        not_zip = tmp_path / "not_a_zip.kra"
        not_zip.write_bytes(b"THIS IS NOT A ZIP FILE AT ALL")

        result = render_kra_image(not_zip)

        assert result is None

    def test_render_kra_image_directory_path(self, tmp_path: Path) -> None:
        """A directory path yields None."""
        result = render_kra_image(tmp_path)

        assert result is None

    def test_render_kra_image_empty_zip(self, tmp_path: Path) -> None:
        """A ZIP archive with no entries yields None."""
        kra_path = _make_kra_file(tmp_path / "empty_zip.kra")

        result = render_kra_image(kra_path)

        assert result is None

    def test_render_kra_image_no_image_entries(self, tmp_path: Path) -> None:
        """A ZIP with only non-image entries yields None."""
        kra_path = _make_kra_file(
            tmp_path / "no_images.kra",
            {"mimetype": b"application/x-krita", "documentinfo.xml": b"<info/>"},
        )

        result = render_kra_image(kra_path)

        assert result is None


class TestEntrySelection:
    """Entry selection behavior for archives with a single image entry."""

    def test_render_kra_image_with_only_mergedimage_renders_merged_image(self, tmp_path: Path) -> None:
        """A .kra with only mergedimage.png renders the merged image."""
        kra_path = _make_kra_file(
            tmp_path / "merged_only.kra",
            {"mergedimage.png": _png_bytes((100, 80), "red")},
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.size == (100, 80)

    def test_render_kra_image_with_only_preview_falls_back_to_preview(self, tmp_path: Path) -> None:
        """A .kra with only preview.png falls back to the preview image."""
        kra_path = _make_kra_file(
            tmp_path / "preview_only.kra",
            {"preview.png": _png_bytes((50, 40), "blue")},
        )

        result = render_kra_image(kra_path)

        assert result is not None
        assert result.size == (50, 40)


class TestInvalidImageData:
    """Invalid embedded image data yields None."""

    def test_render_kra_image_corrupt_png(self, tmp_path: Path) -> None:
        """A PNG signature followed by garbage yields None."""
        kra_path = _make_kra_file(
            tmp_path / "corrupt.kra",
            {
                "mergedimage.png": b"\x89PNG\r\n\x1a\n" + b"\x00" * 100,
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is None

    def test_render_kra_image_truncated_png(self, tmp_path: Path) -> None:
        """A truncated PNG yields None."""
        png = _png_bytes()
        kra_path = _make_kra_file(
            tmp_path / "truncated.kra",
            {
                "mergedimage.png": png[: len(png) // 2],
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is None

    def test_render_kra_image_empty_png_entry(self, tmp_path: Path) -> None:
        """An empty mergedimage.png entry yields None."""
        kra_path = _make_kra_file(
            tmp_path / "empty_png.kra",
            {
                "mergedimage.png": b"",
                "preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is None


class TestEntryNameEdgeCases:
    """Entry name matching is exact and case-sensitive."""

    def test_render_kra_image_nested_mergedimage_returns_none(self, tmp_path: Path) -> None:
        """A nested mergedimage.png path does not match the root entry name."""
        kra_path = _make_kra_file(
            tmp_path / "nested.kra",
            {"subdir/mergedimage.png": _png_bytes()},
        )

        result = render_kra_image(kra_path)

        assert result is None

    def test_render_kra_image_uppercase_entry_names_returns_none(self, tmp_path: Path) -> None:
        """Uppercase entry names do not match the lowercase lookups."""
        kra_path = _make_kra_file(
            tmp_path / "uppercase.kra",
            {
                "MergedImage.png": _png_bytes(),
                "Preview.png": _png_bytes(),
            },
        )

        result = render_kra_image(kra_path)

        assert result is None
