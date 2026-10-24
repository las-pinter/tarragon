# PSD golden images — sources

These files are golden images. A test compares the tarragon compositor output
with each file. The source fixtures in the parent directory are inputs, not
golden images.

The tool, its version, its license, and its Python dependencies are in
`../SOURCES.md`. This file adds only what the golden set adds.

## Generator

- **File:** `tests/fixtures/psd/generate_goldens.py` (this repository, MIT)
- **Render call:** `PSDImage.open(path).composite(ignore_preview=True).convert("RGBA")`
- **PNG writer:** `compress_level=9`, `optimize=False`, no text chunks
- **Write the files:** `python3 tests/fixtures/psd/generate_goldens.py --write`
- **Check the files:** `python3 tests/fixtures/psd/generate_goldens.py --check`

The `ignore_preview` switch makes psd-tools composite the layers instead of
using the merged preview. The `force` switch does not do that: in psd-tools
1.9.30 `force` means "force vector drawing". A CMYK composite with
`force=True` returns mode `CMYKA`, which Pillow 10.2.0 cannot write.

The check mode is the default and it is strict:

- A missing golden file is an error.
- A pixel difference is an error. Both PNG files decode and compare as RGBA8
  pixels, so a Pillow or zlib change that keeps the pixels passes this check.

The generator uses deterministic inputs and fixed PNG settings. A second run
writes byte-identical files.

## Golden set

`golden/manifest.txt` lists the eight golden stems, one per line. The generator
and the Rust test read that one file, so a stem cannot be added to one side
only. The corpus file for a stem is `<stem>.psd`, or `<stem>.psb` for the one
PSB fixture.

| File | Source fixture | Size |
|------|----------------|------|
| `rgb_8bit.png` | `rgb_8bit.psd` | 64x64 |
| `rgb_16bit.png` | `rgb_16bit.psd` | 48x48 |
| `cmyk_8bit.png` | `cmyk_8bit.psd` | 64x64 |
| `layered_blend_modes.png` | `layered_blend_modes.psd` | 96x96 |
| `layer_groups.png` | `layer_groups.psd` | 96x96 |
| `clipping_mask.png` | `clipping_mask.psd` | 96x96 |
| `layer_mask.png` | `layer_mask.psd` | 64x64 |
| `psb_8bit.png` | `psb_8bit.psb` | 64x64 |

Note: the CMYK golden pixel is pending an independent oracle reading, because
the current expectation comes from the same formula the code uses.

### Excluded fixtures

- `truncated.psd` — the file is cut on purpose and a full read must fail.
- `layer_effects.psd` — psd-tools renders the layer effects only in part. The
  known deltas do not give a useful golden image.

## Local run

The golden tests run in a plain test run:

```sh
cargo nextest run -p tarragon-psd
```

To regenerate the PNG files, write them with the generator:

```sh
python3 tests/fixtures/psd/generate_goldens.py --write
```

To verify the committed files, run the check. It renders each fixture in
memory, compares pixels, and writes nothing:

```sh
python3 tests/fixtures/psd/generate_goldens.py --check
```

## Known deltas

The golden suite compares straight RGB with alpha ignored. The two alpha
differences below are recorded deltas, not failures.

| Fixture | Delta |
|---------|-------|
| `layer_mask` | At the first differing coordinate the composite is `[255,255,255,0]` and the golden is `[255,255,255,255]`. The pixel-mask alpha cut is covered by the rule test `the_layer_mask_fixture_cuts_the_layer_alpha` in `crates/tarragon-psd/tests/composite_fixtures.rs`. This row proves only that the composite RGB agrees. |
| `layered_blend_modes` | The fixture holds five layers whose channel alpha is 160 with layer opacity 200. The straight-alpha stack gives `255 * (1 - (1 - (160/255) * (200/255))^5)`, which is about 246.4, so 246 is correct for a document with no merged background. The oracle's 255 is the delta. |

## Tolerance

The tests use a provisional per-channel tolerance of 2 on 8-bit values. A small
band absorbs rounding through float compositing: the two renderers use
different float paths, so a channel can differ by a small value after the
conversion to 8 bits. A measured maximum delta replaces the value once the
first set lands. The shared helper prints that maximum when
`TARRAGON_GOLDEN_REPORT` is set, and every failure message carries it.

## Behavior when a file is absent

A comparison test that finds its golden absent prints one line with the path
and returns without a failure. The existence test and `--check` fail on an
absent file. CI writes the files before it runs the tests.
