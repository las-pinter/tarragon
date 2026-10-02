"""Tests for render_clip_image"""

from __future__ import annotations

import io
import sqlite3
from pathlib import Path
from typing import Any

from PIL import Image

from tarragon.renderers.clip import _SQLITE_SCAN_CHUNK_SIZE, render_clip_image


def _make_clip_file(
    dest: Path,
    png_bytes: bytes | None = None,
    image_size: tuple[int, int] = (100, 80),
    image_color: str = "red",
    header: bytes = b"CSFCHUNK" + b"\x00" * 16,
    create_table: bool = True,
    insert_null: bool = False,
    insert_no_png_sig: bool = False,
) -> Path:
    """Create a synthetic .clip file with an embedded SQLite database.

    Parameters
    ----------
    dest:
        File path to write the .clip file to.
    png_bytes:
        Raw PNG bytes to insert as ImageData.  When *None*, a real PNG is
        generated from a solid-colour PIL image of *image_size*.
    image_size:
        Dimensions of the auto-generated PNG (ignored when *png_bytes* given).
    image_color:
        Fill colour for the auto-generated PNG.
    header:
        Binary header prepended before the SQLite data.
    create_table:
        When *False*, the CanvasPreview table is NOT created.
    insert_null:
        When *True*, insert NULL as ImageData instead of PNG bytes.
    insert_no_png_sig:
        When *True*, insert valid non-PNG bytes (no PNG signature) as ImageData.
    """
    # Generate real PNG bytes if not provided
    if png_bytes is None:
        img = Image.new("RGBA", image_size, color=image_color)
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        png_bytes = buf.getvalue()

    # Build the SQLite database in-memory
    db_path = dest.with_suffix(".sqlite")
    conn = sqlite3.connect(str(db_path))
    if create_table:
        conn.execute("CREATE TABLE CanvasPreview (ImageData BLOB)")
        if insert_null:
            conn.execute("INSERT INTO CanvasPreview (ImageData) VALUES (NULL)")
        elif insert_no_png_sig:
            # Valid data but no PNG signature - e.g. raw JPEG-like bytes
            conn.execute(
                "INSERT INTO CanvasPreview (ImageData) VALUES (?)",
                (b"\xff\xd8\xff\xe0" + b"\x00" * 100,),
            )
        else:
            conn.execute(
                "INSERT INTO CanvasPreview (ImageData) VALUES (?)",
                (png_bytes,),
            )
    conn.commit()
    conn.close()

    # Read the database bytes and prepend the fake header
    sqlite_data = db_path.read_bytes()
    db_path.unlink()  # Clean up temp sqlite file

    clip_data = header + sqlite_data
    dest.write_bytes(clip_data)
    return dest


class TestThumbnailExtraction:
    """Valid .clip files render thumbnails from embedded image data."""

    def test_render_clip_image_extracts_thumbnail(self, tmp_path: Path) -> None:
        """A valid .clip file yields a thumbnail with the embedded image size."""
        clip_path = _make_clip_file(
            tmp_path / "test.clip",
            image_size=(100, 80),
            image_color="blue",
        )

        result = render_clip_image(clip_path)

        assert result is not None
        assert isinstance(result, Image.Image)
        assert result.size == (100, 80)
        assert result.mode in ("RGB", "RGBA")

    def test_render_clip_image_resize(self, tmp_path: Path) -> None:
        """The target_size option resizes the thumbnail while preserving the aspect ratio."""
        clip_path = _make_clip_file(
            tmp_path / "resize.clip",
            image_size=(200, 160),
        )

        result = render_clip_image(clip_path, target_size=50)

        assert result is not None
        assert max(result.size) == 50
        ratio = result.size[0] / result.size[1]
        assert abs(ratio - 200 / 160) < 0.05, f"Aspect ratio changed: {ratio}"


class TestStreamingRead:
    """Streaming header scan: bounded-memory reads and chunk-boundary handling."""

    def test_render_clip_image_header_at_late_offset(self, tmp_path: Path) -> None:
        """A header located after ~2 scan chunks of junk still extracts correctly."""
        # 600 KiB of non-header bytes puts the SQLite header into the third chunk.
        junk = b"\x00" * 600_000
        clip_path = _make_clip_file(
            tmp_path / "late_offset.clip",
            image_size=(100, 80),
            image_color="green",
            header=junk + b"CSFCHUNK" + b"\x00" * 16,
        )

        result = render_clip_image(clip_path)

        assert result is not None
        assert isinstance(result, Image.Image)
        assert result.size == (100, 80)
        assert result.mode in ("RGB", "RGBA")

    def test_render_clip_image_header_straddles_chunk_boundary(self, tmp_path: Path) -> None:
        """A SQLite header spanning a scan-chunk boundary is still detected.

        The embedded database begins 10 bytes before the end of the first
        scan chunk, so the 16-byte header magic straddles the boundary and
        can only be found with the overlap window.
        """
        junk = b"\x00" * (_SQLITE_SCAN_CHUNK_SIZE - 10)
        clip_path = _make_clip_file(
            tmp_path / "straddle.clip",
            image_size=(64, 64),
            image_color="blue",
            header=junk,
        )

        result = render_clip_image(clip_path)

        assert result is not None
        assert result.size == (64, 64)

    def test_render_clip_image_never_reads_whole_file(self, tmp_path: Path, monkeypatch: Any) -> None:
        """The renderer streams the file; a failing whole-file read must not matter."""
        clip_path = _make_clip_file(
            tmp_path / "stream.clip",
            image_size=(100, 80),
            image_color="red",
        )

        def fail_read_bytes(_self: Path) -> bytes:
            raise AssertionError("Path.read_bytes must not be called")

        monkeypatch.setattr(Path, "read_bytes", fail_read_bytes)

        result = render_clip_image(clip_path)

        assert result is not None
        assert result.size == (100, 80)


class TestUnusableFiles:
    """Missing or structurally invalid .clip files yield None."""

    def test_render_clip_image_missing_file(self, tmp_path: Path) -> None:
        """A missing file yields None."""
        nonexistent = tmp_path / "does_not_exist.clip"

        result = render_clip_image(nonexistent)

        assert result is None

    def test_render_clip_image_no_sqlite_header(self, tmp_path: Path) -> None:
        """A file without an embedded SQLite database yields None."""
        clip_path = tmp_path / "no_sqlite.clip"
        clip_path.write_bytes(b"THIS IS JUST RANDOM GARBAGE DATA WITH NO SQLITE HEADER")

        result = render_clip_image(clip_path)

        assert result is None

    def test_render_clip_image_no_canvas_preview_table(self, tmp_path: Path) -> None:
        """A database without the CanvasPreview table yields None."""
        clip_path = _make_clip_file(
            tmp_path / "no_table.clip",
            create_table=False,
        )

        result = render_clip_image(clip_path)

        assert result is None


class TestInvalidImageData:
    """Invalid embedded image data yields None."""

    def test_render_clip_image_empty_image_data(self, tmp_path: Path) -> None:
        """NULL ImageData yields None."""
        clip_path = _make_clip_file(
            tmp_path / "null_data.clip",
            insert_null=True,
        )

        result = render_clip_image(clip_path)

        assert result is None

    def test_render_clip_image_no_png_signature(self, tmp_path: Path) -> None:
        """ImageData without a PNG signature yields None."""
        clip_path = _make_clip_file(
            tmp_path / "no_png_sig.clip",
            insert_no_png_sig=True,
        )

        result = render_clip_image(clip_path)

        assert result is None
