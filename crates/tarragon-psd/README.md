# tarragon-psd

The PSD and PSB parser, the merged preview reader, and the layer compositor
for tarragon. The crate reads the Photoshop binary format directly. It does
not use a `psd` crate (Q9).

This file is the documented approach for the parser and the layer compositor.
It records the task 17 spike result, the task 18 full tiled parser, and the
task 19 compositor.

## Status and scope

Task 18 delivers:

- A checked parser for the full layer-and-mask section: every layer record,
  layer masks, blending ranges, the extra data, the layer groups, the clipping
  flag, and the global mask.
- A checked parser for the image data section: the merged image and the per
  channel data, in RLE and raw form, with the PSD four-byte and PSB eight-byte
  length rules.
- A tiled read. `PsdDocument` owns the compressed file bytes. A region read
  decodes only the rows and columns a viewport needs. The full canvas is never
  decoded into memory.
- The full-tier cap. The long edge is clamped to 8192 px by
  `limits::full_tier_size`, and a single region decode is refused when either
  edge exceeds the cap.
- The 120 s deadline. `ParseOptions` carries a `Deadline`. An expired deadline
  returns `PsdError::Timeout`. The reader checks the deadline per record and
  per row.
- Bounds checks for `section_length` and `layer_info_length`. Every declared
  length is validated before an allocation. A length past the file end is a
  `Truncated` error, and a count that cannot fit is refused before the walk.

The parser never panics on bad input. A fuzz-style test walks prefixes and
bounded corrupt slices of the fixtures; every read returns a value or a typed
error.

Task 17 was the header and first-layer spike. This document keeps the spike
result as history; task 18 replaces it.

## Crate placement (Q16)

`tarragon-psd` depends on `tarragon-core` only. It never depends on the GUI or
the database. The parser error converts to the shared `RenderError` at the
composite boundary (`PsdError::into_render_error`).

## Binary section layout

A PSD or PSB file is one pass of these sections, in this order:

| Section | Length field | Content |
|---------|--------------|---------|
| Header | fixed 26 bytes | signature, version, channels, height, width, depth, color mode |
| Color mode data | 4 bytes | palette or mode data; usually empty |
| Image resources | 4 bytes | resource blocks such as ICC profile and resolution |
| Layer and mask information | 4 bytes in PSD, 8 bytes in PSB | layer info and global layer mask |
| Image data | none | the merged image, RLE or raw |

The header is 26 bytes:

| Field | Bytes | Meaning |
|-------|-------|---------|
| Signature | 4 | `8BPS` |
| Version | 2 | 1 is PSD, 2 is PSB |
| Reserved | 6 | must be zero |
| Channels | 2 | merged image channel count |
| Height | 4 | rows |
| Width | 4 | columns |
| Depth | 2 | bits per channel |
| Color mode | 2 | 3 is RGB, 4 is CMYK |

The layer info section starts with a layer count. A negative count marks
merged transparency. Then one layer record per layer. A layer record holds:

| Field | Bytes | Meaning |
|-------|-------|---------|
| Top, left, bottom, right | 4 each | layer rectangle |
| Channel count | 2 | number of channels |
| Channel info | 6 in PSD, 10 in PSB each | channel ID and data length |
| Blend signature | 4 | `8BIM` |
| Blend key | 4 | for example `norm` |
| Opacity, clipping, flags, filler | 1 each | layer flags |
| Extra data length | 4 | length of the extra data |
| Extra data | variable | mask, blending ranges, name, extra blocks |

The extra data holds the mask data, the blending ranges, a padded Pascal name,
and additional info blocks. The `luni` block holds the UTF-16BE unicode name.
The parser prefers the unicode name, as `psd-tools` does.

The channel image data follows all layer records, one block per channel, in
record order. The `ChannelInfo` length gives the block size. The parser
computes every layer channel file offset from that rule.

## The PSB 8-byte length rule

PSB files use eight-byte length fields where PSD files use four. The rule
applies to:

- The layer and mask information section length.
- The layer info section length.
- The channel data length in each layer record.

