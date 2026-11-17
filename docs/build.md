# Build

This file records the pinned toolchain, the build commands, the link mode,
and the measured results.

## Pinned toolchain

`rust-toolchain.toml` pins the toolchain:

```toml
[toolchain]
channel = "1.99.0"
components = ["rustfmt", "clippy"]
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

Windows and macOS run `cargo build --release` and `cargo test`. Two workflows
exist:

- `.github/workflows/rust.yml` runs the gates, the Windows and macOS builds,
  and the PSD jobs.
- `.github/workflows/release-linux.yml` builds the Linux portable zip with
  `cargo zigbuild`, smoke-tests it, and, on a tag only, makes a draft release.

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

No CJK font is bundled, because the OFL license of the usual CJK fonts is off
the allowlist. Fonts the app reads from the system are not shipped, so the
allowlist does not apply to them. After the window opens, a thread scans the
OS font folders with `fontdb`, without fontconfig. On Linux the folders are
`/usr/share/fonts`, `/usr/local/share/fonts`, `~/.fonts`, and
`~/.local/share/fonts`. The log line `system fallback fonts:` lists the kept
faces. When no CJK face is found, the app logs the warning
`no system CJK font found; CJK file names may show as boxes`. The app skips a
face that it cannot parse and logs the skipped names. A panic on the thread is
caught and logged, and the app keeps Roboto. A kept face is read whole into
memory. A Noto Sans CJK `.ttc` file costs about 20 MB. The app keeps one face
for each file, so a .ttc file costs its size once. On Arch, a user can install
`noto-fonts-cjk`.

## Quality gates

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run
cargo deny check
```

All four gates pass on this branch.

## Link mode

The Linux release binary links glibc. CI builds it with `cargo zigbuild` for
glibc 2.31. The pins and the build command are in "Release build".

## Grid thumbnail tier

The grid tier is 320 px on the longest edge, and its cache folder is `320`. A
grid cell of 160 pt at a display scale of 2.0 needs 320 px. The thumbnail is
sharp up to scale 2.0, and soft above it.

## Measured sizes

The `tarragon-app` binary parses the `--version` and `--help` flags. Without a
flag it resolves the data directory, takes the single-instance guard, sets up
logging, and opens the database, then opens the shell window with eframe. It
tries the wgpu renderer first and falls back to glow. A second start ends after
the guard, before the log file is opened or rolled.

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

### Library of 100,000 rows

An ignored test loads a stored library of 100,000 rows (1,000 folders of 100
files) through the real pipeline and gallery. It runs headless: egui lays out
and tessellates, and no window opens. The render function makes 256 by 256
images, and the test scrolls the grid by wheel events while four workers
render.

```sh
cargo nextest run --release -p tarragon-app --run-ignored only --no-capture library_of_100k
```

Machine: Intel Core i7-12700K, 10 logical CPUs, 12 GiB RAM, Ubuntu 24.04,
kernel 6.17.0-35-generic, release build. The median and p95 agreed within 5
percent on two runs. The test asserts the five bars, so a regression fails it.

| Measure | Result | Bar | |
|---------|--------|-----|-----|
| `library_files` plus `open_library` (open thread) | 0.1 s (19 ms and 75 ms) | under 1 s | pass |
| `set_items` plus first `refresh_view` (UI thread, once) | 70 ms | under 250 ms | pass |
| Frame time, 301 frames while scrolling | median 17 ms, p95 20 ms, max 28 to 38 ms | under 100 ms | pass |
| Resident memory added by the library | 137 MiB (144 MiB after the scroll) | under 150 MiB | pass |
| Grid textures while scrolling | peak 252 MiB | under 256 MiB | pass |

Notes:

- The frame time is the whole app frame: result upload, `Gallery::show`,
  `after_draw`, and tessellation. It has no GPU work. A quiet frame at 10,000
  items costs under 1 ms in the grid benchmark, so the 17 ms median comes from
  the scroll work. The cause is not measured. A likely cause is the queue
  reorder that runs when the visible tiles change.
