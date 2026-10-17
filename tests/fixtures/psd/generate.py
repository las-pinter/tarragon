#!/usr/bin/env python3
"""Generate the PSD/PSB fixture corpus.

The tool is psd-tools. See tests/fixtures/psd/SOURCES.md for the tool, its
version, its license, and the full fixture list.

Usage:
    python3 tests/fixtures/psd/generate.py
        Write the nine committed fixtures into tests/fixtures/psd/.

    python3 tests/fixtures/psd/generate.py --large-psb PATH
        Write the large PSB for CI into PATH. Do not commit that file.

    python3 tests/fixtures/psd/generate.py --verify-only
        Reopen each committed fixture with psd-tools and check its shape.
"""

from __future__ import annotations

import argparse
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

import psd_tools
from psd_tools import PSDImage
from psd_tools.constants import (
    BlendMode,
    ChannelID,
    Clipping,
    ColorMode,
    Compression,
    SectionDivider,
    Tag,
)
from psd_tools.psd import (
    FileHeader,
    ImageResources,
    LayerAndMaskInformation,
    PSD,
)
from psd_tools.psd.image_data import ImageData
from psd_tools.psd.layer_and_mask import (
    ChannelData,
    ChannelDataList,
    ChannelImageData,
    ChannelInfo,
    LayerInfo,
    LayerRecord,
    LayerRecords,
    MaskData,
    MaskFlags,
)

FIXTURE_DIR = Path(__file__).resolve().parent

# The large PSB is larger than the 8192 px cap and larger than the 20 MP
# large-canvas threshold, so CI exercises both limits.
LARGE_PSB_EDGE = 9000

RGB = ColorMode.RGB
CMYK = ColorMode.CMYK


def build_channels(width: int, height: int, depth: int, count: int, pick) -> list[bytes]:
    """Build `count` raw big-endian channel planes.

    `pick(x, y)` returns a tuple of `count` integer samples.
    """
    planes = [bytearray() for _ in range(count)]
    mask = 0xFF if depth == 8 else 0xFFFF
    for y in range(height):
        for x in range(width):
            values = pick(x, y)
            for index in range(count):
                value = int(values[index]) & mask
                if depth == 8:
                    planes[index].append(value)
                else:
                    planes[index] += struct.pack(">H", value)
    raw = [bytes(plane) for plane in planes]
    expected = width * height * (1 if depth == 8 else 2)
    assert all(len(plane) == expected for plane in raw)
    return raw


def solid_channels(width: int, height: int, depth: int, values: tuple[int, ...]) -> list[bytes]:
    return build_channels(width, height, depth, len(values), lambda _x, _y: values)


def rgb_gradient(x: int, y: int) -> tuple[int, int, int]:
    return ((x * 4) & 0xFF, (y * 4) & 0xFF, ((x + y) * 2) & 0xFF)


@dataclass
class LayerSpec:
    """One layer record plus its channel planes."""

    name: str
    rect: tuple[int, int, int, int]
    channel_ids: list[int]
    channel_raws: list[bytes]
    blend: BlendMode = BlendMode.NORMAL
    opacity: int = 255
    clipping: Clipping = Clipping.BASE
    divider: SectionDivider | None = None
    mask: tuple[tuple[int, int, int, int], bytes] | None = None


@dataclass
class DocumentSpec:
    """A complete PSD/PSB document."""

    width: int
    height: int
    depth: int
    color_mode: ColorMode
    version: int
    merged_channels: list[bytes]
    layers: list[LayerSpec] = field(default_factory=list)


