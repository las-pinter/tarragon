# tarragon-psd

The PSD and PSB parser, the merged preview reader, and the layer compositor
for tarragon. The crate reads the Photoshop binary format directly. It does
not use a `psd` crate (Q9).

This file is the documented approach for study task 17, the parser spike. It
also records the plan for task 18 (tiled parser) and task 19 (layer
compositor).

## Status and scope

The spike delivers:

- A real, checked parser that reads the file header and the first layer record
  from each committed fixture.
- A typed error for malformed input. The parser never panics.
- This document: the full parser plan.

The spike does not composite pixels, read channel image data, or enforce the
caps. It loads the whole file into memory for the header and record walk.
Task 18 replaces that with a seekable reader that decodes one tile at a time.

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

The layer info section starts with a layer count. A negative count means the
merged alpha channel comes first. Then one layer record per layer. A layer
record holds:

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
computes each first-layer channel file offset from that rule.

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

The spike reads the mode code for every file. The compositor supports:

- RGB, 8-bit: three channels, one byte per sample.
- RGB, 16-bit: three channels, two big-endian bytes per sample.
- CMYK, 8-bit: four channels (C, M, Y, K), one byte per sample.

Other modes parse at the header level and stay unsupported for compositing
until a later task.

Channel IDs map as: 0 to 3 are color, -1 is the transparency mask, -2 is the
user layer mask.

## Tiled read plan (Q9)

The full parser reads channel data in tiles, never as one full native-size
buffer. The plan:

1. Read the header and the layer records. Keep the layer tree in memory.
2. For a viewport, compute the tile rectangles from the Q28 tile grid
   (`1x1` to `4x4`).
3. Decode only the channel rows a tile needs. RLE rows are independent, so a
   row offset table gives direct access to each row group.
4. Composite one tile at a time. Free the tile buffer after the copy.
5. Composite effects on a padded tile, then crop to the tile edge. The pad
   stops seams at tile boundaries (Q27 risk).
6. Write the tiles into the output image and return it.

The large PSB (9000x9000, built in CI) exercises this path.

## Caps and deadline

Two limits stay from the Python renderer (Q9):

- The full-tier render caps the long edge at 8192 px. A larger canvas is
  resized down. The cached tiers keep the same cap.
- The whole render has a 120 s deadline. The worker polls the deadline and
  cancels the read. A timeout returns a typed error, not a panic.

The parser checks every declared length before it allocates. A length beyond
the file size is a `Truncated` error. A length that overflows `usize` is a
`LengthTooLarge` error.

## Errors and the no-panic rule

`PsdError` has one variant per failure kind: `Io`, `BadSignature`,
`UnsupportedVersion`, `Truncated`, `BadBlendSignature`, and `LengthTooLarge`.
The reader uses checked arithmetic and checked slices. A fuzz-style test
walks every prefix of a small file and a bounded stride of a fixture; every
read returns a value or a typed error, never a panic.

## Tests and the golden oracle

The unit tests cover the reader and the header. The integration tests in
`tests/parse_fixtures.rs` read the nine committed fixtures under
`tests/fixtures/psd/`. They assert the exact header and first-layer values for
the eight valid fixtures, the PSB eight-byte lengths, the typed error for the
truncated fixture, and the no-panic truncation sweep.

`psd-tools` 1.9.30 is the golden oracle (Q27). Task 23 compares the compositor
output against it per feature. The spike does not need the oracle; the
fixtures already record the expected shape.

## Work split

| Task | Work |
|------|------|
| 17 | Header and first layer spike; this document |
| 18 | Full tiled parser; caps and deadline |
| 19 | Layer compositor: blend modes, masks, groups, clipping, CMYK, 8/16-bit |
| 20 | Layer effects on padded tiles |
| 23 | Per-feature golden tests against `psd-tools` |
