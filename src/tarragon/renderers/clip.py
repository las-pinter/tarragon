"""CLIP image rendering — extract thumbnails from Clip Studio Paint .clip files."""

from __future__ import annotations

import io
import logging
import sqlite3
import tempfile
from pathlib import Path

from PIL import Image

logger = logging.getLogger(__name__)

# PNG signature and IEND chunk marker for extracting PNG from binary blobs
_PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
_IEND_CHUNK_END = b"IEND"
_SQLITE_HEADER = b"SQLite format 3\x00"


def render_clip_image(
    file_path: Path,
    target_size: int | None = None,
) -> Image.Image | None:
    """Extract a thumbnail from a Clip Studio Paint .clip file.

    A .clip file contains an embedded SQLite3 database with a
    ``CanvasPreview`` table that stores a PNG preview in its
    ``ImageData`` column.  This function locates the SQLite portion
    of the file, queries the preview, and returns it as a PIL Image.

    Parameters
    ----------
    file_path:
        Path to the .clip source file.
    target_size:
        If specified, shrink the image so the longest side is at most
        *target_size* pixels.  If ``None``, render at full resolution.

    Returns
    -------
    Image.Image | None
        The extracted preview image, or ``None`` on any failure
        (file not found, no SQLite header, missing table, corrupt data, …).
    """
    try:
        data = file_path.read_bytes()
    except Exception:
        logger.warning("render_clip_image: cannot read file %s", file_path)
        return None

    # Locate the embedded SQLite database within the .clip binary
    sqlite_offset = data.find(_SQLITE_HEADER)
    if sqlite_offset == -1:
        logger.warning("render_clip_image: no SQLite header found in %s", file_path)
        return None

    sqlite_data = data[sqlite_offset:]

    # Write the SQLite portion to a temp file so sqlite3 can open it
    tmp = None
    try:
        tmp = tempfile.NamedTemporaryFile(suffix=".sqlite", delete=False)
        tmp.write(sqlite_data)
        tmp.close()

        conn = sqlite3.connect(tmp.name)
        try:
            cursor = conn.execute("SELECT ImageData FROM CanvasPreview LIMIT 1")
            row = cursor.fetchone()
        finally:
            conn.close()
    except Exception:
        logger.warning(
            "render_clip_image: failed to query CanvasPreview in %s", file_path
        )
        return None
    finally:
        if tmp is not None:
            try:
                Path(tmp.name).unlink(missing_ok=True)
            except Exception:
                pass

    if row is None or row[0] is None:
        logger.warning("render_clip_image: no ImageData found in %s", file_path)
        return None

    blob = row[0]

    # Extract the PNG from the blob — it may contain extra bytes before/after
    png_start = blob.find(_PNG_SIGNATURE)
    if png_start == -1:
        logger.warning("render_clip_image: no PNG signature in ImageData of %s", file_path)
        return None

    iend_pos = blob.find(_IEND_CHUNK_END, png_start)
    if iend_pos == -1:
        logger.warning("render_clip_image: no IEND chunk in ImageData of %s", file_path)
        return None

    # IEND is 4 bytes for "IEND" + 4 bytes CRC after it
    png_end = iend_pos + len(_IEND_CHUNK_END) + 4
    png_bytes = blob[png_start:png_end]

    try:
        img = Image.open(io.BytesIO(png_bytes))
        img.load()  # Force decode so we catch corrupt PNG data now
    except Exception:
        logger.warning("render_clip_image: failed to decode PNG from %s", file_path)
        return None

    if img.mode not in ("RGBA", "RGB"):
        img = img.convert("RGBA")

    if target_size is not None:
        img.thumbnail((target_size, target_size), Image.LANCZOS)

    return img
