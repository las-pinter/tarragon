@echo off
setlocal

REM Build script for Tarragon Viewer (Windows)
REM Syncs dependencies with uv and runs the Nuitka build.
REM
REM Usage:
REM   scripts\build.bat              Build release onefile binary

set "SCRIPT_DIR=%~dp0"
set "PROJECT_ROOT=%SCRIPT_DIR%.."

cd /d "%PROJECT_ROOT%"

REM Install dependencies (creates .venv automatically from uv.lock)
echo ==^> Installing dependencies...
uv sync --extra build

REM Check for ccache (dramatically speeds up repeat builds)
where ccache >nul 2>&1
if %errorlevel% equ 0 (
    echo ==^> ccache detected - repeat builds will be fast
) else (
    echo ==^> ccache not found - install from https://ccache.dev/
    echo     This will dramatically speed up repeat builds
)

REM Run build
echo ==^> Building...
uv run python scripts\package_nuitka.py --platform windows

echo ==^> Build complete! Check dist\ directory for output.

endlocal
