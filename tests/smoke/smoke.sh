#!/bin/sh
# Smoke script for the user's machine. It only measures. It prints one line
# per row: PASS name, FAIL name, or INFO name, then a summary block. The exit
# code is 0 only when no row is FAIL. The whole output is also written to
# smoke-result.txt in the current folder, and each app run keeps its perf log
# as perf-<row>.txt beside it.
#
# Usage: sh smoke.sh [--check-only] [DIR]
#   --check-only  print the memory line and the CPU pin, then stop. No cache
#                 drop, no corpus, no scan, no app run, no result file.
#   DIR           corpus folder (default /var/tmp/tarragon-10k). It must be
#                 absent or empty, or hold exactly the expected number of
#                 files from an earlier run (then it is reused). The script
#                 never deletes a folder.
#
# Layout of the base folder (the folder that holds this script, or
# $TARRAGON_SMOKE_BASE): bin/corpus_gen, bin/scan_bench, bin/tarragon,
# bin/psd_rss_bench, and the PSD file fixtures/rgb_8bit.psd.
#
# The app rows read the perf log that the app writes. They never read the
# screen. Each app run has a time limit, a fresh data folder, and the same
# CPU pin as the scan. The two frame runs need the window visible and the
# mouse still for about 40 s.
#
# Needs: lscpu, taskset, timeout, sudo, free.
#
# Test-only overrides (set by the tests, not by the user):
#   TARRAGON_SMOKE_LSCPU_CORES  file that replaces `lscpu -p=CPU,CORE`
#   TARRAGON_SMOKE_LSCPU_FREQ   file that replaces `lscpu -e=CPU,CORE,MAXMHZ`
#   TARRAGON_SMOKE_DROP_CMD     shell command that replaces the root drop
#   TARRAGON_SMOKE_FILES        corpus size (default: the corpus_gen default)
#   TARRAGON_SMOKE_APP          path of the app binary (default: bin/tarragon)
#   TARRAGON_SMOKE_APP_TIMEOUT  time limit of one app run in seconds (default 300)
#                              It also limits the psd_rss_bench run. A frame run
#                              is the load plus 40 s, so the limit is generous.
set -u

pass_count=0
fail_count=0
info_count=0
# One line per PASS or FAIL row, for the summary block.
rows=""

# Print one result row and count it. $1 is PASS, FAIL or INFO. The rest is
# the row text.
row() {
    kind="$1"
    shift
    echo "$kind $*"
    case "$kind" in
        PASS) pass_count=$((pass_count + 1)) ;;
        FAIL) fail_count=$((fail_count + 1)) ;;
        *) info_count=$((info_count + 1)) ;;
    esac
    if [ "$kind" != INFO ]; then
        rows="$rows  $kind $1
"
    fi
}

# Print the summary block and leave with the right exit code.
finish() {
    echo "SUMMARY pass=$pass_count fail=$fail_count info=$info_count"
    printf '%s' "$rows"
    if [ "$fail_count" -eq 0 ]; then
        exit 0
    fi
    exit 1
}

# Print the CPU table that `lscpu -p=CPU,CORE` gives, or the test file.
cpu_cores() {
    if [ -n "${TARRAGON_SMOKE_LSCPU_CORES:-}" ]; then
        cat "$TARRAGON_SMOKE_LSCPU_CORES"
    else
        LC_ALL=C lscpu -p=CPU,CORE
    fi
}

# Print the frequency table that `lscpu -e=CPU,CORE,MAXMHZ` gives, or the
# test file.
cpu_freq() {
    if [ -n "${TARRAGON_SMOKE_LSCPU_FREQ:-}" ]; then
        cat "$TARRAGON_SMOKE_LSCPU_FREQ"
    else
        LC_ALL=C lscpu -e=CPU,CORE,MAXMHZ
    fi
}

# Print the first logical CPU of each of the first four physical cores, as a
# comma list. Print fewer entries when the machine has fewer cores.
pin_list() {
    cpu_cores | awk -F, '
        /^#/ || NF < 2 { next }
        !($2 in seen) && n < 4 { seen[$2] = 1; n++; list = list (n > 1 ? "," : "") $1 }
        END { print list }'
}

