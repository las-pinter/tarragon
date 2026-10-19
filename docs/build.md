# Build

This file records the pinned toolchain, the build commands, the link mode,
the measured binary sizes, and the 60 MB check.

## Pinned toolchain

`rust-toolchain.toml` pins the toolchain:

```toml
[toolchain]
channel = "1.99.0"
components = ["rustfmt", "clippy", "rust-analyzer"]
```

The workspace sets `rust-version = "1.99"` and `edition = "2024"`.
`cargo --version` reports `cargo 1.99.0 (5f94df478 2026-08-27)`.

## Build commands

Host build, debug:

```sh
cargo build
```

Host release build, glibc:

```sh
cargo build --release
```

The host release binary lands at `target/release/tarragon-app`.

Musl release build:

```sh
cargo build --release --target x86_64-unknown-linux-musl
```

Windows and macOS run `cargo build --release` and `cargo test`. No release
job builds the Linux portable zip yet. The only workflow,
`.github/workflows/rust.yml`, has one job that builds the large PSB fixture.

## Vendored font

`tarragon-ui` embeds the Roboto family. The files are in
`crates/tarragon-ui/assets/fonts/`. The family is Roboto Regular, Medium, and
Bold in unhinted TrueType form. The license is Apache-2.0 and the full text is
`LICENSE-Apache-2.0.txt` in the same directory. The source package, the
version, the copyright, and the file checksums are in `SOURCES.md` there. No
Roboto Mono file exists, so Regular serves the `Monospace` family.

The app crate wires `install` when it takes the `tarragon-ui` dependency, so no
window renders Roboto yet.

## Quality gates

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run
cargo deny check
```

All four gates pass on this branch.

## Link mode

Link musl when every crate allows; otherwise link glibc and record the size.
The result is glibc. The musl path stays the goal.

Facts from this run:

- The musl target is not installed. The toolchain directory lists only
  `x86_64-unknown-linux-gnu`. The sandbox denies the `rustup` command, so
  `rustup target list --installed` and `rustup target add
  x86_64-unknown-linux-musl` did not run.
- The musl build fails with `error[E0463]: can't find crate for 'core'` and
  `can't find crate for 'std'`. The failure is the missing target std, not a
  crate that needs glibc.
- No crate is proven to block musl. The musl target must be installed before
  the musl build can run.

Note: `tarragon-db` will use `rusqlite` with `bundled`. A bundled C build
for musl needs a musl C toolchain, such as `musl-gcc`. That toolchain is absent
here. The `tarragon-db` crate has no dependency yet.

## Measured sizes

Host release, glibc (`cargo build --release`):

| Binary | Bytes | MiB |
|--------|-------|-----|
| `target/release/tarragon-app` | 444,704 | 0.42 |

Musl release: not built. The target std is absent.

### Scope of the measurement

`tarragon-app` is a scaffold. Its manifest has no workspace crate dependency
yet, and `main.rs` prints the version only. The release binary links std only.
The binary has no eframe window and no crate wiring yet.

The two heavy crates are measured through their examples. Run
`cargo build --release --examples`.

| Example | Crates | Bytes | MiB |
|---------|--------|-------|-----|
| `grid_bench` | egui 0.36.2, rayon | 5,114,248 | 4.88 |
| `jpeg_q90_check` | image 0.25.10 | 2,041,520 | 1.95 |

The two example rows were re-measured on this working tree, which is commit
`41ae1b8` plus the uncommitted font work. Neither example calls the font
module, so the font bytes stay out of these sizes.

The final size of the wired binary is not measured yet.

## 60 MB check

The limit is 60 MB. Take 60 MB as 62,914,560 bytes.

| Binary | Bytes | Result |
|--------|-------|--------|
| `tarragon-app` (glibc, release) | 444,704 | pass |
| `grid_bench` (example) | 5,114,248 | pass |
| `jpeg_q90_check` (example) | 2,041,520 | pass |

The highest measured size is 5,114,248 bytes. That is 8.1% of the 60 MB budget.
Every measured binary is under the limit.

## Reproduce

```sh
rustup target list --installed
cargo build --release
stat -c '%s' target/release/tarragon-app
cargo build --release --examples
stat -c '%s' target/release/examples/grid_bench target/release/examples/jpeg_q90_check
```
