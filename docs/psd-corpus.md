# PSD corpus

The PSD corpus is 16 files from public repositories: 9 PSD or PSB files and 7
reference PNG files. Tests render the 9 files through the same code the app
uses. The tests need real files because the committed fixtures are small and
synthetic.

## Why it is not committed

Upstream states the licence of each repository, not the licence of each binary
file. The repositories are ag-psd (MIT), Krita (GPL-3.0), and psd-tools (MIT).
Because the file licences are unknown, `tests/psd-corpus/fetch.sh` downloads the
files over https. Each file is pinned to one commit and checked against a
sha256. The files are never committed. The ignore rule in `.gitignore` covers
the folder `tests/psd-corpus/` except the script.

## Committed adjustment and fill fixtures

Two small fixtures are committed in `tests/fixtures/psd/`. They run in CI with
no download.

- `adjustment_blocks.psd` holds ten adjustment blocks, one per cell. A layer
  mask limits each block to its cell.
- `fill_layers.psd` holds four fills: solid, solid with opacity, gradient, and
  pattern.

How each is checked:

- `fill_layers.psd` has a golden image from psd-tools, `golden/fill_layers.png`,
  with a tolerance of 2 per channel.
- `adjustment_blocks.psd` has rule tests: order, sign, identity, and exact
  channel moves. psd-tools skips adjustment layers, and no Photoshop render is
  committed, so no golden image exists.

The one real oracle for the adjustment blocks is the ag-psd `adjustment-layers`
sample in the uncommitted corpus. It covers only the end of the layer stack.

The Rust test builders write both files, not `generate.py`. To rewrite a file
after a builder change, run `cargo test -p tarragon-psd --test <name> --
--ignored`, with `<name>` set to `adjustment_fixture` or `fill_fixture`.

The tests do not check the recorded approximations in `hue2`, `blnc`, `selc`,
`blwh`, and `phfl` against Photoshop.

## Commands

1. Download the files into a folder:

   ```sh
   sh tests/psd-corpus/fetch.sh /path/to/corpus
   ```

2. Point the tests at the folder:

   ```sh
   export TARRAGON_PSD_CORPUS=/path/to/corpus
   ```

3. Run the render tests and the parser tests:

   ```sh
   cargo nextest run -p tarragon-render --test psd_corpus
   cargo nextest run -p tarragon-psd
   ```

With `TARRAGON_PSD_CORPUS` unset, each corpus test prints
`skip: TARRAGON_PSD_CORPUS is not set` and passes. Nextest shows that line for
the `psd_corpus` tests, because the repository's nextest config shows the output
of that test binary.

With the variable set, the two behaviours differ by crate:

- In `tarragon-render`, a listed file that is missing fails the test. The
  message names `tests/psd-corpus/fetch.sh`.
- In `tarragon-psd`, the vector mask test and the pattern fill test print
  `skip: <path> does not exist` and pass when their file is missing.

CI does not run the corpus. Only the checks that need no files run there.

## The nine files

Measured on 2026-11-09 with `--no-capture`. "Real merged image" is whether the
file has a merged image that a program wrote. "Raw scan" is a search of the file
bytes for the 16-bit and 32-bit layer keys. Sizes are in bytes.

| File | Source | Size | Width x height | Depth | Mode | Real merged image | Raw scan | Outcome | Reference used |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `16bits.psd` | ag-psd | 722,758 | 500 x 500 | 16 | RGB | no | Lr16 | renders, from its layers (ZIP with prediction) | no, blank |
| `32bits.psd` | ag-psd | 1,211,902 | 300 x 300 | 32 | RGB | no | Lr32 | typed decode error | no, blank |
| `cmyk.psd` | ag-psd | 736,185 | 200 x 200 | 8 | CMYK | yes | none | renders | logged only |
| `psb.psb` | ag-psd | 135,246 | 413 x 413 | 8 | RGB | yes | none | renders | yes |
| `adjustment-layers.psd` | ag-psd | 229,398 | 200 x 200 | 8 | RGB | yes | none | renders | yes |
| `effects.psd` | ag-psd | 103,944 | 200 x 200 | 8 | RGB | yes | none | renders | yes |
| `vector_mask.psd` | Krita | 46,437 | 128 x 128 | 8 | RGB | yes | none | renders | its merged image, premultiplied, with alpha checked |
| `masks.psd` | Krita | 367,671 | 220 x 220 | 8 | RGB | yes | none | renders | yes |
| `pattern-fill.psd` | psd-tools | 62,934 | 32 x 32 | 8 | RGB | yes | none | renders | its merged image |

The other 7 files are the reference PNG files: `16bits`, `32bits`, `cmyk`,
`psb`, `adjustment-layers`, `effects`, and `masks`.

## Findings

8 of the 9 files render. Each render took 148 ms or less in a debug build.
The slowest is `16bits.psd`.

- `32bits.psd` gives a typed `Decode` error, because 32-bit RGB is not
  supported. The test expects this error.
- `psb.psb`, `masks.psd`, `adjustment-layers.psd`, and `effects.psd` match their
  reference PNG files at the golden tolerance of 2.
- `pattern-fill.psd` matches its merged image at the golden tolerance of 2.
- `cmyk.psd` differs from its reference by up to 102 on 103,624 of 160,000
  channels. The test only logs this gap. The cause is not examined.
- `16bits.psd` has no real merged image, and its layer channels use ZIP with
  prediction. The layer compositor decodes them. The test
  `the_16bit_sample_matches_the_measured_pixels` pins five pixels and the
  visible pixel count. The pixels were measured with a reference decoder, as
  RGBA at (x, y): (0, 0) is 0, 0, 0, 0; (499, 499) is 0, 0, 0, 0; (250, 250) is
  95, 39, 2, 255; (100, 100) is 33, 35, 20, 255; (400, 300) is 60, 70, 13, 255.
  The colour tolerance is 2. The visible count is 180,972 pixels, within 0.1
  percent.
- The merged image of `vector_mask.psd` stores its colour matted over white. The
  raw samples therefore differ from the layer composite where alpha is below
  one. The test `the_vector_mask_sample_matches_its_merged_image` compares alpha
  and premultiplied colour with `read_merged_pixels`, with alpha within the
  tolerance of 2, and it passes. Both sides
  share the sample decoding, and the merged alpha comes from the raster mask
  alone. So the test cannot show that the vector shape is right.

ZIP (code 2) and ZIP with prediction (code 3) layer channels decode at depth 8
and 16. A cut stream, extra output, a missing checksum, or garbage is a typed
decode error, and it fails the render. Compression codes 4 and above still skip
the layer.

Known defects:

- The `cmyk.psd` gap above is open.
- 32-bit RGB is not supported (`32bits.psd`).

The reference PNG files of `16bits.psd` and `32bits.psd` are blank white, so no
test uses them.

## Memory run

The 10,000-file run of the memory benchmark used this corpus. See
[PSD memory benchmark](build.md#psd-memory-benchmark) for the numbers. Its FAIL
verdict is not a memory failure. The bench reports errors by kind only. The 18
sources cycle over 1,000 PSD and PSB files, so `32bits.psd` gets 55 copies. This
matches the 55 decode errors, so the 32-bit error is the likely cause.

The run was repeated on 2026-11-09 after ZIP decoding, when `16bits.psd` began
to composite instead of being skipped. The peak rose from 29.7 MiB to 35.3 MiB.
The run did not measure the cause of that change. The verdict is still FAIL for
the same reason: 55 `Decode` errors, none of them new.