The color mode data length and the image resources length stay four bytes in
both versions. The reader selects the length-field width from the file
version, so a PSB file with four-byte fields misparses; the PSB fixture
proves the eight-byte read.

## Color modes

The parser reads the mode code for every file. The channel reader supports:

- RGB, 8-bit: three channels, one byte per sample.
- RGB, 16-bit: three channels, two big-endian bytes per sample.
- CMYK, 8-bit: four channels (C, M, Y, K), one byte per sample.

Other modes parse at the header level and stay unsupported for compositing
until a later task.

Channel IDs map as: 0 to 3 are color, -1 is the transparency mask, -2 is the
user layer mask.

## Tiled read plan (Q9)

The full parser decodes channel data in regions, never as one full native-size
buffer. The plan:

1. Read the header and the layer records. Keep the layer tree in memory.
2. For a viewport, clip it to the channel rectangle.
3. RLE rows are independent, so the row-length table gives direct access to
   each row group. The reader sums the table entries before the first needed
   row, then decodes only the needed rows.
4. The caller composites one tile at a time and frees the tile buffer after
   the copy. The compositor (task 19) owns this loop.
5. Composite effects on a padded tile, then crop to the tile edge (task 20).

The compositor-facing API:

```rust
let document = PsdDocument::open(path)?;                 // parsed structure
let options = ParseOptions::default();                   // 120 s deadline
let region = document.read_merged_region(viewport, &options)?;
let layer = document.read_layer_region(index, viewport, &options)?;
```

`Viewport::new(left, top, right, bottom)` uses exclusive right and bottom
edges. A returned `ImageRegion` covers exactly the viewport. Each
`ChannelRegion` carries row-major `u16` samples, so 8-bit and 16-bit samples
share one type. A pixel outside a channel rectangle is zero.

The whole compressed file is read into memory once. The decoded pixel buffer
is bounded by the viewport, not by the canvas. The per-row scratch buffer is
bounded by the channel width. This keeps the large PSB
(9000x9000, built in CI) from a full-canvas decode.

## Layer compositor (task 19)

`PsdDocument::composite_region` composites the layer tree over one viewport.
It returns a `CompositeRegion` with straight `f32` RGB and alpha. The `rgba8`
and `rgba16` methods encode the region. The compositor works in tiles. Every
buffer is viewport-sized, and the full canvas is never allocated.

The compositor reads the layers bottom to top and honors:

- Visibility. A hidden layer contributes nothing.
- Opacity and fill opacity. Fill opacity comes from the `iOpa` block.
- The 27 Q27 blend modes. The formulas follow `psd_tools/composite/blend.py`,
  with Photoshop's Vivid Light, Hard Mix, Color Burn and Color Dodge variants
  (a source extreme wins, near-extreme backdrop values count as exact).
  Dissolve falls back to normal, as `psd-tools` does.
- Layer masks. The `-2` mask channel scales the layer alpha. The mask default
  color fills outside the mask rectangle, and a disabled mask is skipped.
- Clipping masks. A clipped layer composites over the base color, and the base
  coverage restricts the result.
- Groups. A normal group composites onto an isolated buffer and then blends
  onto the backdrop. A pass-through group composites onto the backdrop
  directly.
- Adjustment and fill layers. A solid or gradient fill layer draws its content
  and composites it like a pixel layer. An adjustment layer transforms the
  backdrop, then blends the result back by the layer opacity and mask.
- CMYK. A CMYK layer converts to RGB with the non-ICC `R = (1 - C) * (1 - K)`
  formula, then blends in RGB. ICC parity is deferred (see below).
- 8-bit and 16-bit depth. Samples divide by 255 or 65535 into `f32`, so the
  same image at both depths gives the same 8-bit result. Posterize samples a
  depth-sized table (256 at 8-bit, 65536 at 16-bit); Curves sample a fixed
  256-entry table, and Levels apply directly per pixel.

Layer effects are task 20.

The fill opacity here is plain coverage: it multiplies the layer shape. The
reference also has the special-eight behavior (`psblend::fill_is_special`):
below 100% fill, eight blend modes pull the source toward the mode's neutral
color. This crate does not do that. Task 23 verifies the difference.