- The memory bar is checked after the scroll (144 MiB), so the margin is
  6 MiB. The memory includes the pipeline job table of 100,000 jobs. A larger
  library passes the bar only if this table shrinks.
- The seed stores one folder row for each folder. A scan stores the folder row
  only for the folder that it opens, so a stored library can have rows without
  a folder row. Causes are a trailing slash in the folder text, rows removed by
  `cleanup_stale_folder_uuids`, and an old database. Each missing folder costs
  one `get_or_create_folder_uuid` transaction. With one call for each folder,
  1,000 folders without folder rows made `open_library` take about 12 s on its
  first run. `open_library` now calls `get_or_create_folder_uuids`, which
  creates the missing folder ids in one transaction. A second ignored test
  (`library_of_100k_without_folder_rows_opens_fast`) seeds 1,000 folders with
  no folder row. Its `open_library` took 98 to 108 ms on three runs, with a
  bar of 1 s.
- The database has one connection behind a mutex. The writer thread locks it
  for each row. A library that opens during heavy row writes can wait for the
  lock. One wait of 0.9 s was seen once. No test measures it. A later fix is a
  batched writer transaction or a separate read connection.
- The page cache is warm. No cold-cache run was done.
- Follow-up: measure the queue reorder cost for each scroll step, to find the
  cause of the 17 ms frame.
- The test runs about 80 s, mostly seeding and scrolling.

### PSD memory benchmark

The `psd_rss_bench` example measures the peak resident memory of a scan that
includes PSD and PSB files. It builds a temp folder (10,000 files, 10 percent
PSD or PSB by default), scans it, renders every file through the registry on a
pool of 4 workers, and writes the three cache tiers into a temp cache. A gate
keeps at most 3 PSD or PSB renders in flight, which models the
`max_psd_workers` setting (default 3). It prints the file counts, the counts
per format, the errors by kind, the times, `VmHWM` (the peak), and `VmRSS`.

```sh
cargo build --release --example psd_rss_bench -p tarragon-render
cargo run --release --example psd_rss_bench -p tarragon-render -- \
    [--files 10000] [--psd-percent 10] [--workers 4] [--psd-gate 3] \
    [--source-dir DIR] [--large-psb FILE] [--work-dir DIR] [--no-write]
```

PASS means the peak is under 2 GiB (2,048 MiB) and no file gave an error. The
exit code is 1 on FAIL. The figures come from `/proc/self/status`, so other
systems print `n/a` and judge on errors alone. The workspace forbids unsafe
code, so the bench cannot call a system API for the peak.

The committed fixtures are small, so a default run is a floor, not the
answer. For a large PSB, run
`sh tests/fixtures/psd/build_large_psb.sh /tmp/large.psb` (it needs Python) and
pass the file with `--large-psb`. `--source-dir` adds every PSD and PSB file
directly inside a folder to the set the run cycles through.

`--work-dir DIR` sets the parent of the temp folder (default: the system temp
folder). The run prints the folder path at the start and removes it on exit. At
the start it also removes folders of earlier runs that died, and lists them. It
only removes directories named like its own, and only if older than one hour
with no live process. Without `/proc` it removes none. Do not share
`--work-dir` between hosts or containers.
`--no-write` skips the cache writes, which splits the render cost from the
write cost. The run prints the cache size, a warning when a hard link failed
and the file was copied instead, and the first cache write error. `--help`
lists the options.

Run over real PSD files, 2026-11-09, release build, Intel Core i7-12700K (10
logical CPUs, no SMT, in a KVM guest), Ubuntu. The command used the 9 corpus files of
[`docs/psd-corpus.md`](psd-corpus.md) and the 9 committed fixtures, 18 sources
in all, with `--files 10000 --psd-percent 10 --source-dir DIR`. This run came
after ZIP layer channels were decoded. The earlier run, before ZIP decoding,
skipped the layers of `16bits.psd`. Its results are in the last column:

