#!/bin/sh
# Build the large PSB for CI. The output is not committed.
# This script runs in the CI workflow.
set -eu

out="${1:-${TMPDIR:-/tmp}/tarragon-large.psb}"
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
exec python3 "$here/generate.py" --large-psb "$out"
