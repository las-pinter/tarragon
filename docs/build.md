# Build

This file records the pinned toolchain, the build commands, the link mode,
and the benchmarks.

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
  `cargo zigbuild`, checks it on old distros and on Arch Linux, and, on a tag
  only, makes a draft release.

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

## Runtime packages

These are run-time packages for any install of the app. They are not build
packages. The binary loads the window and graphics libraries at run time, so
the `ldd` check in "Release build" does not list them. Windows and macOS need
no extra package. Copy and Paste in the grid use the Linux helper tools below.
They are not supported yet on Windows and macOS.

| Need | Ubuntu package | Arch package | Required |
|------|----------------|--------------|----------|
| X11 libraries | `libx11-6`, `libxcursor1`, `libxrandr2`, `libxi6` | `libx11`, `libxcursor`, `libxrandr`, `libxi` | Yes, on X11 |
| Wayland client library | `libwayland-client0` | `wayland` | Yes, on Wayland |
| Keyboard maps | `libxkbcommon0`, `libxkbcommon-x11-0` | `libxkbcommon`, `libxkbcommon-x11` | Yes (the X11 part on X11 only) |
| EGL and OpenGL | `libegl1`, `libgl1`, `libgl1-mesa-dri` | `libglvnd`, `mesa` | One of this row or the Vulkan row: the fallback renderer |
| Vulkan loader and one driver | `libvulkan1`, `mesa-vulkan-drivers` | `vulkan-icd-loader`, and one Vulkan driver for the GPU | One of this row or the EGL row: the default renderer |
| Folder picker: portal and one backend | `xdg-desktop-portal`, `xdg-desktop-portal-gtk` | `xdg-desktop-portal`, `xdg-desktop-portal-gtk` | Yes, for the folder picker |
| Open in editor and Show in file manager (`xdg-open`, `xdg-mime`) | `xdg-utils` | `xdg-utils` | Yes, for those items |
| Copy and Paste in the grid menu | `xclip` (X11) or `wl-clipboard` (Wayland) | `xclip` (X11) or `wl-clipboard` (Wayland) | Optional |
| CJK font | `fonts-noto-cjk` | `noto-fonts-cjk` | Optional |

The folder picker talks to the desktop portal at run time. Without `xclip` or
`wl-clipboard`, the app runs and shows a notice that names the missing package.
Without a CJK font, CJK file names can show as boxes (see "Vendored font").

## Grid thumbnail tier

The grid tier is 320 px on the longest edge, and its cache folder is `320`. A
grid cell of 160 pt at a display scale of 2.0 needs 320 px. The thumbnail is
sharp up to scale 2.0, and soft above it.

## Benchmarks

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
thumbnails and the decoded images behind them to the measured cost.

The text path is the vendored Roboto family, installed through
`tarragon_ui::install`, so the measure includes the shipped text layout. Each
pass runs 400 frames after 40 warmup frames. The command prints its results and
asserts no bar.

```sh
cargo run --release --example grid_bench -p tarragon-ui
```

### Library of 100,000 rows

An ignored test loads a stored library of 100,000 rows (1,000 folders of 100
files) through the real pipeline and gallery. It runs headless: egui lays out
and tessellates, and no window opens. The render function makes 256 by 256
images, and the test scrolls the grid by wheel events while four workers
render.

```sh
cargo nextest run --release -p tarragon-app --run-ignored only --no-capture library_of_100k
```

The test asserts these bars, so a regression fails it:

- Together, the two database reads on the open thread take under 1 s.
- The load into the gallery on the UI thread takes under 250 ms.
- No frame while scrolling takes 100 ms or more.
- The resident memory added by the library, after the scroll, is under 150 MiB.
- The peak grid textures stay at or under the 256 MiB grid texture budget.

A second ignored test, `library_of_100k_without_folder_rows_opens_fast`, seeds
1,000 folders with no folder row. Its `open_library` call must take under 1 s.

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

PASS means the peak is under 2 GiB (2,048 MiB) and no file fails to render. The
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

To run over real PSD files, fetch the corpus once, then pass its folder with
`--source-dir`. The header of `tests/psd-corpus/fetch.sh` describes the corpus.
Every run also uses the valid fixtures in
`tests/fixtures/psd/` (all but `truncated.psd`). `--source-dir` adds the PSD and
PSB files directly inside its folder to them. A file that the decoder cannot
read counts as a render error, and any render error makes the run FAIL.

```sh
sh tests/psd-corpus/fetch.sh tests/psd-corpus
cargo run --release --example psd_rss_bench -p tarragon-render -- \
    --files 10000 --psd-percent 10 --source-dir tests/psd-corpus
```

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

5. Run the scan right after step 3 or 4. Run it a second time for the warm
   case. Use the CPU list from step 2:

   ```sh
   taskset -c <list> cargo run --release --example scan_bench -p tarragon-scan -- \
       --folder /var/tmp/tarragon-10k
   ```

6. Remove the corpus: `rm -r /var/tmp/tarragon-10k`.

## GUI stack spike

The spike checked the window stack before the app shell was wired in. The code
is the example `crates/tarragon-app/examples/stack_spike/`. `eframe` is a normal
dependency of `tarragon-app`, because the app window uses it. `tarragon-ui`
depends on `egui_dock`, so the app binary links it. `tarragon-app` also lists it
as a dev-dependency for the spike.