| Measure | Result | Before ZIP decoding |
| --- | --- | --- |
| Files scanned, rendered | 10,000, 9,945 | 10,000, 9,945 |
| Formats | 4,500 `.jpg`, 4,500 `.png`, 889 `.psd`, 111 `.psb` | the same |
| Errors | 55, all of kind `Decode` | the same |
| Scan time | 0.10 s | 0.26 s |
| `render+write` | 2.92 s | 1.98 s |
| `wall time` | 3.02 s | 2.24 s |
| Render, summed over workers | 1.81 s | 1.06 s |
| `VmHWM` (peak) | 35.3 MiB | 29.7 MiB |
| `VmRSS` (current) | 18.4 MiB | 20.5 MiB |
| Verdict | `FAIL` | `FAIL` |

The peak is 35.3 MiB against the limit of 2,048 MiB. It rose by 5.6 MiB. The run
did not measure the cause of the change in the memory figures or in the time
figures. The FAIL verdict is not a
memory failure. It follows from the 55 errors, because PASS needs zero errors.
`32bits.psd` is the only corpus file that gives a `Decode` error: the decoder
does not support 32-bit RGB. The bench reports errors by kind only. It cycles
through its 18 sources, so 1,000 PSD and PSB files hold 55 copies of that file.
This matches the 55 decode errors. The link is an inference, not a per-file
count. The files and their outcomes are in
[`docs/psd-corpus.md`](psd-corpus.md).

### Cold-cache scan of the 10k corpus

`scan_bench --folder DIR` scans an existing folder, never deletes it, and
prints PASS or FAIL against 120 s (exit code 1 on FAIL, 2 on a bad folder).

1. Make the corpus: 7,000 PNG, 2,000 JPEG, and 1,000 PSD copies. Each PSD copy
   has its own inode.

   ```sh
   cargo run --release --example corpus_gen -p tarragon-scan -- \
       --out /var/tmp/tarragon-10k \
       --psd-file tests/fixtures/psd/rgb_8bit.psd --psd-percent 10
   ```

2. Pick 4 physical cores. List them with `lscpu -p=CPU,CORE`. Take the first
   logical CPU of each of the first four cores. With SMT these are not `0-3`.
   `scan_bench` caps its pool at 4 threads anyway.
3. Cold run, with root. This drops the data, dentry, and inode caches. A stat
   scan needs the dentry and inode caches to be cold:

   ```sh
   sync && echo 3 | sudo tee /proc/sys/vm/drop_caches
   ```

4. Partly cold run, without root. This drops file data only. The dentry and
   inode caches stay warm. It needs GNU `find`, which accepts `{}` inside
   `if={}`:

   ```sh
   find /var/tmp/tarragon-10k -type f -exec dd if={} of=/dev/null iflag=nocache count=0 status=none \;
   ```

5. Run the scan right after step 3 or 4. Run it a second time to get the warm
   figure. Use the CPU list from step 2:

   ```sh
   taskset -c <list> cargo run --release --example scan_bench -p tarragon-scan -- \
       --folder /var/tmp/tarragon-10k
   ```

6. Remove the corpus: `rm -r /var/tmp/tarragon-10k`.

Machine: Intel Core i7-12700K, Ubuntu 24.04, kernel 6.17.0-35-generic,
release build, 2026-11-04. This machine has no SMT (1 thread per core), so
CPUs 0 to 3 are four cores and the list was `0,1,2,3`.

| Cache state | Scan | Write | Total | Result (120 s) |
|-------------|------|-------|-------|----------------|
| Cold | not run: needs root | | | |
| Partly cold (step 4) | 0.066 s | 0.038 s | 0.104 s | PASS |
| Warm (next run) | 0.057 s | 0.036 s | 0.093 s | PASS |

Do not read the partly cold row as a cold figure.

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

- The binary is smaller.
- The dependency tree is smaller.
- It works on old GL drivers and in virtual machines that have no Vulkan.

Cost of keeping both: the combined build is larger than the `wgpu` build.

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
newer image raises the floor to the glibc of that image, at most. The table is
a record of an old test. The release build no longer depends on the image
glibc (see "Release build" below). The system libraries that the binary loads
at run time (X11, Wayland, EGL, Vulkan) are not in this number.