# Print the core type of the CPUs in the list $1: P-core when every CPU has
# at least 90 percent of the highest MAXMHZ on the machine, else "mixed" or
# "unknown" (no frequency data, for example in a VM).
core_type() {
    cpu_freq | awk -v list="$1" '
        BEGIN { n = split(list, want, ","); for (i = 1; i <= n; i++) pick[want[i]] = 1 }
        $1 ~ /^[0-9]+$/ && $3 ~ /^[0-9.]+$/ {
            f[$1] = $3 + 0
            if (f[$1] > max) max = f[$1]
        }
        END {
            if (max == 0) { print "unknown"; exit }
            high = 0; low = 0
            for (c in pick) {
                if (!(c in f)) { print "unknown"; exit }
                if (f[c] >= 0.9 * max) high++; else low++
            }
            if (low == 0) print "P-core"; else if (high == 0) print "E-core"; else print "mixed"
        }'
}

# Drop the data, dentry and inode caches with root. Return the exit code of
# the drop command.
drop_cache() {
    if [ -n "${TARRAGON_SMOKE_DROP_CMD:-}" ]; then
        sh -c "$TARRAGON_SMOKE_DROP_CMD"
    else
        sudo sh -c 'echo 3 > /proc/sys/vm/drop_caches'
    fi
}

# Print the value of the key $2 from the perf log $1, or nothing.
perf_val() {
    sed -n "s/^$2: *//p" "$1" | sed -n 1p
}

# Add the reason that the app timed out when the exit code $1 says so.
timeout_note() {
    if [ "$1" -eq 124 ]; then
        echo ", timed out after $app_timeout s"
    fi
}

# Run the app once on the corpus under the pin and the time limit. $1 is the
# perf log file, $2 is "load" (close when every tile is done) or "frame" (the
# frame run), $3 is "wayland" or "xwayland" (no WAYLAND_DISPLAY). The data
# folder is new, so the database file is new. Return the exit code. The app
# output is dropped; the perf log is the result. The limit runs with
# --foreground, so the app stays in the script's process group and Ctrl-C
# reaches it. The app is the direct child of timeout, so the kill at the limit
# still reaches it.
run_app() {
    rm -f "$1"
    data="$(mktemp -d "$work/data.XXXXXX")"
    (
        unset TARRAGON_FRAME_RUN TARRAGON_CLOSE_WHEN_LOADED
        export XDG_DATA_HOME="$data" TARRAGON_PERF_LOG="$1"
        if [ "$2" = frame ]; then
            export TARRAGON_FRAME_RUN=1
        else
            export TARRAGON_CLOSE_WHEN_LOADED=1
        fi
        if [ "$3" = xwayland ]; then
            exec taskset -c "$pin" timeout --foreground "$app_timeout" env -u WAYLAND_DISPLAY "$app" "$dir"
        fi
        exec taskset -c "$pin" timeout --foreground "$app_timeout" "$app" "$dir"
    ) >/dev/null 2>&1
}

# Judge a frame run from the perf log $1 and the app exit code $2. Set
# frame_pass to 1 or 0, frame_reason to the reasons for a miss (or nothing),
# and frame_text to the numbers.
frame_check() {
    frame_pass=0
    frame_reason=""
    if [ ! -s "$1" ]; then
        frame_text="no perf log (app exit $2$(timeout_note "$2"))"
        frame_reason="$frame_text"
        return
    fi
    valid="$(perf_val "$1" run_valid)"
    verdict="$(perf_val "$1" frame_verdict)"
    session="$(perf_val "$1" session)"
    frame_text="p99 $(perf_val "$1" p99_ms) ms, max $(perf_val "$1" max_ms) ms, session ${session:-missing}, window $(perf_val "$1" window), textures $(perf_val "$1" live_textures_at_end), run_valid ${valid:-missing}, verdict ${verdict:-missing}"
    if [ "$valid" != yes ]; then
        frame_reason="$frame_reason; run is not valid (run_valid ${valid:-missing})"
    fi
    if [ "$session" != wayland ]; then
        frame_reason="$frame_reason; session is ${session:-missing}, it must be wayland"
    fi
    if [ "$verdict" != PASS ]; then
        frame_reason="$frame_reason; frame_verdict ${verdict:-missing}"
    fi
    if [ "$2" -eq 124 ]; then
        frame_reason="$frame_reason$(timeout_note "$2")"
    fi
    if [ -z "$frame_reason" ]; then
        frame_pass=1
    else
        frame_reason="${frame_reason#; }"
    fi
}

