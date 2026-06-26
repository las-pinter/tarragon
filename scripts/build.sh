#!/bin/bash
set -e

# Build script for Tarragon Viewer (Linux/macOS)
# Creates a virtual environment, installs dependencies, and runs the Nuitka build.
#
# Usage:
#   ./scripts/build.sh              # Build onefile binary
#   ./scripts/build.sh --standalone # Build standalone directory

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Create venv if needed
if [ ! -d ".venv" ]; then
    echo "==> Creating virtual environment..."
    python3 -m venv .venv
fi

# Activate venv
echo "==> Activating virtual environment..."
source .venv/bin/activate

# Upgrade pip
pip install --upgrade pip --quiet

# Install dependencies
echo "==> Installing runtime dependencies..."
pip install -r requirements.txt --quiet

echo "==> Installing build dependencies..."
pip install -r requirements-build.txt --quiet

# Run build
echo "==> Starting Nuitka build..."
python scripts/package_nuitka.py "$@"

echo "==> Build complete! Check dist/ directory for output."
