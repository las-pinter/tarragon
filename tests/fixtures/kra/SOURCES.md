# Krita fixture sources

## Fixture

| File | Case | Bytes |
|------|------|-------|
| `merged.kra` | ZIP archive holding `mergedimage.png` and a smaller `preview.png` | 3137 |

The archive is a minimal ZIP archive holding the two members the renderer
reads. It holds two STORED members at the archive root:

| Member | Size | Content |
|--------|------|---------|
| `mergedimage.png` | 40x24 | Opaque RGBA pixels from a fixed formula |
| `preview.png` | 16x12 | Partly transparent RGBA pixels from a different fixed formula |

The two sizes differ on purpose. The renderer must return the merged image, so
the test pins the merge-wins rule. Both PNG members are lossless, so a render
round trip returns every pixel unchanged.

## Generator

- **File:** `crates/tarragon-render/tests/kra_fixture/mod.rs` (this repository,
  MIT)
- **Owner:** the render test tree. The builder writes the committed bytes with
  the `zip` crate writer. The fixture is not a golden image.
- **Check:** `cargo test -p tarragon-render --test kra_render` compares the
  committed bytes with the builder output.
- **Regenerate:** `cargo nextest run -p tarragon-render --test kra_render
  rewrite_the_committed_fixture --run-ignored ignored-only`

The builder pins every archive field: STORED compression, the DOS timestamp
1980-01-01 00:00:00, and the Unix mode 0o644. The pixels come from fixed
formulas. A second run writes byte-identical files.

## Determinism

The builder is deterministic. No test compares a fixed hash, so no hash line
lives here.
