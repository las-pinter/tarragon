#!/bin/sh
# Smoke script for the user's machine. It only measures. It prints one line
# per row: PASS name, FAIL name, or INFO name, then a summary line. The exit
# code is 0 only when no row is FAIL.
#
# Usage: sh smoke.sh [--check-only] [DIR]
#   --check-only  print the memory line and the CPU pin, then stop. No cache
#                 drop, no corpus, no scan.
#   DIR           corpus folder (default /var/tmp/tarragon-10k). It must be
#                 absent or empty, or hold exactly the expected number of
#                 files from an earlier run (then it is reused). The script
#                 never deletes a folder.
#
# Layout of the base folder (the folder that holds this script, or
# $TARRAGON_SMOKE_BASE): bin/corpus_gen, bin/scan_bench, and the PSD file
# fixtures/rgb_8bit.psd.
#
# Needs: lscpu, taskset, sudo, free.
#
# Test-only overrides (set by the tests, not by the user):
#   TARRAGON_SMOKE_LSCPU_CORES  file that replaces `lscpu -p=CPU,CORE`
#   TARRAGON_SMOKE_LSCPU_FREQ   file that replaces `lscpu -e=CPU,CORE,MAXMHZ`
#   TARRAGON_SMOKE_DROP_CMD     shell command that replaces the root drop
#   TARRAGON_SMOKE_FILES        corpus size (default: the corpus_gen default)
set -u

pass_count=0
fail_count=0
info_count=0

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
}

# Print the summary line and leave with the right exit code.
finish() {
    echo "SUMMARY pass=$pass_count fail=$fail_count info=$info_count"
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
        row FAIL scan_cold "no corpus"
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
        row FAIL scan_cold "no corpus"
        finish
    fi
fi

# Marked cold step. A failed drop leaves the cache partly cold, and then the
# scan row cannot pass.
echo "COLD STEP: sudo will ask for your password"
sync
if drop_cache; then
    cache_cold=1
    row INFO cache COLD
else
    cache_cold=0
    row INFO cache PARTLY COLD
fi

# Scan under the pin, straight after the drop.
out="$(mktemp)"
trap 'rm -f "$out"' EXIT
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

finish