def make_layer(spec: LayerSpec, depth: int, version: int) -> tuple[LayerRecord, ChannelDataList]:
    top, left, bottom, right = spec.rect
    width = right - left
    height = bottom - top
    record = LayerRecord(top=top, left=left, bottom=bottom, right=right)
    record.blend_mode = spec.blend
    record.opacity = spec.opacity
    record.clipping = spec.clipping
    record.name = spec.name
    record.tagged_blocks.set_data(Tag.UNICODE_LAYER_NAME, spec.name)
    if spec.divider is not None:
        record.tagged_blocks.set_data(
            Tag.SECTION_DIVIDER_SETTING,
            kind=spec.divider,
            signature=b"8BIM",
            blend_mode=spec.blend,
        )

    record.channel_info = [
        ChannelInfo(id=channel_id, length=0) for channel_id in spec.channel_ids
    ]
    data_list = []
    for raw in spec.channel_raws:
        channel = ChannelData(compression=Compression.RLE)
        channel.set_data(raw, width, height, depth, version)
        data_list.append(channel)

    if spec.mask is not None:
        mask_rect, mask_raw = spec.mask
        m_top, m_left, m_bottom, m_right = mask_rect
        record.mask_data = MaskData(
            top=m_top,
            left=m_left,
            bottom=m_bottom,
            right=m_right,
            background_color=0,
            flags=MaskFlags(),
        )
        record.channel_info.append(
            ChannelInfo(id=ChannelID.USER_LAYER_MASK, length=0)
        )
        mask_channel = ChannelData(compression=Compression.RLE)
        mask_channel.set_data(
            mask_raw, m_right - m_left, m_bottom - m_top, depth, version
        )
        data_list.append(mask_channel)

    return record, ChannelDataList(data_list)


def assemble(path: Path, spec: DocumentSpec) -> None:
    """Write one PSD (version 1) or PSB (version 2) file."""
    header = FileHeader(
        version=spec.version,
        channels=len(spec.merged_channels),
        height=spec.height,
        width=spec.width,
        depth=spec.depth,
        color_mode=spec.color_mode,
    )
    document = PSD(header=header, image_resources=ImageResources.new())
    document.image_data = ImageData(compression=Compression.RLE)
    document.image_data.set_data(spec.merged_channels, header)

    if spec.layers:
        records = []
        channel_blocks = []
        for layer in spec.layers:
            record, data_list = make_layer(layer, spec.depth, spec.version)
            records.append(record)
            channel_blocks.append(data_list)
        document.layer_and_mask_information = LayerAndMaskInformation(
            layer_info=LayerInfo(
                layer_count=len(records),
                layer_records=LayerRecords(records),
                channel_image_data=ChannelImageData(channel_blocks),
            )
        )
    with path.open("wb") as handle:
        document.write(handle)


# --------------------------------------------------------------------------
# Fixture builders
# --------------------------------------------------------------------------


def spec_rgb_8bit() -> DocumentSpec:
    size = 64
    return DocumentSpec(
        width=size,
        height=size,
        depth=8,
        color_mode=RGB,
        version=1,
        merged_channels=build_channels(size, size, 8, 3, rgb_gradient),
        layers=[
            LayerSpec(
                name="Background",
                rect=(0, 0, size, size),
                channel_ids=[0, 1, 2],
                channel_raws=solid_channels(size, size, 8, (200, 60, 30)),
            )
        ],
    )


def spec_rgb_16bit() -> DocumentSpec:
    size = 48
    return DocumentSpec(
        width=size,
        height=size,
        depth=16,
        color_mode=RGB,
        version=1,
        merged_channels=build_channels(
            size, size, 16, 3, lambda x, y: (x * 800, y * 800, (x + y) * 400)
        ),
        layers=[
            LayerSpec(
                name="Deep 16-bit",
                rect=(0, 0, size, size),
                channel_ids=[0, 1, 2],
                channel_raws=solid_channels(size, size, 16, (0x1234, 0x5678, 0x9ABC)),
            )
        ],
    )


def spec_cmyk_8bit() -> DocumentSpec:
    size = 64
    return DocumentSpec(
        width=size,
        height=size,
        depth=8,
        color_mode=CMYK,
        version=1,
        merged_channels=solid_channels(size, size, 8, (30, 60, 90, 10)),
        layers=[
            LayerSpec(
                name="CMYK Ink",
                rect=(0, 0, size, size),
                channel_ids=[0, 1, 2, 3],
                channel_raws=solid_channels(size, size, 8, (200, 40, 120, 20)),
            )
        ],
    )


def spec_layered_blend_modes() -> DocumentSpec:
    size = 96
    merged = build_channels(size, size, 8, 3, rgb_gradient)
    merged.append(b"\xff" * (size * size))
    modes = [
        ("Normal", BlendMode.NORMAL, (220, 40, 40)),
        ("Multiply", BlendMode.MULTIPLY, (40, 200, 60)),
        ("Screen", BlendMode.SCREEN, (40, 60, 220)),
        ("Overlay", BlendMode.OVERLAY, (230, 210, 40)),
        ("Linear Dodge", BlendMode.LINEAR_DODGE, (40, 210, 210)),
    ]
    layers = [
        LayerSpec(
            name=name,
            rect=(0, 0, size, size),
            channel_ids=[0, 1, 2, ChannelID.TRANSPARENCY_MASK],
            channel_raws=solid_channels(size, size, 8, (*color, 160)),
            blend=mode,
            opacity=200,
        )
        for name, mode, color in modes
    ]
    return DocumentSpec(size, size, 8, RGB, 1, merged, layers)