Packages that the container build needed (Debian and Ubuntu names):
`build-essential`, `ca-certificates`, `curl`, and `binutils`. No `-dev`
package was needed. The window and graphics libraries load at run time.

Packages for a run in a container (not tested as a minimal set):
`libx11-6`, `libxcursor1`, `libxrandr2`, `libxi6`, `libxkbcommon0`,
`libxkbcommon-x11-0`, `libwayland-client0`, `libegl1`, `libgl1`,
`libgl1-mesa-dri`, `libvulkan1`, and `mesa-vulkan-drivers`.

Optional run-time packages, for any install of the app. They are not build
packages. File Copy and Paste in the grid menu need `xclip` on X11 or
`wl-clipboard` on Wayland. Without them the app runs and shows a notice that names
the missing package. The Show in file manager item and the system opener start
`xdg-open` on Linux (package `xdg-utils` on Debian and Ubuntu).

The CI `gates` job needs no new `apt` step. The example built on a plain
`ubuntu:22.04` image with `build-essential` only. The `release-build` jobs
for Windows and macOS run `cargo build --release --workspace`. That command
builds no example and no dev-dependency, so the new crates do not enter it.

#### Release build

`.github/workflows/release-linux.yml` builds the Linux portable zip. It runs on
a push to `rust`, on a `v*` tag, and by hand. It has three jobs:

- `build` runs in an `ubuntu:26.04` container. It builds, checks, and packs
  the zip, and uploads the zip and its sha256 file as one workflow artifact.
- `smoke` starts the unpacked binary in `ubuntu:20.04` and `debian:11`.
- `publish` runs only on a `v*` tag, after `build` and `smoke` pass. It
  creates a draft release. The owner reads the draft and publishes it by hand.

The build uses these pins:

- Rust: the toolchain in `rust-toolchain.toml` (1.99.0).
- Zig: `ziglang==0.17.0`, from pip in a venv.
- `cargo-zigbuild` 0.24.0, installed with `--locked`.
- Target: `x86_64-unknown-linux-gnu.2.31`. The `.2.31` suffix makes Zig link
  against glibc 2.31, whatever glibc the image has.

The build command is:

```sh
cargo zigbuild --release --locked -p tarragon-app --target x86_64-unknown-linux-gnu.2.31
```

Fallback if Zig 0.17 fails: use Zig 0.16.x with `cargo-zigbuild` 0.23.4. Never
mix a `cargo-zigbuild` below 0.23 with Zig 0.17. PyPI marks the `ziglang`
0.17.0 wheel as yanked (a partial upload). `pip` still installs it, with a
warning.

Two flags break the release build: `linker-plugin-lto` and `crt-static`.
`cargo-zigbuild` does not support `crt-static` for glibc targets.
`linker-plugin-lto` fails with `unsupported linker arg: -plugin-opt`. The
`build` job fails when either one is set in a Cargo config, a `Cargo.toml`,
or the workflow file. It also fails when `RUSTFLAGS` is set.

The `build` job checks each result and fails on a miss:

- The highest `GLIBC_` symbol from `objdump -T` must not pass 2.31.
- `ldd` may list only `linux-vdso.so.1`, `libc.so.6`, `libm.so.6`,
  `libpthread.so.0`, `libdl.so.2`, and `ld-linux-x86-64.so.2`.
- The binary must be at most 62,914,560 bytes (60 MB).
- `tarragon --version` must print `tarragon` and the workspace version.
- On a tag run, the tag must equal `v` and the workspace version.

Measured on a push run of this workflow (cold cache, version 0.2.0):

| Measure | Result |
|---------|--------|
| Highest `GLIBC_` symbol | 2.30 (limit 2.31) |
| `ldd` list | `linux-vdso.so.1`, `libm.so.6`, `libc.so.6`, `/lib64/ld-linux-x86-64.so.2`, `libpthread.so.0`, `libdl.so.2` |
| `cargo zigbuild --release` | 2 min 54 s |
| Smoke in `ubuntu:20.04` and `debian:11` | `--version` prints `tarragon 0.2.0`, `--help` exits 0 |

The binary does not link `libgcc_s`.

Limits of the test:

- RHEL 8 (glibc 2.28) is not covered. The floor is glibc 2.31.
- The smoke test runs `--version` and `--help` only. A GUI start on old
  distros is not tested.
- The window and graphics libraries (X11, Wayland, EGL, Vulkan) load at run
  time. The `ldd` check does not see them.

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
cargo build --release
cargo build --release --examples
```

## End-to-end smoke test

`tests/smoke/smoke.sh` is a POSIX `sh` script. It only measures. It prints one
row per check (`PASS`, `FAIL`, or `INFO`), then a `SUMMARY` block. The exit code
is 0 only when no row is `FAIL`. The output is also written to
`smoke-result.txt`, and each app run keeps its perf log as `perf-<row>.txt`.
`sh smoke.sh --check-only` prints the memory line and the CPU pin, then stops.
It runs no drop, no scan, and no app.

Rows and bars:

| Row | Bar |
|-----|-----|
| `corpus` | the 10k corpus is made or reused; `FAIL` stops the run |
| `pin` | `FAIL` with fewer than 4 physical cores; stops the run |
| `scan_cold` | `scan_bench` on the 10k corpus, 120 s, right after the cache drop |
| `cold_start` | under 3000 ms, from spawn to the first frame with the grid shown |
| `peak_rss` | under 2 GiB (2,097,152 KiB) |
| `p99_wayland` | frame run under Wayland: p99 under 33 ms, no frame over 100 ms |
| `p99_xwayland` | same run with `WAYLAND_DISPLAY` unset; `INFO` only, never a bar |
| `psd_rss_bench` | `INFO` only; the numbers are for the record |

How it runs:

- Corpus: 10,000 files, 10 percent PSD copies, in `/var/tmp/tarragon-10k` or
  the folder given as the argument. A folder that holds exactly that many files
  from an earlier run is reused. Any other folder with files fails the row. The
  script never deletes a folder.
- Pin: the first logical CPU of each of the first four physical cores, from
  `lscpu -p=CPU,CORE`. The scan, the cold start, and both frame runs run under
  `taskset` with that list, and so does `psd_rss_bench`. The pin row shows the core type. The script fails
  when the machine has fewer than 4 physical cores.
- Sudo step: before the scan and again before the cold start, the script runs
  `sync` and drops the caches with `sudo`. It asks for the password. It prints
  `cache COLD` only when the drop works. Otherwise it prints `cache PARTLY
  COLD` and the cold rows fail.
- App runs: each has a 300 s time limit, a new data folder, and the pin. The
  rows read the perf log that the app writes, never the screen. The two frame
  runs take about 40 s each. Keep the window visible and do not touch the mouse.
  The p99 run is valid only with about 1000 or more live textures.
- Wayland: the run must report a Wayland session, or the row fails. The
  XWayland run needs `DISPLAY`; without it the row is skipped.
- Memory: the script prints `free -g` for the record. It has no guard and no
  cap.

To run it on a machine with no Rust, build the bundle on the build machine. Run
`sh tests/psd-corpus/fetch.sh tests/psd-corpus` once first. Then run:

```sh
sh tests/smoke/bundle.sh
```

The script refuses a dirty tree. It checks the sha256 of each corpus file, never
downloads, and builds the four release binaries with `--locked`. It writes
`/var/tmp/tarragon-smoke-<commit>.tar.gz` and removes its staging folder. The
archive holds `bin/` (`tarragon`, `scan_bench`, `corpus_gen`,
`psd_rss_bench`), `smoke.sh`, `README.txt`, `fixtures/rgb_8bit.psd`,
`psd-corpus/` (16 files, `fetch.sh`, `SHA256SUMS`), and `VERSION`. The archive
is under 150 MB.

Copy the archive to the target machine, unpack it in an empty folder, and run
`sh smoke.sh`. Send back `smoke-result.txt` and the `perf-*.txt` files. The
binaries are built on the build machine, so the target needs glibc 2.39 or
newer. `VERSION` records the commit, the rustc version, and the libc.

Never commit or publish the bundle. The licences of the corpus files are
unknown (see `psd-corpus.md`).
