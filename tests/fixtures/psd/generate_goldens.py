#!/usr/bin/env python3
"""Generate the PSD golden images for the compositor tests.

Each fixture opens with ``PSDImage.open``. The layer composite renders with
``image.composite(ignore_preview=True)``, which skips the merged preview and
composites the layers. The result converts to 8-bit RGBA with
``.convert("RGBA")``. The PNG writer uses ``compress_level=9`` and writes no
text chunks, so a second run writes byte-identical files.

The golden set is ``golden/manifest.txt``. This script and the Rust test read
that one file, so a stem cannot be added to one side only.

Usage:
    python3 tests/fixtures/psd/generate_goldens.py --check
        Regenerate every image in memory and compare it with the committed
        file. A missing golden file is an error. A pixel difference is an
        error. Both PNG files decode and compare as RGBA8 pixels, so a Pillow
        or zlib change that keeps the pixels does not fail the check.

    python3 tests/fixtures/psd/generate_goldens.py --write
        Write every golden PNG file.
"""

from __future__ import annotations

import argparse
import io
from pathlib import Path

from PIL import Image
from psd_tools import PSDImage

HERE = Path(__file__).resolve().parent
GOLDEN_DIR = HERE / "golden"
MANIFEST = GOLDEN_DIR / "manifest.txt"


def golden_stems() -> tuple[str, ...]:
    """The golden stems from the one manifest that both sides read."""
    stems = []
    for line in MANIFEST.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            stems.append(line)
    return tuple(stems)


def fixture_for(stem: str) -> Path:
    """The corpus file for one stem. The one PSB fixture ends in `.psb`."""
    psd = HERE / f"{stem}.psd"
    return psd if psd.exists() else HERE / f"{stem}.psb"


def render(fixture: Path) -> bytes:
    """Render one fixture to deterministic RGBA PNG bytes."""
    image = PSDImage.open(str(fixture))
    merged = image.composite(ignore_preview=True)
    if merged is None:
        raise SystemExit(f"{fixture.name}: psd-tools returned no composite")
    composite = merged.convert("RGBA")
    buffer = io.BytesIO()
    composite.save(buffer, format="PNG", compress_level=9, optimize=False)
    return buffer.getvalue()


def target(stem: str) -> Path:
    """The committed path for one golden image."""
    return GOLDEN_DIR / f"{stem}.png"


def pixels(png: bytes) -> tuple[tuple[int, int], bytes]:
    """Decode one PNG to its size and its RGBA8 pixel bytes."""
    with Image.open(io.BytesIO(png)) as image:
        rgba = image.convert("RGBA")
        return rgba.size, rgba.tobytes()


def check() -> int:
    """Regenerate in memory and fail on a missing file or a pixel difference."""
    stems = golden_stems()
    failures = 0
    for stem in stems:
        path = target(stem)
        if not path.exists():
            print(f"missing golden: {path}")
            failures += 1
            continue
        want = pixels(render(fixture_for(stem)))
        got = pixels(path.read_bytes())
        if got != want:
            detail = "sizes" if got[0] != want[0] else "pixels"
            print(f"stale golden: {path}: {detail} differ from psd-tools 1.9.30")
            failures += 1
    if failures:
        print(
            f"{failures} of {len(stems)} golden file(s) failed the check;"
            " run --write and commit the result"
        )
        return 1
    print(f"all {len(stems)} golden files match psd-tools 1.9.30")
    return 0


def write() -> int:
    """Write every golden image."""
    GOLDEN_DIR.mkdir(parents=True, exist_ok=True)
    for stem in golden_stems():
        path = target(stem)
        path.write_bytes(render(fixture_for(stem)))
        print(f"wrote {path}")
    return 0


def main() -> int:
    """Run the requested mode. The default mode is `--check`."""
    parser = argparse.ArgumentParser(description="Generate the PSD golden images.")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--check",
        action="store_true",
        help="regenerate in memory and fail on a missing file or a pixel change",
    )
    mode.add_argument("--write", action="store_true", help="write the golden files")
    args = parser.parse_args()
    if args.write:
        return write()
    return check()


if __name__ == "__main__":
    raise SystemExit(main())
