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
window renders Roboto yet. The `grid_bench` example calls `tarragon_ui::install`
and measures that shipped text path.

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
| `grid_bench` | egui 0.36.2, rayon, the vendored Roboto fonts | 6,552,192 | 6.25 |
| `jpeg_q90_check` | image 0.25.10 | 2,041,520 | 1.95 |

The two example rows were re-measured on this working tree. The `grid_bench`
example carries the three Roboto files (305,608, 306,536, and 306,940 bytes),
because it installs the shipped fonts. `jpeg_q90_check` calls no font module.

The final size of the wired binary is not measured yet.

### Grid benchmark

The grid benchmark measures the CPU cost of one frame at the target geometry:
196 pt minimum tracks, 208 pt cell height, and a 14 pt gap. The run covers two
widths: 874 by 900 pt for the shell grid width, and 1600 by 900 pt for a wide
viewport. It runs headless. It measures egui layout, the paint list build, and
tessellation. It measures no GPU work.

Two passes run at each width. The first pass paints the grid container only and
sweeps the whole 10,000 item list. The second pass paints the whole gallery
(`Gallery::show`: the tabs, the search field, the info bar, the sort pills, and
the grid) and sends no input, so it measures one quiet frame that reads the
cached visible order.

Warning: the run covers the simplified data set. A tile carries a file name, a
badge derived from the path, and no texture. A real frame adds the uploaded
thumbnails and the decoded images behind them to these numbers.

The text path is the vendored Roboto family, installed through
`tarragon_ui::install`, so the numbers include the shipped text layout.

Machine: Intel Core i7-12700K, 10 logical CPUs available, 3.6 GHz, Ubuntu
24.04.4 LTS, kernel 6.17.0-35-generic, Rust 1.99.0. The run uses a release
build and a measurement pool capped at four threads.

```sh
cargo run --release --example grid_bench -p tarragon-ui
```

Grid container pass:

| Metric | 874 pt grid | 1600 pt grid |
|--------|-------------|--------------|
| Items | 10,000 | 10,000 |
| Grid (at the requested width) | 4 columns, 208 pt cells, 14 pt gap | 7 columns, 216.6 pt cells, 14 pt gap |
| Cells built per frame | min 20, max 24, avg 20 | min 35, max 42, avg 35 |
| Minimum frame time | 0.146 ms | 0.198 ms |
| Average frame time | 0.172 ms | 0.240 ms |
| Median frame time | 0.154 ms | 0.227 ms |
| p95 frame time | 0.275 ms | 0.313 ms |
| Max frame time | 0.580 ms | 0.657 ms |

Gallery pass, 10,000 items in scope:

| Metric | 874 pt grid | 1600 pt grid |
|--------|-------------|--------------|
| Visible items | 10,000 | 10,000 |
| Tiles painted per frame | 16 | 28 |
| Minimum frame time | 0.060 ms | 0.085 ms |
| Average frame time | 0.072 ms | 0.093 ms |
| Median frame time | 0.062 ms | 0.088 ms |
| p95 frame time | 0.120 ms | 0.111 ms |
| Max frame time | 0.144 ms | 0.273 ms |

The frame count is 400 after 40 warmup frames. The painter stretches the cell
width to the inner width after the scrollbar, so the painted cells are at most
the stated width.

The gallery keeps the display order in a cache, so a quiet frame rebuilds no
list of 10,000 items. A scope, folder, sort, or applied-search change rebuilds
that cache once. The quiet frame then costs 0.062 ms at 874 pt and 0.088 ms at
1600 pt.

Two frame limits apply. The interface targets 60 frames per second, so one frame
has 16.7 ms. The hard gate for one frame is 100 ms. The slowest measured frame
is 0.657 ms for the grid container and 0.273 ms for the whole gallery. The
grid cost is 3.9% of the 16.7 ms budget and the gallery cost is 1.6% of it.

## 60 MB check

The limit is 60 MB. Take 60 MB as 62,914,560 bytes.

| Binary | Bytes | Result |
|--------|-------|--------|
| `tarragon-app` (glibc, release) | 444,704 | pass |
| `grid_bench` (example) | 6,552,192 | pass |
| `jpeg_q90_check` (example) | 2,041,520 | pass |

The highest measured size is 6,552,192 bytes. That is 10.4% of the 60 MB
budget. Every measured binary is under the limit.

## Reproduce

```sh
rustup target list --installed
cargo build --release
stat -c '%s' target/release/tarragon-app
cargo build --release --examples
stat -c '%s' target/release/examples/grid_bench target/release/examples/jpeg_q90_check
```
