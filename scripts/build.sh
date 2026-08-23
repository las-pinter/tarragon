#!/bin/bash
set -e

# Build script for Tarragon Viewer (Linux/macOS)
# Syncs dependencies with uv and runs the Nuitka build.
#
# Usage:
#   ./scripts/build.sh              # Build release onefile binary

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Detect if we're in a VirtualBox shared folder (symlinks don't work)
REAL_PATH="$(pwd -P)"
if [[ "$REAL_PATH" == /media/sf_* ]] || [[ "$REAL_PATH" == */VirtualBox* ]]; then
    echo "==> Detected VirtualBox shared folder - using external venv location..."
    export UV_PROJECT_ENVIRONMENT="$HOME/.tarragon-build-venv"
fi

# Install dependencies (creates .venv automatically from uv.lock)
echo "==> Installing dependencies..."
uv sync --extra build

# Check for ccache (dramatically speeds up repeat builds)
if command -v ccache &>/dev/null; then
    echo "==> ccache detected - repeat builds will be fast"
else
    echo "==> ccache not found - install with: sudo apt-get install ccache"
    echo "    This will dramatically speed up repeat builds"
fi

# Run build
echo "==> Building..."
uv run python scripts/package_nuitka.py --platform linux

echo "==> Build complete! Check dist/ directory for output."
