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

The host release binary lands at `target/release/tarragon`.

Musl release build:

```sh
cargo build --release --target x86_64-unknown-linux-musl
```

Windows and macOS run `cargo build --release` and `cargo test`. No release
job builds the Linux portable zip yet. The only workflow,
`.github/workflows/rust.yml`, has one job that builds the large PSB fixture.

## Portable data folder

A `data/` directory beside the binary turns on portable mode. The database
(`tarragon.db`), the cache (`cache/`), and the log (`tarragon.log`) then live
inside it. An empty `data/` counts. A plain file named `data` does not.
Without the directory, the app uses the platform data folder, `tarragon`
inside it:

- Linux: `~/.local/share/tarragon`
- macOS: `~/Library/Application Support/tarragon`
- Windows: `%APPDATA%\tarragon`

The check runs in every build, debug included. The `TARRAGON_DB` variable
overrides the database path only. An empty `TARRAGON_DB` counts as unset. The
app creates the data root and `cache/` at start, unless `TARRAGON_DB` is set.
Today only the database path is wired in; the cache and log paths
are resolved for later use.

## Vendored font

`tarragon-ui` embeds the Roboto family. The files are in
`crates/tarragon-ui/assets/fonts/`. The family is Roboto Regular, Medium, and
Bold in unhinted TrueType form. The license is Apache-2.0 and the full text is
`LICENSE-Apache-2.0.txt` in the same directory. The source package, the
version, the copyright, and the file checksums are in `SOURCES.md` there. No
Roboto Mono file exists, so Regular serves the `Monospace` family.

The app window renders Roboto: `apply_theme` installs the fonts, and the app
calls it when the window is created. The `grid_bench` example calls `tarragon_ui::install`
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
| `target/release/tarragon` | 444,704 | 0.42 |

Musl release: not built. The target std is absent.

### Scope of the measurement

The `tarragon-app` binary parses the `--version` and `--help` flags. Without a
flag it resolves the data directory, takes the single-instance guard, sets up
logging, and opens the database, then opens the shell window with eframe. It
tries the wgpu renderer first and falls back to glow. A second start ends after
the guard, before the log file is opened or rolled. The size figure above
was measured at an earlier, smaller build, before the window was wired in, and
is not re-measured here.

The two heavy crates are measured through their examples. Run
`cargo build --release --examples`.

| Example | Crates | Bytes | MiB |
|---------|--------|-------|-----|
| `grid_bench` | egui 0.36.2, rayon, the vendored Roboto fonts | 6,559,464 | 6.26 |
| `jpeg_q90_check` | image 0.25.10 | 1,971,456 | 1.88 |

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
| `tarragon` (package `tarragon-app`, glibc, release) | 444,704 | pass |
| `grid_bench` (example) | 6,559,464 | pass |
| `jpeg_q90_check` (example) | 1,971,456 | pass |

The highest measured size is 6,559,464 bytes. That is 10.4% of the 60 MB
budget. Every measured binary is under the limit.

## GUI stack spike

The spike checked the window stack before the app shell was wired in. The code
is the example `crates/tarragon-app/examples/stack_spike/`. `eframe` is a normal
dependency of `tarragon-app`, because the app window uses it. Only `egui_dock`
is a dev-dependency, so the shipped app binary does not link it.

The workspace pins `eframe =0.36.2` with default features off. The features
are `glow`, `wgpu`, `x11`, and `wayland`. `default_fonts` stays off, and the
example installs the vendored Roboto fonts. It adds `egui_dock 0.21.1`. The
example picks the renderer at run time through `NativeOptions`.

### Renderer

Decision: `wgpu` is the default renderer. `glow` stays as the fallback that
the user can select. The decision is final. It follows the frame-time run
below.

Rule: `wgpu` stays the default unless one of these is true on the owner run:

- `wgpu` fails a limit (p99 of 33 ms or more, or a frame over 100 ms) and
  `glow` passes.
- `glow` is clearly better: its p99 is more than 20 percent lower than the
  `wgpu` p99 on the same machine and in the same session type.

Outcome: neither condition is true in either session type. `wgpu` passes
both. `glow` has the higher p99 in both session types: 39.91 ms against
27.83 ms on Wayland, where `glow` also fails the limit, and 21.21 ms against
15.74 ms on X11. See the results in "Frame
time".

Reasons for `wgpu`:

- It uses Metal on macOS. Apple deprecated OpenGL on macOS in macOS 10.14.
- It uses Vulkan or DX12 on Linux and Windows.
- eframe 0.36.2 picks `wgpu` when both renderers are compiled in. Its default
  features also list `wgpu` and not `glow`.