### Adjustment and fill layers

The parser reads the tagged blocks of a fill or adjustment layer and stores a
`LayerContent` on the layer record. The compositor then draws the fill or
applies the adjustment. The implemented set is:

- Solid color fill (`SoCo`): `RGBC`, `CMYC` and `Grsc` color descriptors.
- Gradient fill (`GdFl`): linear, radial, angle, reflected and diamond styles,
  color stops with midpoints, opacity stops, angle, scale, offset and reverse.
- Invert (`nvrt`), threshold (`thrs`), posterize (`post`).
- Levels (`levl`): the master record and the three channel records.
- Curves (`curv`): a natural cubic spline per channel, then the master curve.
- Brightness/contrast (`brit`): the legacy formula, or the modern `CgEd`
  descriptor when it is present.

The parser marks a recognized block it does not apply yet as unsupported, and
the compositor contributes nothing for that layer.

### Not in task 19

- Vector masks. The parser records the `vmsk` and `vsms` blocks as
  `LayerRecord::has_vector_mask`, but does not rasterize the path. An enabled
  vector mask is not applied; this is a documented gap.
- Adjustment blocks outside the core set. Hue/Saturation (`hue2`), Color
  Balance (`blnc`), Channel Mixer (`mixr`), Gradient Map (`grdm`), Photo Filter
  (`phfl`), Selective Color (`selc`), Black & White (`blwh`), Exposure
  (`expA`), Vibrance (`vibA`) and Color Lookup (`clrL`) parse as unsupported.
- Pattern fill (`PtFl`). It needs the document pattern table; the layer is
  unsupported.
- Real user masks. The compositor uses the `-2` user mask channel only. The
  `-3` real mask channel is not combined.
- Mask density and feather. The user mask density, the vector mask density and
  feather are not read; the mask applies at full density and no feather.
- Gradient smoothness and dither. The gradient stops interpolate linearly; the
  Photoshop smoothness and the `Dthr` dither are not applied.
- Gradient opacity midpoints. `parse_gradient_stops` and `sample_opacity`
  sample the opacity stops linearly and drop every `Mdpn`. The reference
  applies them with the same ending-stop convention (`photocraft`
  `crates/io/src/blocks.rs`, `smoothed_opacity_stops`). Task 23 settles the
  difference.
- Gradient frame. The canvas is the gradient frame. The reference narrows the
  frame to a mask or a vector-mask path when the fill is aligned with the
  layer; this crate does not.

### CMYK blend space

The reference reads a CMYK layer through the document's CMYK profile and
composites it through the profile (ICC) path. This crate does not: it converts
the CMYK channels to RGB with the non-ICC `R = (1 - C) * (1 - K)` formula and
blends in RGB. ICC parity is deferred. A future ICC task can change the blend
space without changing the public API.

### Attribution

The blend formulas, the layer compositor structure, the adjustment block
layouts and the descriptor reader follow the working reference **photocraft**
(GitHub `storytold/photocraft`, license MIT OR Apache-2.0), specifically
`crates/compose/src/psblend.rs`, `crates/compose/src/adjust.rs`,
`crates/compose/src/effects.rs`, `crates/compose/src/gradient_fill.rs`,
`crates/io/src/adjust_map.rs`, `crates/io/src/blocks.rs` and
`crates/psd/src/descriptor.rs`. Tarragon does not depend on the photocraft
crates, and the code here is its own implementation.

The gradient geometry and sampling in `crates/tarragon-psd/src/content.rs`
(`parameter_at`, `gradient_units`, `sample_stops`, `sample_opacity`,
`midpoint_remap`) is a recognizable port of `crates/compose/src/effects.rs`
(`gradient_t`, `gradient_units`) and `crates/compose/src/gradient_fill.rs`
(`Ramp`, `midpoint_remap`, `sample_color`, `sample_opacity`). The functions
were renamed and inlined into the compositor. That reference carries this
notice:

> Copyright (c) 2026 ArtCraft Team and the PhotoCraft contributors
>
> Licensed under the MIT License or the Apache License, Version 2.0, at your
> option (MIT OR Apache-2.0).

The Vivid Light, Hard Mix, Color Burn and Color Dodge variants, the compositor
pass-through and clipping semantics, the Levels gamma toe, and the
lightness/saturation non-separable formulas are recognizable ports of that
reference; the copyright notice stays with the reference project. See the Q52
decision record.

## Caps and deadline

Two limits stay from the Python renderer (Q9):

- The full-tier render caps the long edge at 8192 px. `limits::full_tier_size`
  clamps the target. A region read whose width or height exceeds 8192 returns
  `PsdError::CapExceeded`. The compositor reads source tiles at or below the
  cap and downscales them.
- The whole render has a 120 s deadline. `ParseOptions::default` sets it. The
  reader polls the deadline and stops. A timeout returns `PsdError::Timeout`,
  not a panic.

The parser checks every declared length before it allocates. A length beyond
the file size is a `Truncated` error. A length that overflows `usize` is a
`LengthTooLarge` error. The `section_length` bounds the record walk, and the
`layer_info_length` bounds the records plus the channel data block.

## Errors and the no-panic rule

`PsdError` has one variant per failure kind: `Io`, `BadSignature`,
`UnsupportedVersion`, `Truncated`, `BadBlendSignature`, `LengthTooLarge`,
`Timeout`, `UnsupportedColorMode`, `UnsupportedCompression`,
`InvalidViewport`, `CapExceeded`, `LayerOutOfRange`, and `BadDimensions`. The
reader uses checked arithmetic and checked slices. `into_render_error` maps
`CapExceeded` to the shared `RenderError::TooLarge` and the rest to a decode
reason. A fuzz-style test walks every prefix of a small file and a bounded
stride of a fixture; every read returns a value or a typed error, never a
panic.

## Tests and the golden oracle

The unit tests cover the reader, the RLE codec, the cap, the deadline, and the
error mapping. The integration tests in `tests/parse_fixtures.rs` read the
nine committed fixtures under `tests/fixtures/psd/`. They assert the exact
header and layer values for the eight valid fixtures, the group and clipping
flags, the layer mask rectangle, the merged and layer viewport pixels for
8-bit RGB, 16-bit RGB, and CMYK, the raw path on a synthetic file, the PSB
eight-byte lengths, the typed error for the truncated fixture, a rejected
corrupt section length, the cap guard, the expired deadline, and the no-panic
truncation sweep.

The compositor tests are in `tests/composite_fixtures.rs`. The fixture tests
cover the pass-through group, the clipping mask, the layer mask, and the CMYK
decode path. The synthetic tests cover the normal, multiply, screen, overlay,
and linear dodge modes, the Vivid Light and Hard Mix extremes, opacity, fill
opacity, a hidden layer, a pass-through group against an isolated group, 8-bit
against 16-bit, a vector mask parser hook, and the adjustment and fill set:
solid color, a masked fill, a gradient ramp, invert, levels, curves, legacy and
modern brightness/contrast, posterize, and threshold. The blend unit tests
cover every blend key, the analytic formulas, and the non-separable modes.

The corpus has no adjustment/fill fixture, no vector mask fixture, no hidden
layer, and no fill opacity value. It covers one pass-through group and no
normal group, and it covers four blend modes. The synthetic tests cover those
compositor features. Task 23 adds the `psd-tools` comparison.

`psd-tools` 1.9.30 is the golden oracle (Q27). Task 23 compares the compositor
output against it per feature. The parser does not need the oracle; the
fixtures already record the expected shape.

## Work split

| Task | Work |
|------|------|
| 17 | Header and first layer spike; this document |
| 18 | Full tiled parser; caps and deadline; this document |
| 19 | Layer compositor: blend modes, masks, groups, clipping, CMYK, 8/16-bit; adjustment/fill core; this document |
| 20 | Layer effects on padded tiles |
| 23 | Per-feature golden tests against `psd-tools` |
