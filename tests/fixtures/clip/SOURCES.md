# CLIP fixture sources

## Fixture

| File | Case | Bytes |
|------|------|-------|
| `preview.clip` | Embedded SQLite database with one `CanvasPreview` row and an overflow-carried PNG preview | 12424 |

The preview is a 48x32 RGBA image. Every pixel follows a fixed formula, so the
bytes do not change between builds. The PNG is 5161 bytes, larger than the
local payload a 4096-byte table leaf page can hold, so the row keeps one page
of payload and one overflow page. A partly transparent pixel (alpha 128) and
opaque pixels are both present.

`preview.clip` has this layout:

| Part | Bytes |
|------|-------|
| Binary prefix, `CSFCHUNK` then zeros | 136 |
| SQLite page 1, schema table | 4096 |
| SQLite page 2, `CanvasPreview` row leaf | 4096 |
| SQLite page 3, overflow page | 4096 |

## Generator

- **File:** `crates/tarragon-render/tests/clip_fixture/mod.rs` (this repository,
  MIT)
- **Owner:** the render test tree. The builder writes the committed bytes; the
  builder writes the SQLite database by hand, page by page, from the SQLite
  file format. The fixture is not a golden image.
- **Check:** `cargo test -p tarragon-render --test clip_render` compares the
  committed bytes with the builder output.
- **Regenerate:** `cargo test -p tarragon-render --test clip_render
  rewrite_the_committed_fixture -- --ignored`
- **Verify with SQLite:** `cargo test -p tarragon-db --test clip_fixture` opens
  the embedded database with `rusqlite` 0.40 (bundled SQLite 3.53.2), runs
  `PRAGMA integrity_check`, and reads the preview blob.

The builder and the reader share the local-payload formula. That shared formula
is not independent evidence, so the rusqlite test is the real oracle.

The generator uses only deterministic inputs. A second run writes
byte-identical files.

## Determinism

SHA-256 of the committed file:

``` text
b019de0c06e41a7eba30aac526a2ac3f7236043fc70f13d99c83319834bdf9e4  preview.clip
```