def spec_layer_groups() -> DocumentSpec:
    size = 96
    # psd-tools reads a group as a BOUNDING_SECTION_DIVIDER record first, then
    # the child records, then the OPEN_FOLDER record that carries the name.
    layers = [
        LayerSpec(
            name="Background",
            rect=(0, 0, size, size),
            channel_ids=[0, 1, 2],
            channel_raws=solid_channels(size, size, 8, (30, 30, 30)),
        ),
        LayerSpec(
            name="Group 1",
            rect=(0, 0, 0, 0),
            channel_ids=[],
            channel_raws=[],
            blend=BlendMode.PASS_THROUGH,
            divider=SectionDivider.BOUNDING_SECTION_DIVIDER,
        ),
        LayerSpec(
            name="Child A",
            rect=(0, 0, size, size),
            channel_ids=[0, 1, 2],
            channel_raws=solid_channels(size, size, 8, (200, 80, 40)),
        ),
        LayerSpec(
            name="Child B",
            rect=(0, 0, size, size),
            channel_ids=[0, 1, 2],
            channel_raws=solid_channels(size, size, 8, (40, 120, 200)),
        ),
        LayerSpec(
            name="Group 1",
            rect=(0, 0, 0, 0),
            channel_ids=[],
            channel_raws=[],
            blend=BlendMode.PASS_THROUGH,
            divider=SectionDivider.OPEN_FOLDER,
        ),
    ]
    return DocumentSpec(
        size, size, 8, RGB, 1, build_channels(size, size, 8, 3, rgb_gradient), layers
    )


def spec_clipping_mask() -> DocumentSpec:
    size = 96
    half = size // 2
    layers = [
        LayerSpec(
            name="Base",
            rect=(0, 0, size, size),
            channel_ids=[0, 1, 2],
            channel_raws=solid_channels(size, size, 8, (200, 200, 200)),
        ),
        LayerSpec(
            name="Clip A",
            rect=(0, 0, size, size),
            channel_ids=[0, 1, 2, ChannelID.TRANSPARENCY_MASK],
            channel_raws=solid_channels(size, size, 8, (220, 60, 60, 255)),
            clipping=Clipping.NON_BASE,
        ),
        LayerSpec(
            name="Clip B",
            rect=(0, 0, half, half),
            channel_ids=[0, 1, 2, ChannelID.TRANSPARENCY_MASK],
            channel_raws=solid_channels(half, half, 8, (60, 60, 220, 255)),
            clipping=Clipping.NON_BASE,
        ),
    ]
    return DocumentSpec(
        size, size, 8, RGB, 1, build_channels(size, size, 8, 3, rgb_gradient), layers
    )


def spec_layer_mask() -> DocumentSpec:
    size = 64
    mask_raw = build_channels(size, size, 8, 1, lambda x, _y: ((x * 4) & 0xFF,))[0]
    layer = LayerSpec(
        name="Masked",
        rect=(0, 0, size, size),
        channel_ids=[0, 1, 2, ChannelID.TRANSPARENCY_MASK],
        channel_raws=solid_channels(size, size, 8, (120, 180, 90, 255)),
        mask=((0, 0, size, size), mask_raw),
    )
    return DocumentSpec(
        size, size, 8, RGB, 1, build_channels(size, size, 8, 3, rgb_gradient), [layer]
    )


def spec_psb_8bit() -> DocumentSpec:
    size = 64
    return DocumentSpec(
        size,
        size,
        8,
        RGB,
        2,
        build_channels(size, size, 8, 3, rgb_gradient),
        [
            LayerSpec(
                name="PSB Layer",
                rect=(0, 0, size, size),
                channel_ids=[0, 1, 2],
                channel_raws=solid_channels(size, size, 8, (10, 120, 200)),
            )
        ],
    )


# --------------------------------------------------------------------------
# Corpus
# --------------------------------------------------------------------------