- It reports the adapter. The log shows a software device such as `llvmpipe`.

Reasons to keep `glow`:

- The binary is about 5.7 MB smaller after strip and thin LTO.
- The dependency tree is smaller.
- It works on old GL drivers and in virtual machines that have no Vulkan.

Both fit the size limit by a wide margin. Cost of keeping both: the combined
build is about 0.7 MB larger than the `wgpu` build.

The `accesskit` feature of eframe is off in the spike. The real app must
decide on it, because it adds screen reader support. This is an open item.

### Dependency check

`cargo deny check` on the full tree, with the eframe and `egui_dock` crates:

| Check | Result |
|-------|--------|
| advisories | ok |
| bans | ok, with six duplicate-version warnings (`getrandom`, `hashbrown`, `miniz_oxide`, `r-efi`, `syn`, `windows-sys`) |
| licenses | ok, with one warning: the `BSL-1.0` allowance matches no crate |
| sources | ok |

No new license was needed. `deny.toml` did not change. Crates with other
licenses in the tree (`0BSD`, `BSD-2-Clause`, `Apache-2.0 WITH LLVM-exception`,
`Unlicense`) are dual-licensed with an allowed option, so the check passes.
The tree has only one `egui` (0.36.2).

### Binary size

The example, built with `cargo build --release --example stack_spike -p
tarragon-app`. The limit is 60 MB (62,914,560 bytes). The strip and thin LTO
column uses `CARGO_PROFILE_RELEASE_STRIP=true` and
`CARGO_PROFILE_RELEASE_LTO=thin`. No release profile is set in the repository.

| Build | Default release (bytes) | Strip and thin LTO (bytes) | Share of 60 MB |
|-------|-------------------------|----------------------------|----------------|
| glow only | 13,334,176 | 9,747,672 | 15.5% |
| wgpu only | 20,536,744 | 15,431,152 | 24.5% |
| both renderers | 21,297,320 | 16,079,680 | 25.6% |

The share is the strip and thin LTO size over 60 MB. Every build is under the
limit. The "both renderers" row is from the final example. The single-renderer
rows come from a copy of the repository with one renderer only, and from an
earlier example that was a few kilobytes smaller. The repository builds both
renderers.

### glibc floor

The example was built in release mode in containers. The repository was
mounted read-only. The toolchain came from `rust-toolchain.toml` through
rustup. The highest `GLIBC_` symbol version comes from `objdump -T`.

| Build image | glibc of the image | Highest symbol | Symbols at the top |
|-------------|--------------------|----------------|--------------------|
| `ubuntu:22.04` | 2.35 | `GLIBC_2.35` | `hypot`, `hypotf` |
| `ubuntu:20.04` | 2.31 | `GLIBC_2.30` | `gettid` |
| host, Ubuntu 24.04 | 2.39 | `GLIBC_2.35` | `hypot`, `hypotf` |

The floor is glibc 2.30 when the build runs on `ubuntu:20.04`. A build on a
newer image raises the floor to the glibc of that image, at most. Use an old
image for the release build. `debian:bullseye` could not install packages: its
security archive is gone. The system libraries that the binary loads at run
time (X11, Wayland, EGL, Vulkan) are not in this number.

Packages that the container build needed (Debian and Ubuntu names):
`build-essential`, `ca-certificates`, `curl`, and `binutils`. No `-dev`
package was needed. The window and graphics libraries load at run time.

Packages for a run in a container (not tested as a minimal set):
`libx11-6`, `libxcursor1`, `libxrandr2`, `libxi6`, `libxkbcommon0`,
`libxkbcommon-x11-0`, `libwayland-client0`, `libegl1`, `libgl1`,
`libgl1-mesa-dri`, `libvulkan1`, and `mesa-vulkan-drivers`.

The CI `gates` job needs no new `apt` step. The example built on a plain
`ubuntu:22.04` image with `build-essential` only. The `release-build` jobs
for Windows and macOS run `cargo build --release --workspace`. That command
builds no example and no dev-dependency, so the new crates do not enter it.

### egui_dock

Verdict: `egui_dock 0.21.1` works with `egui =0.36.2`. Version 0.21 requires
`egui ^0.36`, and the lock file has one `egui`. The example shows one tab
("Stats") in a `DockArea`. A run under Xvfb drew the tab, the tab text in
Roboto, and the grid, with both renderers and without a panic. Version 0.20
needs `egui ^0.35` (crates.io data) and does not fit. The workspace pins
`egui_dock =0.21.1`.

### Frame time

Status: done. The owner ran the measurement on a local desktop with a GPU.
Do not measure over an SSH X forward. It usually renders in software and has
no DRI3 direct path.

