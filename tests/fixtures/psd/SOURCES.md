# PSD/PSB fixture corpus — sources

Decision record: Q20. Task: study task 3. Branch: `rust`.

## Tool

- **Name:** psd-tools
- **Version:** 1.9.30
- **License:** MIT License
- **Home:** https://github.com/psd-tools/psd-tools

psd-tools is the one open tool that makes every fixture. The generator uses
the psd-tools low-level writer. psd-tools is also the Q27 golden oracle. One
tool makes the fixtures and reads them back, so the corpus and the oracle agree
by construction.

psd-tools is a test-only tool. It is not linked into the Rust binary. Its
Python runtime dependencies in this environment are Pillow 10.2.0 (HPND) and
NumPy 1.26.4 (BSD-3-Clause). Install it in CI with
`pip install -r tests/fixtures/psd/requirements.txt`. The requirements file
pins the tool and its runtime dependencies: `psd-tools==1.9.30`,
`Pillow==10.2.0`, and `numpy==1.26.4`.

## Generator

- **File:** `tests/fixtures/psd/generate.py` (this repository, MIT)
- **Build the corpus:** `python3 tests/fixtures/psd/generate.py`
- **Verify only:** `python3 tests/fixtures/psd/generate.py --verify-only`
- **Build the large PSB (CI):** `python3 tests/fixtures/psd/generate.py --large-psb PATH`

The generator uses only deterministic inputs. A second run writes byte-identical
files. Each fixture is rebuilt and then reopened with `PSDImage.open`. The
generator fails when a file is 5 MB or larger.

## Cases

Q20 names the fixed case list. This task commits the eight Q20 cases plus one
Q27 fixture, the layer-mask fixture. The count is nine. Q27's other two
fixtures, adjustment/fill and layer effects, arrive with tasks 19 and 20.

| # | File | Case | Bytes |
|---|------|------|-------|
| 1 | `rgb_8bit.psd` | 8-bit RGB PSD | 9894 |
| 2 | `rgb_16bit.psd` | 16-bit RGB PSD | 28715 |
| 3 | `cmyk_8bit.psd` | CMYK PSD | 2346 |
| 4 | `layered_blend_modes.psd` | Layered PSD, common blend modes | 28378 |
| 5 | `layer_groups.psd` | Layer groups | 23726 |
| 6 | `clipping_mask.psd` | Clipping masks | 23426 |
| 7 | `layer_mask.psd` | Layer mask (Q27 fixture) | 14462 |
| 8 | `psb_8bit.psb` | One PSB (version 2, 8-bit RGB) | 10682 |
| 9 | `truncated.psd` | One truncated file | 200 |

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
8. `psb_8bit.psb` — 64x64, RGB, depth 8. File version 2 (PSB). One layer,
   `PSB Layer`.
9. `truncated.psd` — the first 200 bytes of `layered_blend_modes.psd`. The
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
- **Size:** about 3.9 MB. Build time is about 21 s on the Q10 reference.

The 9000 px edge is larger than the 8192 px full-tier cap (Q9) and larger than
the 20 MP large-canvas threshold (Q28). CI exercises both limits with this file.

Task 3 wires `build_large_psb.sh` into CI. Task 36 extends the workflow with
the cross-platform `--release` build and test matrix.
