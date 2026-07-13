@echo off
setlocal

REM Build script for Tarragon Viewer (Windows)
REM Creates a virtual environment, installs dependencies, and runs the Nuitka build.
REM
REM Usage:
REM   scripts\build.bat              Build release onefile binary

set "SCRIPT_DIR=%~dp0"
set "PROJECT_ROOT=%SCRIPT_DIR%.."

cd /d "%PROJECT_ROOT%"

REM Create venv if needed
if not exist ".venv\Scripts\activate.bat" (
    echo ==^> Creating virtual environment...
    if exist ".venv" rmdir /s /q .venv
    python -m venv .venv
    if errorlevel 1 (
        echo ERROR: Failed to create virtual environment
        exit /b 1
    )
)

REM Activate venv
echo ==^> Activating virtual environment...
call .venv\Scripts\activate.bat

REM Upgrade pip
pip install --upgrade pip --quiet

REM Install dependencies
echo ==^> Installing dependencies...
pip install -e ".[build]" --quiet

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
python scripts\package_nuitka.py

echo ==^> Build complete! Check dist\ directory for output.

endlocal
