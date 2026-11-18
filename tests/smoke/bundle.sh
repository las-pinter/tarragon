#!/bin/sh
# Pack the smoke bundle: one .tar.gz that the user copies to a machine with no
# Rust. It holds the release binaries, smoke.sh, the PSD fixture, the fetched
# PSD corpus, and a VERSION file. The script builds with --locked, never
# downloads, and never touches the network.
#
# Usage: sh tests/smoke/bundle.sh
# Output: /var/tmp/tarragon-smoke-<commit>.tar.gz
#         (with the override below: tarragon-smoke-<commit>-dirty.tar.gz)
#
# Rules:
#   - The tree must be clean, so the bundle always matches one commit.
#   - The PSD corpus must already be fetched (sh tests/psd-corpus/fetch.sh
#     tests/psd-corpus). Every file is checked against the sha256 in fetch.sh.
#   - The bundle is for the user only. Never commit or publish it: the licences
#     of the corpus files are unknown (see the header of
#     tests/psd-corpus/fetch.sh).
#   - The staging folder and a half-written archive are removed on exit. A
#     finished archive is not.
#
# Test-only override (set by tests, not by the user):
#   TARRAGON_BUNDLE_ALLOW_DIRTY  set to 1 to skip the clean-tree check. The
#                                script warns, and the archive name and the
#                                VERSION commit line end in -dirty.
set -eu

# Largest allowed archive, in bytes (150 MB).
max_bytes=157286400

# Print an error and leave.
die() {
    echo "error: $*" >&2
    exit 1
}

root="$(cd "$(dirname "$0")/../.." && pwd)"
manifest="$root/Cargo.toml"
corpus="$root/tests/psd-corpus"
fixture="$root/tests/fixtures/psd/rgb_8bit.psd"
target="${CARGO_TARGET_DIR:-$root/target}"

if command -v sha256sum >/dev/null 2>&1; then
    hash_of() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null 2>&1; then
    hash_of() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
    die "neither sha256sum nor shasum is installed"
fi

# The tree must be clean. Ignored files do not count.
suffix=""
if [ "${TARRAGON_BUNDLE_ALLOW_DIRTY:-}" = 1 ]; then
    echo "warning: dirty tree, test bundle" >&2
    suffix="-dirty"
elif [ -n "$(git -C "$root" status --porcelain)" ]; then
    die "the tree is dirty; commit or remove the changes first"
fi
commit="$(git -C "$root" rev-parse --short HEAD)$suffix"
out="/var/tmp/tarragon-smoke-$commit.tar.gz"

# Check the corpus before the long build. The list has one "hash  name" line
# for each fetch line of fetch.sh. Nothing is downloaded.
sums="$(sed -n 's/^fetch \([^ ]*\) .* \([0-9a-f]\{64\}\)$/\2  \1/p' "$corpus/fetch.sh")"
count="$(printf '%s\n' "$sums" | grep -c .)" || true
total="$(grep -c '^fetch ' "$corpus/fetch.sh")" || true
if [ "$count" -eq 0 ]; then
    die "no sha256 lines found in $corpus/fetch.sh"
fi
if [ "$count" -ne "$total" ]; then
    die "fetch.sh has $total fetch lines, but $count parsed"
fi
if grep '^fetch ' "$corpus/fetch.sh" | grep -qv '^fetch [^ ]* "\$'; then
    die "a corpus file name in fetch.sh has a space"
fi
printf '%s\n' "$sums" | while read -r want name; do
    [ -f "$corpus/$name" ] || die "corpus file missing: $name (run: sh tests/psd-corpus/fetch.sh tests/psd-corpus)"
    [ "$(hash_of "$corpus/$name")" = "$want" ] || die "sha256 mismatch: $name"
done
[ -f "$fixture" ] || die "fixture missing: $fixture"

# Release builds, locked.
cargo build --release --locked --manifest-path "$manifest" -p tarragon-app --bin tarragon
cargo build --release --locked --manifest-path "$manifest" -p tarragon-scan \
    --example scan_bench --example corpus_gen
cargo build --release --locked --manifest-path "$manifest" -p tarragon-render \
    --example psd_rss_bench

stage="$(mktemp -d /var/tmp/tarragon-bundle.XXXXXX)"
trap 'rm -rf "$stage" "$out.part"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir "$stage/bin" "$stage/fixtures" "$stage/psd-corpus"
cp "$target/release/tarragon" "$stage/bin/tarragon"
cp "$target/release/examples/scan_bench" "$stage/bin/scan_bench"
cp "$target/release/examples/corpus_gen" "$stage/bin/corpus_gen"
cp "$target/release/examples/psd_rss_bench" "$stage/bin/psd_rss_bench"
cp "$root/tests/smoke/smoke.sh" "$stage/smoke.sh"
cp "$fixture" "$stage/fixtures/rgb_8bit.psd"
printf '%s\n' "$sums" | while read -r _ name; do
    cp "$corpus/$name" "$stage/psd-corpus/$name"
done
cp "$corpus/fetch.sh" "$stage/psd-corpus/fetch.sh"
printf '%s\n' "$sums" >"$stage/psd-corpus/SHA256SUMS"

cat >"$stage/README.txt" <<'EOF'
1. Unpack this archive in an empty folder: tar xzf tarragon-smoke-*.tar.gz
2. Run: sh smoke.sh
3. Keep the app window visible and do not touch the mouse while it runs.
4. Type your sudo password when the script asks for it.
5. Send back smoke-result.txt and the perf-*.txt files.
EOF

{
    echo "commit $commit"
    rustc --version
    echo "build host $(ldd --version | sed -n 1p)"
    ldd "$stage/bin/tarragon" | grep 'libc\.so' | sed 's/^[[:space:]]*//; s/ (0x.*//'
} >"$stage/VERSION"

tar -C "$stage" -czf "$out.part" bin smoke.sh README.txt fixtures psd-corpus VERSION
size="$(wc -c <"$out.part" | tr -d ' ')"
if [ "$size" -gt "$max_bytes" ]; then
    die "archive is $size bytes, the limit is $max_bytes"
fi
mv -f "$out.part" "$out"
echo "wrote $out ($size bytes)"
