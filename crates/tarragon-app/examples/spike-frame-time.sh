#!/bin/sh
# Frame-time run for the GUI stack spike. Needs no root.
#
# The script builds the spike example in release mode. Then it runs the
# example once for each renderer (glow and wgpu) in the current session. Each
# run opens a real window, so run the script on a desktop with a GPU.
#
# Keep the window visible and not covered during each run. Do not minimize it
# and do not put another window over it. A minimized or covered window stops
# frame callbacks, so the run is marked not valid. A lost focus does not stop
# the frames. The log only records how many frames had no focus.
#
# Usage:
#   crates/tarragon-app/examples/spike-frame-time.sh [OUT_DIR] [DURATION_SECONDS]
#
# OUT_DIR defaults to ./spike-frame-time. DURATION defaults to 30.
#
# Run under X11
# -------------
# On a Wayland desktop the window opens under Wayland. To run under X11 use one
# of these:
#   1. Force XWayland: run `env -u WAYLAND_DISPLAY` before the script path.
#      This needs DISPLAY to be set, which is true under XWayland.
#   2. Log in to an X11 session and run the script there.
# Run the script once in each session type. Use a different OUT_DIR for each.
# The log header line `# session: x11` proves that the run used X11.
#
# Exit code: 0 when both runs pass, 1 when a run fails the limits, 2 when a
# window cannot open or a run is not valid (including a timeout).
set -u

out_dir=${1:-spike-frame-time}
duration=${2:-30}
warmup=5
case $duration in
    '' | *[!0-9]*)
        echo "DURATION must be a whole number of seconds, not '$duration'" >&2
        exit 2
        ;;
esac
root=$(cd "$(dirname "$0")/../../.." && pwd)
manifest="$root/Cargo.toml"
# A run that takes longer than this is stuck. It is not valid.
limit=$((warmup + duration + 60))

mkdir -p "$out_dir"
echo "session: XDG_SESSION_TYPE=${XDG_SESSION_TYPE:-unset} WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-unset} DISPLAY=${DISPLAY:-unset}"
if command -v glxinfo >/dev/null 2>&1; then
    glxinfo -B > "$out_dir/glxinfo.txt" 2>&1
    echo "glxinfo saved to $out_dir/glxinfo.txt"
fi
if command -v vulkaninfo >/dev/null 2>&1; then
    vulkaninfo --summary > "$out_dir/vulkaninfo.txt" 2>&1
    echo "vulkaninfo saved to $out_dir/vulkaninfo.txt"
fi

cargo build --release --example stack_spike -p tarragon-app \
    --manifest-path "$manifest" || exit 2
# Ask cargo for the target directory, so no path is assumed. This also follows
# CARGO_TARGET_DIR and the cargo config.
target_dir=$(cargo metadata --no-deps --format-version 1 --manifest-path "$manifest" |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
bin="$target_dir/release/examples/stack_spike"
if [ ! -x "$bin" ]; then
    echo "cannot find the example binary: $bin"
    exit 2
fi

status=0
for renderer in glow wgpu; do
    echo "== $renderer, $duration s"
    timeout "$limit" "$bin" \
        --renderer "$renderer" --warmup "$warmup" --duration "$duration" \
        --out "$out_dir/frame-time-$renderer.log"
    code=$?
    if [ "$code" -eq 124 ]; then
        echo "the run timed out after $limit s: not valid"
        code=2
    fi
    echo "exit code: $code"
    if [ "$code" -gt "$status" ]; then
        status=$code
    fi
done
exit "$status"
