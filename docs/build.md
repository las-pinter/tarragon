# Build

This file records the pinned toolchain, the build commands, the link mode
(Q17), the measured binary sizes, and the 60 MB check (Q6).

## Pinned toolchain

`rust-toolchain.toml` pins the toolchain:

```toml
[toolchain]
channel = "1.99.0"
components = ["rustfmt", "clippy", "rust-analyzer"]
```

The workspace sets `rust-version = "1.99"` and `edition = "2024"` (Q7).
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

Windows and macOS run `cargo build --release` and `cargo test` (Q12, task 36).
Task 37 builds the Linux portable zip.

## Quality gates

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run
cargo deny check
```

All four gates pass on this branch.

## Link mode (Q17)

Q17 says: link musl when every crate allows; otherwise link glibc and record
the size. The result of this spike is glibc. The musl path stays the goal.

Facts from this run:

- The musl target is not installed. The toolchain directory lists only
  `x86_64-unknown-linux-gnu`. The sandbox denies the `rustup` command, so
  `rustup target list --installed` and `rustup target add
  x86_64-unknown-linux-musl` did not run.
- The musl build fails with `error[E0463]: can't find crate for 'core'` and
  `can't find crate for 'std'`. The failure is the missing target std, not a
  crate that needs glibc.
- No crate is proven to block musl. The musl build cannot run until the target
  std is installed. Task 37 must install the target and rebuild.

Note: `tarragon-db` will use `rusqlite` with `bundled` (Q21). A bundled C build
for musl needs a musl C toolchain, such as `musl-gcc`. That toolchain is absent
here. Verify it at task 7. The `tarragon-db` crate has no dependency yet.

## Measured sizes

Host release, glibc (`cargo build --release`):

| Binary | Bytes | MiB |
|--------|-------|-----|
| `target/release/tarragon-app` | 444,704 | 0.42 |

Musl release: not built. The target std is absent.

### Scope of the measurement

`tarragon-app` is a scaffold. Its manifest has no workspace crate dependency
yet, and `main.rs` prints the version only. The release binary links std only.
Task 34 adds the eframe window and the crate wiring.

The two heavy crates are measured through their examples. Run
`cargo build --release --examples`.

| Example | Crates | Bytes | MiB |
|---------|--------|-------|-----|
| `grid_spike` | egui 0.36.2, rayon | 5,112,632 | 4.88 |
| `jpeg_q90_check` | image 0.25.10 | 1,991,648 | 1.90 |

Task 37 records the final size of the wired binary.

## 60 MB check (Q6)

The limit is 60 MB. Take 60 MB as 62,914,560 bytes.

| Binary | Bytes | Result |
|--------|-------|--------|
| `tarragon-app` (glibc, release) | 444,704 | pass |
| `grid_spike` (example) | 5,112,632 | pass |
| `jpeg_q90_check` (example) | 1,991,648 | pass |

The highest measured size is 5,112,632 bytes. That is 8.1% of the 60 MB budget.
Every measured binary is under the limit.

## Reproduce

```sh
rustup target list --installed
cargo build --release
stat -c '%s' target/release/tarragon-app
cargo build --release --examples
stat -c '%s' target/release/examples/grid_spike target/release/examples/jpeg_q90_check
```