@dataclass
class Fixture:
    filename: str
    case: str
    build: Callable[[], DocumentSpec] | None = None


FIXTURES: list[Fixture] = [
    Fixture("rgb_8bit.psd", "8-bit RGB PSD", spec_rgb_8bit),
    Fixture("rgb_16bit.psd", "16-bit RGB PSD", spec_rgb_16bit),
    Fixture("cmyk_8bit.psd", "CMYK PSD", spec_cmyk_8bit),
    Fixture(
        "layered_blend_modes.psd",
        "Layered PSD with common blend modes",
        spec_layered_blend_modes,
    ),
    Fixture("layer_groups.psd", "Layer groups", spec_layer_groups),
    Fixture("clipping_mask.psd", "Clipping masks", spec_clipping_mask),
    Fixture("layer_mask.psd", "Layer mask", spec_layer_mask),
    Fixture("psb_8bit.psb", "One PSB (8-bit RGB, version 2)", spec_psb_8bit),
    Fixture("truncated.psd", "One truncated file", None),
]

# The truncated case is derived from this valid fixture and cut at this offset.
TRUNCATED_SOURCE = "layered_blend_modes.psd"
TRUNCATED_BYTES = 200


def build_corpus(out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    for fixture in FIXTURES:
        target = out_dir / fixture.filename
        if fixture.build is None:
            continue
        assemble(target, fixture.build())

    data = (out_dir / TRUNCATED_SOURCE).read_bytes()
    (out_dir / "truncated.psd").write_bytes(data[:TRUNCATED_BYTES])


def build_large_psb(path: Path) -> None:
    """Write the CI large PSB. The file is not committed."""
    edge = LARGE_PSB_EDGE
    spec = DocumentSpec(
        width=edge,
        height=edge,
        depth=8,
        color_mode=RGB,
        version=2,
        merged_channels=solid_channels(edge, edge, 8, (12, 34, 56)),
        layers=[],
    )
    assemble(path, spec)


# --------------------------------------------------------------------------
# Verification
# --------------------------------------------------------------------------


def verify_fixture(path: Path) -> str:
    """Return a short shape report for a committed fixture."""
    if path.name == "truncated.psd":
        header = path.read_bytes()[:4]
        assert header == b"8BPS", f"{path.name}: bad signature {header!r}"
        try:
            PSDImage.open(str(path))
        except Exception as error:  # noqa: BLE001 - any read failure is the point
            return f"truncated; header ok; read failed with {type(error).__name__}"
        raise AssertionError(f"{path.name}: truncated file opened without error")

    image = PSDImage.open(str(path))
    parts = [
        f"{image.width}x{image.height}",
        f"mode={image.color_mode.name}",
        f"depth={image.depth}",
        f"channels={image.channels}",
        f"layers={len(image)}",
    ]

    def describe(layer) -> str:
        flags = []
        if layer.is_group():
            flags.append("group")
        if layer.has_mask():
            flags.append("mask")
        if layer.clipping_layer:
            flags.append("clip")
        flag_text = ("[" + ",".join(flags) + "]") if flags else ""
        text = f"{layer.name}:{layer.blend_mode.name}{flag_text}"
        if layer.is_group():
            children = " ".join(describe(child) for child in layer)
            text += "(" + children + ")"
        return text

    for layer in image:
        parts.append(describe(layer))
    return " ".join(parts)


def verify_corpus(out_dir: Path) -> None:
    for fixture in FIXTURES:
        path = out_dir / fixture.filename
        if not path.exists():
            raise SystemExit(f"missing fixture: {path}")
        report = verify_fixture(path)
        size = path.stat().st_size
        if size >= 5 * 1024 * 1024:
            raise SystemExit(f"{path.name}: {size} bytes is not under 5 MB")
        print(f"{fixture.filename:28} {size:>8} bytes  {report}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Build the PSD/PSB fixture corpus.")
    parser.add_argument("--out", type=Path, default=FIXTURE_DIR)
    parser.add_argument("--large-psb", type=Path, default=None)
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args(argv)

    print(f"psd-tools {psd_tools.__version__}")
    if args.large_psb is not None:
        build_large_psb(args.large_psb)
        print(f"wrote large PSB: {args.large_psb} ({args.large_psb.stat().st_size} bytes)")
        return 0

    if not args.verify_only:
        build_corpus(args.out)
    verify_corpus(args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
