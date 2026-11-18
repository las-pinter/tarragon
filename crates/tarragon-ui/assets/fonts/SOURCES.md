# Font sources

The three font files in this directory are vendored byte for byte. The build
embeds them with `include_bytes!`, so the binary reads no font from disk.

## Upstream

| Field | Value |
|-------|-------|
| Project | `google/roboto` |
| Source | `git clone https://github.com/google/roboto` |
| Debian package | `fonts-roboto-unhinted` |
| Debian version | `2:0~20170802-3` |
| License | Apache-2.0 |
| Copyright | 2015 Google Inc. |

## Source paths

The files come from the installed Debian package:

| Vendored file | Source path |
|---------------|-------------|
| `Roboto-Regular.ttf` | `/usr/share/fonts/truetype/roboto/unhinted/RobotoTTF/Roboto-Regular.ttf` |
| `Roboto-Medium.ttf` | `/usr/share/fonts/truetype/roboto/unhinted/RobotoTTF/Roboto-Medium.ttf` |
| `Roboto-Bold.ttf` | `/usr/share/fonts/truetype/roboto/unhinted/RobotoTTF/Roboto-Bold.ttf` |

The packaged copyright file is
`/usr/share/doc/fonts-roboto-unhinted/copyright`.

## Files

| File | Bytes | SHA-256 |
|------|-------|---------|
| `Roboto-Regular.ttf` | 305,608 | `797e35f7f5d6020a5c6ea13b42ecd668bcfb3bbc4baa0e74773527e5b6cb3174` |
| `Roboto-Medium.ttf` | 306,536 | `7984aafeaf437899ce89b251d2875fc5048cb7d290152eac070239cca7ff726c` |
| `Roboto-Bold.ttf` | 306,940 | `36f3709dea3e3ce3c6aedc058079e55980825f898f1e901d091c73c40de8bab1` |
| `LICENSE-Apache-2.0.txt` | 11,358 | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` |

Each file is an unhinted TrueType font. Its first four bytes are `00 01 00 00`
(the TrueType scaler tag). egui does not run hinting bytecode, so the unhinted
build of the family is the correct choice. The hinting instructions in these
files are ignored.

## License

The fonts carry the Apache-2.0 license. The full text is in
`LICENSE-Apache-2.0.txt` in this directory. Apache-2.0 is on the `deny.toml`
allowlist.

## Monospace fallback

No Roboto Mono file is in this directory. No Apache-2.0 monospace font is
installed on the build host. The `Monospace` family therefore lists Roboto
Regular as its only entry. Roboto Regular is not monospace, so its glyphs do
not have a fixed advance width. Text in that family still renders and shapes
correctly.
