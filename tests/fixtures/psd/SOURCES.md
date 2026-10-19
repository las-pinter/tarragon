# PSD/PSB fixture corpus — sources

## Tool

- **Name:** psd-tools
- **Version:** 1.9.30
- **License:** MIT License
- **Home:** https://github.com/psd-tools/psd-tools

psd-tools 1.9.30 generated the corpus. The generator uses the psd-tools
low-level writer.

psd-tools is a test-only tool. It is not linked into the Rust binary. Its
Python runtime dependencies are Pillow 10.2.0 (HPND) and NumPy 1.26.4
(BSD-3-Clause). Install it in CI with
`pip install -r tests/fixtures/psd/requirements.txt`. The requirements file
pins the tool and its runtime dependencies: `psd-tools==1.9.30`,
`Pillow==10.2.0`, and `numpy==1.26.4`.

The golden images in `golden/` have their own source file:
`golden/SOURCES.md`.

## Generator

- **File:** `tests/fixtures/psd/generate.py` (this repository, MIT)
- **Build the corpus:** `python3 tests/fixtures/psd/generate.py`
- **Build the corpus including the Rust-built files:**
  `python3 tests/fixtures/psd/generate.py --include-rust-built`
- **Verify only:** `python3 tests/fixtures/psd/generate.py --verify-only`
- **Build the large PSB (CI):** `python3 tests/fixtures/psd/generate.py --large-psb PATH`

The generator uses only deterministic inputs. A second run writes byte-identical
files. Each fixture is rebuilt and then reopened with `PSDImage.open`. The
generator fails when a file is 5 MB or larger. The layer-effects fixture has a
second builder in the Rust test tree, so its committed bytes stay reproducible
when the pinned interpreter is absent. The Rust builder owns that file. The
generator skips it by default and writes it only with `--include-rust-built`.
That committed file is not a golden image: no test compares it with a Photoshop
render.

The two layer-effects builders write the same document shape, not byte-identical
bytes. These descriptor and record fields differ:

- The Rust builder writes a linear transfer contour (`TrnS`) on the shadows,
  the glows, and the bevel, and the satin curve (`MpgS`). The generator omits
  them.
- The Rust builder can write the inner-glow source enum (`glwS`); the generator
  never writes it.
- The Rust builder writes the bevel depth (`srgR`) as 1 percent. The generator
  writes 100 percent.
- The Rust builder writes a Pascal layer name only. The generator also writes
  a Unicode layer name block.
- The Rust builder writes empty blending ranges. The generator writes the
  default range block.
- The Rust builder writes raw channel rows. The generator writes RLE rows.

## Cases

The corpus has ten fixtures.

| # | File | Case | Bytes |
|---|------|------|-------|
| 1 | `rgb_8bit.psd` | 8-bit RGB PSD | 9894 |
| 2 | `rgb_16bit.psd` | 16-bit RGB PSD | 28715 |
| 3 | `cmyk_8bit.psd` | CMYK PSD | 2346 |
| 4 | `layered_blend_modes.psd` | Layered PSD, common blend modes | 28378 |
| 5 | `layer_groups.psd` | Layer groups | 23726 |
| 6 | `clipping_mask.psd` | Clipping masks | 23426 |
| 7 | `layer_mask.psd` | Layer mask | 14462 |
| 8 | `layer_effects.psd` | Layer effects | 263499 |
| 9 | `psb_8bit.psb` | One PSB (version 2, 8-bit RGB) | 10682 |
| 10 | `truncated.psd` | One truncated file | 200 |

Each file is under 5 MB.

### How each fixture was made

1. `rgb_8bit.psd` — 64x64, RGB, depth 8, 3 channels. One layer, `Background`,
   blend mode `NORMAL`. Raw channel samples are RLE compressed.
2. `rgb_16bit.psd` — 48x48, RGB, depth 16, 3 channels. One layer, `Deep 16-bit`.
   Big-endian 16-bit samples.
3. `cmyk_8bit.psd` — 64x64, CMYK, depth 8, 4 channels (C, M, Y, K). One layer,
   `CMYK Ink`.