The example uploads 1000 textures of 256 by 256 pixels. All 1000 stay alive
for the whole run. It scrolls the grid by itself, and only the visible rows
draw. At 1280x800 about 70 tiles (7 percent) draw in each frame. A Xvfb run
counted a mean of 67.5 and a maximum of 72. The log header records the mean
and the maximum for each run.

It records the time between frames for 30 s after a 5 s warm-up. It writes a
log file and prints p50, p95, p99, max, the frame count, the renderer, and the
session type. The log header shows the real measured span (the sum of the
frame times), not only the configured duration. The exit code is 0 on a pass,
1 on a fail, and 2 when the window does not open, the run is not valid, or the
log cannot be written. The limits are a p99 under 33 ms and no frame over
100 ms (hard fail).

Vsync is on. On a 60 Hz display one dropped frame takes 33.3 ms. So more than
1 percent dropped frames puts the p99 at 33 ms or more, and the run fails. The
logs do not record the refresh rate.

Keep the window visible and not covered during each run. Do not minimize it
and do not put another window over it. A minimized or covered window stops
frame callbacks, mostly on Wayland. The example then marks the run not valid
(exit code 2) and keeps the log with the frames collected so far. A lost focus
does not stop the frames. The log records it as `focus_lost_frames` and the
summary prints a warning. A wall-clock watchdog ends a run that gets no
frames, and the script stops a run after the warm-up, the duration, and 60 s.
The log is written for every run, valid or not, also when the window cannot
open. If the watchdog finds that the window does not close after a normal end,
it writes the report with the real result and exits. The summary and the log
show p99 and max cut to two decimals, never rounded up.

Run both renderers in the current session:

```sh
crates/tarragon-app/examples/spike-frame-time.sh spike-frame-time-wayland
```

Run again under X11. On a Wayland desktop, force XWayland, or log in to an X11
session. The `env -u WAYLAND_DISPLAY` form needs `DISPLAY` to be set, which is
true under XWayland. The log header line `# session: x11` proves that the run
used X11:

```sh
env -u WAYLAND_DISPLAY crates/tarragon-app/examples/spike-frame-time.sh spike-frame-time-x11
```

Run one renderer by hand:

```sh
cargo run --release --example stack_spike -p tarragon-app -- \
    --renderer wgpu --duration 30 --out frame-time-wgpu.log
```

A smoke run under Xvfb with software rendering (`llvmpipe`) passed the code
path. Those times are not GPU times. They are not the measurement.

Results. Machine: NVIDIA GeForce RTX 4070 (discrete GPU), NVIDIA driver
615.78.08, OpenGL 4.6.0, Vulkan device API 1.4.351. `glxinfo` reports direct
rendering. Each run had 1000 textures, a 30 s span, a mean of 67.4 to 67.5 and
a maximum of 72 tiles drawn per frame, and no lost focus. No frame was over
100 ms.

| Renderer | Session | Frames | p50 | p95 | p99 | max | Result |
|----------|---------|--------|-----|-----|-----|-----|--------|
| glow | Wayland | 1116 | 26.70 ms | 27.19 ms | 39.91 ms | 40.66 ms | FAIL |
| wgpu | Wayland | 1703 | 13.58 ms | 27.31 ms | 27.83 ms | 28.17 ms | PASS |
| glow | X11 | 2250 | 13.36 ms | 15.91 ms | 21.21 ms | 25.81 ms | PASS |
| wgpu | X11 | 2250 | 13.37 ms | 14.96 ms | 15.74 ms | 18.11 ms | PASS |

The `glow` log does not name the driver. It reads "OpenGL through glow".
The `wgpu` logs name the adapter: `NVIDIA GeForce RTX 4070 (Vulkan,
DiscreteGpu)`.

On Wayland, the `glow` p50 of 26.70 ms is about two frame intervals. A few
frames near 40 ms put its p99 over the 33 ms limit. The cause is not known.
`wgpu` passes. Also on Wayland, `wgpu` has a p95 of 27.31 ms against a p50 of
13.58 ms, so about a third of the frames take two intervals. The cause is not
known. X11 shows no such effect. On X11, both pass and `wgpu` has the lower
p99. The renderer rule compares only within one session type. `glow` is never
more than 20 percent below `wgpu`, so `wgpu` stays the default.

## Reproduce

```sh
rustup target list --installed
cargo build --release
stat -c '%s' target/release/tarragon
cargo build --release --examples
stat -c '%s' target/release/examples/grid_bench target/release/examples/jpeg_q90_check target/release/examples/stack_spike
```