# Drop the caches again and say so. Set cache_cold to 1 or 0.
cold_step() {
    echo "COLD STEP: sudo will ask for your password"
    sync
    if drop_cache; then
        cache_cold=1
        row INFO cache COLD
    else
        cache_cold=0
        row INFO cache PARTLY COLD
    fi
}

# Print the failed rows for a corpus that is not there.
no_corpus() {
    row FAIL scan_cold "no corpus"
    row FAIL cold_start "no corpus"
    row FAIL peak_rss "no corpus"
    row FAIL p99_wayland "no corpus"
}

check_only=0
dir=/var/tmp/tarragon-10k
dir_set=0
while [ $# -gt 0 ]; do
    case "$1" in
        --check-only) check_only=1 ;;
        -*)
            echo "usage: sh $0 [--check-only] [DIR]" >&2
            exit 2
            ;;
        *)
            if [ "$dir_set" -eq 1 ]; then
                echo "usage: sh $0 [--check-only] [DIR]" >&2
                exit 2
            fi
            dir="$1"
            dir_set=1
            ;;
    esac
    shift
done

base="${TARRAGON_SMOKE_BASE:-$(cd "$(dirname "$0")" && pwd)}"
app="${TARRAGON_SMOKE_APP:-$base/bin/tarragon}"
app_timeout="${TARRAGON_SMOKE_APP_TIMEOUT:-300}"
outdir="$(pwd)"

# Run again with the output copied to smoke-result.txt. The copy keeps the exit
# code of the inner run. The marker is not a TARRAGON_SMOKE_ name, so it is not
# listed as an override.
if [ "$check_only" -eq 0 ] && [ -z "${SMOKE_INNER_RUN:-}" ]; then
    rc_file="$(mktemp)"
    # Remove the temp file on exit and on Ctrl-C. The signal also reaches the
    # inner run, which cleans its own work folder.
    trap 'rm -f "$rc_file"' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    {
        SMOKE_INNER_RUN=1 sh "$0" "$dir" 2>&1
        echo $? >"$rc_file"
    } | tee "$outdir/smoke-result.txt"
    rc="$(cat "$rc_file")"
    exit "$rc"
fi

work="$(mktemp -d)"
# Remove only the folder made above, on exit and on Ctrl-C. A shell can skip the
# exit trap on a signal, so the signals call exit.
trap 'rm -rf "$work"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Memory: for the record only. No guard and no cap.
row INFO memory
for name in $(env | sed -n 's/^\(TARRAGON_SMOKE_[A-Za-z0-9_]*\)=.*/\1/p' | sort); do
    row INFO override "$name"
done
free -g

# Pin.
pin="$(pin_list)"
cores=0
if [ -n "$pin" ]; then
    cores="$(echo "$pin" | awk -F, '{ print NF }')"
fi
if [ "$cores" -lt 4 ]; then
    row FAIL pin "need 4 physical cores, found $cores"
    finish
fi
row INFO pin "cpus $pin ($(core_type "$pin"))"

if [ "$check_only" -eq 1 ]; then
    finish
fi

# Corpus: 10 percent PSD copies. A folder that has files must hold exactly
# the expected number of files from an earlier run, and then it is reused.
# Any other folder with files is left alone and fails the row.
want="${TARRAGON_SMOKE_FILES:-10000}"
if [ -d "$dir" ] && [ -n "$(ls -A "$dir")" ]; then
    have="$(find "$dir" -type f | wc -l | tr -d ' ')"
    if [ "$have" -eq "$want" ]; then
        row INFO corpus "reused $have files"
    else
        row FAIL corpus "$dir has $have files, expected $want; remove the folder or give another one"
        no_corpus
        finish
    fi
else
    set --
    if [ -n "${TARRAGON_SMOKE_FILES:-}" ]; then
        set -- --files "$TARRAGON_SMOKE_FILES"
    fi
    if "$base/bin/corpus_gen" --out "$dir" "$@" \
        --psd-file "$base/fixtures/rgb_8bit.psd" --psd-percent 10 >/dev/null; then
        row PASS corpus "$dir"
    else
        row FAIL corpus "corpus_gen failed for $dir"
        no_corpus
        finish
    fi
