#!/usr/bin/env python3
"""Generate the PSD/PSB fixture corpus.

The tool is psd-tools. See tests/fixtures/psd/SOURCES.md for the tool, its
version, its license, and the full fixture list.

Usage:
    python3 tests/fixtures/psd/generate.py
        Write the committed fixtures into tests/fixtures/psd/. Skip
        layer_effects.psd, because the Rust builder owns that file.

    python3 tests/fixtures/psd/generate.py --include-rust-built
        Write every committed fixture, including layer_effects.psd.

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
from psd_tools.psd.descriptor import (
    Bool,
    Descriptor,
    Double,
    Enumerated,
    Integer,
    List as DescriptorList,
    UnitFloat,
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
from psd_tools.terminology import Unit

FIXTURE_DIR = Path(__file__).resolve().parent

# The large PSB is larger than the 8192 px cap and larger than the 20 MP
# large-canvas threshold, so CI exercises both limits.
LARGE_PSB_EDGE = 9000

RGB = ColorMode.RGB
CMYK = ColorMode.CMYK


def build_channels(
    width: int, height: int, depth: int, count: int, pick
) -> list[bytes]:
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


def solid_channels(
    width: int, height: int, depth: int, values: tuple[int, ...]
) -> list[bytes]:
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
    # Object-based layer effects items, written as an `lfx2` block.
    effects: list | None = None


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


def make_layer(
    spec: LayerSpec, depth: int, version: int
) -> tuple[LayerRecord, ChannelDataList]:
    top, left, bottom, right = spec.rect
    width = right - left
    height = bottom - top
    record = LayerRecord(top=top, left=left, bottom=bottom, right=right)
    record.blend_mode = spec.blend
    record.opacity = spec.opacity
    record.clipping = spec.clipping
    record.name = spec.name
    record.tagged_blocks.set_data(Tag.UNICODE_LAYER_NAME, spec.name)
    if spec.effects is not None:
        record.tagged_blocks.set_data(
            Tag.OBJECT_BASED_EFFECTS_LAYER_INFO,
            version=1,
            data_version=16,
            classID=b"null",
            items=spec.effects,
        )
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
        record.channel_info.append(ChannelInfo(id=ChannelID.USER_LAYER_MASK, length=0))
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
# Layer effects
# --------------------------------------------------------------------------


def _descriptor(class_id: bytes, items: list) -> Descriptor:
    return Descriptor(name="", classID=class_id, items=items)


def _rgb(red: float, green: float, blue: float) -> Descriptor:
    return _descriptor(
        b"RGBC",
        [
            (b"Rd  ", Double(red)),
            (b"Grn ", Double(green)),
            (b"Bl  ", Double(blue)),
        ],
    )


def _common(blend: bytes = b"Nrml", opacity: float = 100.0) -> list:
    return [
        (b"enab", Bool(True)),
        (b"present", Bool(True)),
        (b"showInDialog", Bool(True)),
        (b"Md  ", Enumerated(typeID=b"BlnM", enum=blend)),
        (b"Opct", UnitFloat(Unit.Percent, opacity)),
    ]


def _effect(key: bytes, items: list) -> tuple[bytes, Descriptor]:
    return (key, _descriptor(key, items))


def _shadow(
    key: bytes,
    color: tuple[float, float, float],
    angle: float,
    distance: float,
    size: float,
    opacity: float,
) -> tuple[bytes, Descriptor]:
    items = _common(b"Mltp", opacity) + [
        (b"Clr ", _rgb(*color)),
        (b"lagl", UnitFloat(Unit.Angle, angle)),
        (b"uglg", Bool(False)),
        (b"Dstn", UnitFloat(Unit.Pixels, distance)),
        (b"Ckmt", UnitFloat(Unit.Percent, 0.0)),
        (b"blur", UnitFloat(Unit.Pixels, size)),
        (b"layerConceals", Bool(True)),
    ]
    return _effect(key, items)


def _glow(
    key: bytes,
    color: tuple[float, float, float],
    size: float,
    spread: float,
    opacity: float,
) -> tuple[bytes, Descriptor]:
    items = _common(b"Scrn", opacity) + [
        (b"Clr ", _rgb(*color)),
        (b"Ckmt", UnitFloat(Unit.Percent, spread)),
        (b"blur", UnitFloat(Unit.Pixels, size)),
        (b"Inpr", UnitFloat(Unit.Percent, 50.0)),
        (b"GlwT", Enumerated(typeID=b"GlwT", enum=b"SfBL")),
    ]
    return _effect(key, items)


def _stroke(color: tuple[float, float, float], size: float) -> tuple[bytes, Descriptor]:
    items = _common(b"Nrml", 100.0) + [
        (b"Sz  ", UnitFloat(Unit.Pixels, size)),
        (b"Styl", Enumerated(typeID=b"FStl", enum=b"InsF")),
        (b"PntT", Enumerated(typeID=b"FrFl", enum=b"SClr")),
        (b"Clr ", _rgb(*color)),
    ]
    return _effect(b"FrFX", items)


def _bevel(
    size: float,
    angle: float,
    highlight: tuple[float, float, float],
    shadow: tuple[float, float, float],
    opacity: float,
) -> tuple[bytes, Descriptor]:
    items = [
        (b"enab", Bool(True)),
        (b"bvlS", Enumerated(typeID=b"BvlS", enum=b"In  ")),
        (b"bvlT", Enumerated(typeID=b"BvlT", enum=b"SfBL")),
        (b"bvlD", Enumerated(typeID=b"BvlD", enum=b"In  ")),
        (b"srgR", UnitFloat(Unit.Percent, 100.0)),
        (b"blur", UnitFloat(Unit.Pixels, size)),
        (b"Sftn", UnitFloat(Unit.Pixels, 0.0)),
        (b"lagl", UnitFloat(Unit.Angle, angle)),
        (b"Lald", UnitFloat(Unit.Angle, 30.0)),
        (b"uglg", Bool(False)),
        (b"hglM", Enumerated(typeID=b"BlnM", enum=b"Scrn")),
        (b"hglO", UnitFloat(Unit.Percent, opacity)),
        (b"hglC", _rgb(*highlight)),
        (b"sdwM", Enumerated(typeID=b"BlnM", enum=b"Mltp")),
        (b"sdwO", UnitFloat(Unit.Percent, opacity)),
        (b"sdwC", _rgb(*shadow)),
    ]
    return _effect(b"ebbl", items)


def _satin(
    color: tuple[float, float, float], angle: float, distance: float, size: float
) -> tuple[bytes, Descriptor]:
    items = _common(b"Mltp", 50.0) + [
        (b"Clr ", _rgb(*color)),
        (b"lagl", UnitFloat(Unit.Angle, angle)),
        (b"Dstn", UnitFloat(Unit.Pixels, distance)),
        (b"blur", UnitFloat(Unit.Pixels, size)),
        (b"Invr", Bool(False)),
    ]
    return _effect(b"ChFX", items)


def _solid_overlay(
    color: tuple[float, float, float], opacity: float
) -> tuple[bytes, Descriptor]:
    items = _common(b"Nrml", opacity) + [(b"Clr ", _rgb(*color))]
    return _effect(b"SoFi", items)


def _gradient_overlay(angle: float) -> tuple[bytes, Descriptor]:
    def stop(location: int, midpoint: int, grey: float) -> Descriptor:
        return _descriptor(
            b"Clrt",
            [
                (b"Lctn", Integer(location)),
                (b"Clr ", _rgb(grey, grey, grey)),
                (b"Mdpn", Integer(midpoint)),
            ],
        )

    gradient = _descriptor(
        b"Grdn",
        [
            (b"Clrs", DescriptorList([stop(0, 50, 0.0), stop(4096, 50, 255.0)])),
            (b"Trns", DescriptorList([])),
        ],
    )
    items = _common(b"Nrml", 100.0) + [
        (b"Grad", gradient),
        (b"Angl", UnitFloat(Unit.Angle, angle)),
        (b"Type", Enumerated(typeID=b"GrdT", enum=b"Lnr ")),
        (b"Scl ", UnitFloat(Unit.Percent, 100.0)),
    ]
    return _effect(b"GrFl", items)


def spec_layer_effects() -> DocumentSpec:
    """Nine effect layers on a light backdrop, one effect kind per layer."""
    size = 192
    cells = [
        # name, cell column, cell row, base color, effect items
        (
            "Drop Shadow",
            0,
            0,
            (200, 60, 60),
            [_shadow(b"DrSh", (0.0, 0.0, 0.0), 120.0, 8.0, 6.0, 75.0)],
        ),
        (
            "Inner Shadow",
            1,
            0,
            (60, 120, 200),
            [_shadow(b"IrSh", (0.0, 0.0, 0.0), 120.0, 6.0, 5.0, 75.0)],
        ),
        (
            "Outer Glow",
            2,
            0,
            (60, 180, 90),
            [_glow(b"OrGl", (255.0, 220.0, 60.0), 8.0, 20.0, 75.0)],
        ),
        (
            "Inner Glow",
            0,
            1,
            (180, 140, 60),
            [_glow(b"IrGl", (255.0, 255.0, 255.0), 6.0, 0.0, 70.0)],
        ),
        ("Stroke", 1, 1, (120, 120, 220), [_stroke((255.0, 255.0, 255.0), 4.0)]),
        (
            "Bevel",
            2,
            1,
            (140, 140, 140),
            [_bevel(6.0, 120.0, (255.0, 255.0, 255.0), (0.0, 0.0, 0.0), 75.0)],
        ),
        ("Satin", 0, 2, (120, 120, 120), [_satin((0.0, 0.0, 0.0), 19.0, 6.0, 5.0)]),
        (
            "Color Overlay",
            1,
            2,
            (100, 100, 100),
            [_solid_overlay((255.0, 0.0, 0.0), 50.0)],
        ),
        ("Gradient Overlay", 2, 2, (100, 100, 100), [_gradient_overlay(0.0)]),
    ]
    layers = [
        LayerSpec(
            name="Backdrop",
            rect=(0, 0, size, size),
            channel_ids=[0, 1, 2],
            channel_raws=solid_channels(size, size, 8, (230, 230, 230)),
        )
    ]
    for name, column, row, color, effects in cells:
        left = 16 + column * 64
        top = 16 + row * 64
        layers.append(
            LayerSpec(
                name=name,
                rect=(top, left, top + 32, left + 32),
                channel_ids=[0, 1, 2, ChannelID.TRANSPARENCY_MASK],
                channel_raws=solid_channels(32, 32, 8, (*color, 255)),
                effects=effects,
            )
        )
    return DocumentSpec(
        size,
        size,
        8,
        RGB,
        1,
        solid_channels(size, size, 8, (200, 200, 200)),
        layers,
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
    Fixture("layer_effects.psd", "Layer effects", spec_layer_effects),
    Fixture("psb_8bit.psb", "One PSB (8-bit RGB, version 2)", spec_psb_8bit),
    Fixture("truncated.psd", "One truncated file", None),
]

# The truncated case is derived from this valid fixture and cut at this offset.
TRUNCATED_SOURCE = "layered_blend_modes.psd"
TRUNCATED_BYTES = 200


# The Rust builder owns this file. The generator skips it by default so a run
# does not overwrite the committed bytes.
RUST_BUILT_FIXTURES = {"layer_effects.psd"}


def build_corpus(out_dir: Path, include_rust_built: bool = False) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    for fixture in FIXTURES:
        target = out_dir / fixture.filename
        if fixture.build is None:
            continue
        if not include_rust_built and fixture.filename in RUST_BUILT_FIXTURES:
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
    parser.add_argument("--include-rust-built", action="store_true")
    args = parser.parse_args(argv)

    print(f"psd-tools {psd_tools.__version__}")
    if args.large_psb is not None:
        build_large_psb(args.large_psb)
        print(
            f"wrote large PSB: {args.large_psb} ({args.large_psb.stat().st_size} bytes)"
        )
        return 0

    if not args.verify_only:
        build_corpus(args.out, args.include_rust_built)
    verify_corpus(args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
