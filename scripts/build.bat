@echo off
setlocal

REM Build script for Tarragon Viewer (Windows)
REM Creates a virtual environment, installs dependencies, and runs the Nuitka build.
REM
REM Usage:
REM   scripts\build.bat              Build onefile binary
REM   scripts\build.bat --standalone Build standalone directory

set "SCRIPT_DIR=%~dp0"
set "PROJECT_ROOT=%SCRIPT_DIR%.."

cd /d "%PROJECT_ROOT%"

REM Create venv if needed
if not exist ".venv" (
    echo ==^> Creating virtual environment...
    python -m venv .venv
)

REM Activate venv
echo ==^> Activating virtual environment...
call .venv\Scripts\activate.bat

REM Upgrade pip
pip install --upgrade pip --quiet

REM Install dependencies
echo ==^> Installing runtime dependencies...
pip install -r requirements.txt --quiet

echo ==^> Installing build dependencies...
pip install -r requirements-build.txt --quiet

REM Run build
echo ==^> Starting Nuitka build...
python scripts\package_nuitka.py %*

echo ==^> Build complete! Check dist\ directory for output.

endlocal