fi

# Marked cold step. A failed drop leaves the cache partly cold, and then the
# scan row cannot pass.
cold_step

# Scan under the pin, straight after the drop.
out="$work/scan.out"
taskset -c "$pin" "$base/bin/scan_bench" --folder "$dir" >"$out" 2>&1
bench_rc=$?
sed 's/^/  /' "$out"
secs="$(awk '/^total elapsed:/ { print $3 }' "$out")"
if [ -z "$secs" ]; then
    row FAIL scan_cold "bench gave no time (exit $bench_rc)"
elif [ "$cache_cold" -eq 0 ]; then
    row FAIL scan_cold "$secs s, cache PARTLY COLD"
elif [ "$bench_rc" -eq 0 ] && grep -q '^result: *PASS' "$out"; then
    row PASS scan_cold "$secs s"
else
    row FAIL scan_cold "$secs s, bench said FAIL (exit $bench_rc)"
fi

# Cold start: the caches drop again, so the scan does not warm this run. The
# app writes the time from its spawn to the first frame with the grid shown.
cold_step
cold_log="$outdir/perf-cold_start.txt"
run_app "$cold_log" load wayland
app_rc=$?
if [ ! -s "$cold_log" ]; then
    reason="no perf log (app exit $app_rc$(timeout_note "$app_rc"))"
    row FAIL cold_start "$reason"
    row FAIL peak_rss "$reason"
else
    cold_ms="$(perf_val "$cold_log" cold_start_ms)"
    basis="$(perf_val "$cold_log" cold_start_basis)"
    case "$cold_ms" in
        '' | *[!0-9]*) row FAIL cold_start "no cold_start_ms in the perf log$(timeout_note "$app_rc")" ;;
        *)
            if [ "$cache_cold" -eq 0 ]; then
                row FAIL cold_start "$cold_ms ms, cache PARTLY COLD"
            elif [ "$cold_ms" -lt 3000 ]; then
                row PASS cold_start "$cold_ms ms (basis ${basis:-unknown})"
            else
                row FAIL cold_start "$cold_ms ms, the bar is under 3000 ms"
            fi
            ;;
    esac
    rss="$(perf_val "$cold_log" peak_rss_kib)"
    case "$rss" in
        '' | *[!0-9]*) row FAIL peak_rss "no peak_rss_kib number in the perf log (${rss:-missing})$(timeout_note "$app_rc")" ;;
        *)
            if [ "$rss" -lt 2097152 ]; then
                row PASS peak_rss "$rss KiB"
            else
                row FAIL peak_rss "$rss KiB, the bar is under 2097152 KiB"
            fi
            ;;
    esac
fi

# The PSD memory bench: numbers for the record, never a bar.
if [ -x "$base/bin/psd_rss_bench" ]; then
    taskset -c "$pin" timeout --foreground "$app_timeout" "$base/bin/psd_rss_bench" --source-dir "$dir" >"$work/rss.out" 2>&1
    bench_rc=$?
    row INFO psd_rss_bench "exit $bench_rc$(timeout_note "$bench_rc")"
    sed 's/^/  /' "$work/rss.out"
else
    row INFO psd_rss_bench "not in the bundle"
fi

# Frame run under Wayland: the bar.
echo "FRAME RUN: keep the window visible and do not touch the mouse for 40 s"
wayland_log="$outdir/perf-p99_wayland.txt"
run_app "$wayland_log" frame wayland
frame_check "$wayland_log" "$?"
if [ "$frame_pass" -eq 1 ]; then
    row PASS p99_wayland "$frame_text"
else
    row FAIL p99_wayland "$frame_reason ($frame_text)"
fi

# The same run without Wayland, for comparison only. It never fails the script.
if [ -z "${DISPLAY:-}" ]; then
    row INFO p99_xwayland "skipped, DISPLAY is not set"
else
    echo "FRAME RUN: keep the window visible and do not touch the mouse for 40 s"
    xwayland_log="$outdir/perf-p99_xwayland.txt"
    run_app "$xwayland_log" frame xwayland
    frame_check "$xwayland_log" "$?"
    row INFO p99_xwayland "$frame_text"
fi

finish