4. `layered_blend_modes.psd` — 96x96, RGB, depth 8, 4 merged channels. Five
   layers: `Normal`, `Multiply`, `Screen`, `Overlay`, `Linear Dodge`. Every
   layer has an alpha channel and opacity 200.
5. `layer_groups.psd` — 96x96, RGB. One `Background` layer and one open group
   `Group 1` with two children, `Child A` and `Child B`. The group uses the
   `pass` blend mode.
6. `clipping_mask.psd` — 96x96, RGB. One base layer `Base` and two clipped
   layers, `Clip A` and `Clip B`. Both clipped layers set clipping to `NON_BASE`.
7. `layer_mask.psd` — 64x64, RGB. One layer, `Masked`, with a pixel layer mask
   in channel `-2`.
8. `layer_effects.psd` — 192x192, RGB. One `Backdrop` layer and nine layers,
   one per effect kind: `Drop Shadow`, `Inner Shadow`, `Outer Glow`,
   `Inner Glow`, `Stroke`, `Bevel`, `Satin`, `Color Overlay`, and
   `Gradient Overlay`. Every effect layer carries an `lfx2` block.
   The pinned psd-tools interpreter was not available when the fixture was
   added, so the committed file was hand-built with the PSD writer rules in
   `crates/tarragon-psd/tests/layer_effects_fixture.rs`. The check test there
   keeps the committed bytes and that builder in step. The Rust builder owns
   the committed file; the generator skips it unless `--include-rust-built` is
   passed.
   The Python generator writes the same document shape, not byte-identical
   bytes: the descriptor and record fields listed above differ, and the RLE
   channel rows differ from raw rows. The hash below is for the committed
   hand-built file. The file is not a golden image.
9. `psb_8bit.psb` — 64x64, RGB, depth 8. File version 2 (PSB). One layer,
   `PSB Layer`.
10. `truncated.psd` — the first 200 bytes of `layered_blend_modes.psd`. The
   `8BPS` signature and the header stay valid. The layer and mask section is
   cut, so a full read fails. This file tests the error path.

### Determinism

SHA-256 of each committed file:

``` text
aa2b14f52d4149a01ecd871a13d94e88959cf83cac9c85db10b584648fd126c5  clipping_mask.psd
f78778e6397e6d1fe29f7db363128a7558c3d1e7c6c34c1d47d3637425c27969  cmyk_8bit.psd
e4554b51aa9cb1725f0b188f23da96b132378be6f55cbc6678cbe974f9d9da47  layered_blend_modes.psd
cab3dbaf65c6349539bb51c131c43338034895a09b8c157b4e4ade731e295ff8  layer_groups.psd
b5a5fe6a18ec321e12193d454cadaf252608d90146b4e8d094fcf2f8164b6979  layer_mask.psd
b709cc81933ecdc65e64e2a9b8e5103e16ec4cd8f3e15a6991633042948ad9a7  layer_effects.psd
7d41baafc6fd667c360c83296304946300a6016812302141dcc77fa4481e3e85  rgb_16bit.psd
27cb2f35adc7f351c791bd0042aeb5fb5f768cbf560ff18da3aebe9f6bdddf03  rgb_8bit.psd
cd0fc6fa3354a59d5b51f1a9aee9fff653c93168c391553ea5fcb61cd002edeb  truncated.psd
6f7ceb42e790c725bf84303c4cf25a6f7cd25f9b84b20f08ecaf4d4ca40dcb09  psb_8bit.psb
```

## Large PSB

The large PSB is built at CI time and is not committed.

- **Script:** `tests/fixtures/psd/generate.py --large-psb PATH`
- **Wrapper:** `sh tests/fixtures/psd/build_large_psb.sh [PATH]` (default output
  is a temp path, `${TMPDIR:-/tmp}/tarragon-large.psb`)
- **Shape:** 9000x9000 px, PSB (version 2), RGB, depth 8, merged image only.
- **Size:** about 3.9 MB. Build time is roughly 21 s on a development
  machine.

The 9000 px edge is larger than the 8192 px full-tier cap and larger than the
20 MP large-canvas threshold. CI exercises both limits with this file.

CI runs `build_large_psb.sh` in the cross-platform `--release` build and test
matrix.