The workspace pins `eframe =0.36.2` with default features off. The features
are `glow`, `wgpu`, `x11`, and `wayland`. `default_fonts` stays off, and the
example installs the vendored Roboto fonts. It adds `egui_dock 0.21.1`. The
example picks the renderer at run time through `NativeOptions`.

### Renderer

Decision: `wgpu` is the default renderer. `glow` is the fallback: the app
tries it when `wgpu` fails to start. The spike example picks either one with
`--renderer`.

Rule: `wgpu` stays the default unless one of these is true on a frame-time run
(see "Frame time"):

- `wgpu` fails a limit (p99 of 33 ms or more, or a frame over 100 ms) and
  `glow` passes.
- `glow` is clearly better: its p99 is more than 20 percent lower than the
  `wgpu` p99 on the same machine and in the same session type.

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

The `accesskit` feature of eframe is off. The app must decide on it, because
it adds screen reader support. This is an open item.

### Dependency check

`cargo deny check` runs in the quality gates and covers the whole tree.

### glibc floor

The release is built with `cargo zigbuild` for glibc 2.31, so the floor is
glibc 2.31 whatever glibc the build image has. The release build reads the
highest `GLIBC_` symbol version of the binary with `objdump -T` and fails when
it is above 2.31 (see "Release build"). The system libraries that the binary
loads at run time (X11, Wayland, EGL, Vulkan) are not in this number.

#### Release build

`.github/workflows/release-linux.yml` builds the Linux portable zip. It runs on
a push to `rust`, on a `v*` tag, and by hand. It has four jobs:

- `build` runs in an `ubuntu:26.04` container. On a `v*` tag it runs
  `cargo xtask attribution` before the build and fails when
  `assets/attribution.svg` differs from the committed file. It builds, checks,
  and packs the zip, and uploads the zip and its sha256 file as one workflow
  artifact.
- `smoke` starts the unpacked binary in `ubuntu:20.04` and `debian:11`.
- `arch` (Arch Linux check) runs the unpacked binary in `archlinux:latest`. It
  checks `--version` and `--help`. Then it installs the run-time packages with
  `pacman`, starts the window under Xvfb on a small folder, and checks that the
  app finds a system CJK font. The job gates the release.
- `publish` runs only on a `v*` tag, after `build`, `smoke`, and `arch` pass. It
  creates a draft release. A maintainer reviews the draft and publishes it by
  hand.

A red `arch` job stops the draft release. Nothing skips it. If `pacman` fails
on a mirror error, run the job again. For any other failure, fix the cause (for
example a package name in the job), do the steps in "Before a release tag"
again, and make a new tag.

Before a release tag, do these steps in this order:

1. Finish and commit every other change, so the tree is clean apart from the
   chart. The tool counts uncommitted lines as human, so a chart made over
   uncommitted work goes stale after the commit.
2. Run `cargo xtask attribution` last.
3. Commit only `assets/attribution.svg`.
4. Tag that commit.

The chart counts the tracked text files, so a later change to a tracked text
file outside `assets/` makes it stale and the tag run fails. The check needs
the full history, so the checkout uses `fetch-depth: 0`.

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
- On a tag run, `assets/attribution.svg` must match the committed file.

Limits of the test:

- RHEL 8 (glibc 2.28) is not covered. The floor is glibc 2.31.
- The smoke test runs `--version` and `--help` only. A GUI start on old
  distros is not tested.
- The Arch check starts a window under X11 (Xvfb) only. Wayland, a real
  graphics driver, and the folder picker are not covered there. A real GUI
  check stays by hand.
- The window and graphics libraries (X11, Wayland, EGL, Vulkan) load at run
  time. The `ldd` check does not see them.

### egui_dock

The workspace pins `egui_dock =0.21.1`, which needs `egui ^0.36`.

### Frame time

Run the check on a local desktop with a GPU. Do not run it over an SSH X
forward. That usually renders in software and has no DRI3 direct path.

The example uploads 1000 textures of 256 by 256 pixels. All 1000 stay alive
for the whole run. It scrolls the grid by itself, and only the visible rows
draw. The log header records the mean and the maximum number of tiles drawn in
each frame.

It records the time between frames for 30 s after a 5 s warm-up. It writes a
log file and prints p50, p95, p99, max, the frame count, the renderer, and the
session type. The log header shows the real measured span (the sum of the
frame times), not only the configured duration. The exit code is 0 on a pass,
1 on a fail, and 2 when the window does not open, the run is not valid, or the
log cannot be written. The limits are a p99 under 33 ms and no frame over
100 ms (hard fail).

Vsync is on. On a 60 Hz display one dropped frame takes two frame intervals.
So more than 1 percent dropped frames puts the p99 at 33 ms or more, and the
run fails. The logs do not record the refresh rate.

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
`psd-corpus/` (16 files, `fetch.sh`, `SHA256SUMS`), and `VERSION`. The script
fails when the archive is over 150 MB.

Copy the archive to the target machine, unpack it in an empty folder, and run
`sh smoke.sh`. Send back `smoke-result.txt` and the `perf-*.txt` files. The
binaries are built on the build machine, so the target needs a glibc at least
as new as the one on the build machine. `VERSION` records the commit, the rustc
version, and the libc.

Never commit or publish the bundle. The licences of the corpus files are
unknown (see the header of `tests/psd-corpus/fetch.sh`).
