#!/bin/bash
set -e

# Build script for Tarragon Viewer (Linux/macOS)
# Creates a virtual environment, installs dependencies, and runs the Nuitka build.
#
# Usage:
#   ./scripts/build.sh              # Build release onefile binary
#   ./scripts/build.sh --dev        # Build dev mode (fast iteration)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Detect if we're in a VirtualBox shared folder (symlinks don't work)
REAL_PATH="$(pwd -P)"
if [[ "$REAL_PATH" == /media/sf_* ]] || [[ "$REAL_PATH" == */VirtualBox* ]]; then
    echo "==> Detected VirtualBox shared folder — using external venv location..."
    VENV_DIR="$HOME/.tarragon-build-venv"
else
    VENV_DIR=".venv"
fi

# Create venv if needed
if [ ! -f "$VENV_DIR/bin/activate" ]; then
    echo "==> Creating virtual environment at $VENV_DIR..."
    rm -rf "$VENV_DIR" # Clean up any partial venv
    python3 -m venv --copies "$VENV_DIR" || {
        echo "ERROR: Failed to create virtual environment"
        exit 1
    }
fi

# Activate venv
echo "==> Activating virtual environment..."
source "$VENV_DIR/bin/activate"

# Upgrade pip
pip install --upgrade pip --quiet

# Install dependencies
echo "==> Installing runtime dependencies..."
pip install -r requirements.txt --quiet

echo "==> Installing build dependencies..."
pip install -r requirements-build.txt --quiet

# Check for ccache (dramatically speeds up repeat builds)
if command -v ccache &>/dev/null; then
    echo "==> ccache detected — repeat builds will be fast"
else
    echo "==> ccache not found — install with: sudo apt-get install ccache"
    echo "    This will dramatically speed up repeat builds"
fi

# Determine build mode
BUILD_MODE="release"
if [ "$1" = "--dev" ]; then BUILD_MODE="dev"; fi

# Run build
echo "==> Building RELEASE mode (full build)..."
python scripts/package_nuitka.py --release

echo "==> Release build complete! Check dist/ directory for output."
