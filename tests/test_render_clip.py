"""Tests for render_clip_image — thumbnail extraction from Clip Studio .clip files."""

from __future__ import annotations

import io
import sqlite3
from pathlib import Path

import pytest
from PIL import Image

from tarragon.thumbnail import render_clip_image


# =========================================================================
# Helper — build a synthetic .clip file
# =========================================================================


def _make_clip_file(
    dest: Path,
    png_bytes: bytes | None = None,
    image_size: tuple[int, int] = (100, 80),
    image_color: str = "red",
    header: bytes = b"CSFCHUNK" + b"\x00" * 16,
    create_table: bool = True,
    insert_null: bool = False,
    insert_garbage: bool = False,
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
    insert_garbage:
        When *True*, insert garbage bytes (not a valid PNG) as ImageData.
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
        elif insert_garbage:
            conn.execute(
                "INSERT INTO CanvasPreview (ImageData) VALUES (?)",
                (b"NOT_A_PNG_JUST_GARBAGE\x00\xff\xfe",),
            )
        elif insert_no_png_sig:
            # Valid data but no PNG signature — e.g. raw JPEG-like bytes
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


# =========================================================================
# Tests
# =========================================================================


def test_render_clip_image_extracts_thumbnail(tmp_path: Path) -> None:
    """render_clip_image extracts a valid PNG thumbnail from a synthetic .clip file."""
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


def test_render_clip_image_resize(tmp_path: Path) -> None:
    """render_clip_image resizes the image when target_size is specified."""
    clip_path = _make_clip_file(
        tmp_path / "resize.clip",
        image_size=(200, 160),
    )

    result = render_clip_image(clip_path, target_size=50)

    assert result is not None
    # One dimension should be 50 (the long side is shrunk to target_size)
    assert max(result.size) == 50
    # Aspect ratio should be preserved: 200/160 = 1.25
    ratio = result.size[0] / result.size[1]
    assert abs(ratio - 200 / 160) < 0.05, f"Aspect ratio changed: {ratio}"


def test_render_clip_image_missing_file(tmp_path: Path) -> None:
    """render_clip_image returns None when the file does not exist."""
    nonexistent = tmp_path / "does_not_exist.clip"

    result = render_clip_image(nonexistent)

    assert result is None


def test_render_clip_image_no_sqlite_header(tmp_path: Path) -> None:
    """render_clip_image returns None when the file has no embedded SQLite database."""
    clip_path = tmp_path / "no_sqlite.clip"
    clip_path.write_bytes(b"THIS IS JUST RANDOM GARBAGE DATA WITH NO SQLITE HEADER")

    result = render_clip_image(clip_path)

    assert result is None


def test_render_clip_image_no_canvas_preview_table(tmp_path: Path) -> None:
    """render_clip_image returns None when the SQLite database lacks CanvasPreview table."""
    clip_path = _make_clip_file(
        tmp_path / "no_table.clip",
        create_table=False,
    )

    result = render_clip_image(clip_path)

    assert result is None


def test_render_clip_image_empty_image_data(tmp_path: Path) -> None:
    """render_clip_image returns None when ImageData is NULL."""
    clip_path = _make_clip_file(
        tmp_path / "null_data.clip",
        insert_null=True,
    )

    result = render_clip_image(clip_path)

    assert result is None


def test_render_clip_image_corrupt_png(tmp_path: Path) -> None:
    """render_clip_image returns None when ImageData contains garbage (not a valid PNG)."""
    clip_path = _make_clip_file(
        tmp_path / "corrupt_png.clip",
        insert_garbage=True,
    )

    result = render_clip_image(clip_path)

    assert result is None


def test_render_clip_image_no_png_signature(tmp_path: Path) -> None:
    """render_clip_image returns None when ImageData has no PNG signature bytes."""
    clip_path = _make_clip_file(
        tmp_path / "no_png_sig.clip",
        insert_no_png_sig=True,
    )

    result = render_clip_image(clip_path)

    assert result is None
