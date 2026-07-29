# Tarragon Contributor Guide

## Prerequisites

- **Python 3.12+** — Tarragon requires Python 3.12 or later.
- **uv** (recommended) — Fast Python package and virtual environment manager. [Install uv](https://docs.astral.sh/uv/getting-started/installation/).
- **pip** — Alternative to uv for package installation.
- **Git** — For version control.

## Development Setup

### 1. Clone the Repository

```bash
git clone https://github.com/las-pinter/tarragon.git
cd tarragon
```

### 2. Create a Virtual Environment and Install Dependencies

#### Using uv (recommended)

```bash
uv venv
uv pip install -e ".[dev]"
```

#### Using pip

```bash
python -m venv .venv
source .venv/bin/activate        # Linux/macOS
# .venv\Scripts\activate         # Windows

pip install -e ".[dev]"
```

### 3. Verify Installation

```bash
uv run python -c "import tarragon; print('OK')"
# or, if using pip: python -c "import tarragon; print('OK')"
```

## Running the Application

```bash
uv run python -m tarragon
```

Or, if using pip with an activated virtual environment:

```bash
python -m tarragon
```

This launches the Tarragon GUI. On first run, it creates the data directory and database at the platform-specific location (see [Architecture](docs/architecture.md) for paths).

## Running Tests

```bash
uv run pytest
```

Run a specific test file:

```bash
uv run pytest tests/test_thumbnail_model.py
```

Run with verbose output:

```bash
uv run pytest -v
```

Run with coverage (if pytest-cov is installed):

```bash
uv run pytest --cov=tarragon
```

Test configuration is in `pyproject.toml`:

```toml
[tool.pytest.ini_options]
testpaths = ["tests"]
python_files = ["test_*.py", "*_test.py"]
python_classes = ["Test*"]
python_functions = ["test_*"]
```

## Code Style

### Linting

```bash
uv run ruff check .
```

Auto-fix lint issues:

```bash
uv run ruff check --fix .
```

### Formatting

```bash
uv run ruff format .
```

Check formatting without changes:

```bash
uv run ruff format --check .
```

### Configuration

Ruff is configured in `pyproject.toml`:

```toml
[tool.ruff]
target-version = "py312"
line-length = 120

[tool.ruff.lint]
select = ["E", "F", "I", "N", "W", "UP"]

[tool.ruff.format]
quote-style = "double"
indent-style = "space"
```

Enabled rule sets:
- `E` / `W` — pycodestyle errors and warnings
- `F` — pyflakes
- `I` — isort (import sorting)
- `N` — pep8-naming
- `UP` — pyupgrade (modernize Python syntax)

## Pre-commit Hooks

Install pre-commit hooks to automatically run checks before each commit:

```bash
pre-commit install
```

Run all hooks manually:

```bash
pre-commit run --all-files
```

## Project Structure

```
tarragon/
├── pyproject.toml              # Project metadata, dependencies, tool config
├── README.md                   # Project overview
├── LICENSE                     # MIT license
├── .editorconfig               # Editor settings
├── .pre-commit-config.yaml     # Pre-commit hook definitions
├── scripts/
│   ├── build.sh                # Linux/macOS build script
│   ├── build.bat               # Windows build script
│   ├── package_nuitka.py       # Nuitka packaging configuration
│   └── benchmark_rendering.py  # Rendering performance benchmarks
├── src/tarragon/
│   ├── __init__.py
│   ├── __main__.py             # python -m tarragon entry point
│   ├── main.py                 # Application entry point, MainWindow subclass
│   ├── main_window.py          # Base MainWindow with dock panels
│   ├── app_paths.py            # Platform-aware directory resolution
│   ├── gallery_controller.py   # Gallery filter orchestration & selection
│   ├── image_utils.py          # Image utility functions
│   ├── logging.py              # Logging configuration
│   ├── migrations.py           # Schema migration framework
│   ├── scanner.py              # Folder scanning and file discovery
│   ├── py.typed                # PEP 561 marker
│   ├── db/                     # SQLite database package (mixins)
│   │   ├── __init__.py
│   │   ├── _base.py            # Base DB mixin (schema, CRUD basics)
│   │   ├── _thumbnails.py      # Thumbnail CRUD mixin
│   │   ├── _tags.py            # Tag CRUD mixin
│   │   ├── _favorites.py       # Favorites CRUD mixin
│   │   ├── _folder_cache.py    # Folder cache mixin
│   │   ├── _editors.py         # Editor settings mixin
│   │   ├── _settings.py        # Key-value settings mixin
│   │   └── database.py         # Database class (combines all mixins)
│   ├── models/
│   │   ├── __init__.py
│   │   ├── favorites_model.py  # Favorites data model
│   │   ├── thumbnail_model.py  # Data model for thumbnail grid
│   │   └── filter_state.py     # Filter state management
│   ├── renderers/              # Image rendering pipeline
│   │   ├── __init__.py
│   │   ├── cache.py            # Thumbnail cache management
│   │   ├── clip.py             # Clipboard rendering
│   │   ├── plain.py            # Plain image rendering (JPEG, PNG, etc.)
│   │   └── psd.py              # PSD/PSB file rendering
│   ├── services/
│   │   ├── __init__.py
│   │   ├── color_tagger.py     # Color tagging service
│   │   ├── editors.py          # Editor launching service
│   │   ├── query_service.py    # SQL filter composition
│   │   ├── settings_service.py # Setting subclasses + SettingsService
│   │   ├── tag_service.py      # Tag CRUD with Qt signals
│   │   └── thumbnail_service.py # Async thumbnail orchestration
│   ├── widgets/
│   │   ├── __init__.py
│   │   ├── _chip_utils.py      # Chip widget utilities
│   │   ├── sidebar.py          # Library panel
│   │   ├── thumbnail_grid.py   # Gallery panel
│   │   ├── thumbnail_delegate.py # Thumbnail grid delegate
│   │   ├── thumbnail_animator.py # Thumbnail animations
│   │   ├── preview_panel.py    # Preview panel with tag management
│   │   ├── tag_pill.py         # Tag pill widget
│   │   ├── filter_bar.py       # Combined filter bar widget
│   │   ├── filter_bar_color.py # Color filter swatches
│   │   ├── filter_bar_filter.py # Filter bar filter component
│   │   ├── filter_bar_folder.py # Folder filter dropdown
│   │   ├── filter_bar_tag.py   # Tag filter bar
│   │   ├── gallery_info_bar.py # Gallery info display
│   │   ├── gallery_tabs.py     # Gallery tab widget
│   │   ├── log_panel.py        # Log output panel
│   │   ├── settings_dialog.py  # Settings dialog
│   │   └── flow_layout.py      # Custom flow layout
│   └── theme/
│       ├── __init__.py
│       ├── color_buckets.py    # Color bucket definitions
│       ├── colors.py           # Color utilities
│       ├── constants.py        # Theme constants
│       ├── file_type_badge.py  # File type badge rendering
│       ├── qss_generator.py    # QSS generation utilities
│       ├── typography.py       # Typography settings
│       └── icons/
│           └── search.svg
├── tests/                        # Test suite
└── docs/                         # Documentation
    ├── architecture.md
    ├── database.md
    ├── rendering-pipeline.md
    ├── color-tagging.md
    └── release.md
```

## Architecture Overview

Tarragon follows a layered architecture with a UI layer (PySide6 widgets and dock panels), a service layer (business logic and Qt signal integration), a data layer (SQLite persistence and file discovery), and a rendering layer (image processing pipelines). See [Architecture](docs/architecture.md) for full details.

## Key Dependencies

| Package | Purpose |
|---------|---------|
| `PySide6>=6.6` | Qt 6 GUI framework |
| `Pillow>=10.0` | Image processing (open, resize, convert, save) |
| `psd-tools[composite]>=1.9.7` | PSD/PSB file compositing |
| `platformdirs>=4.0` | Platform-specific directory resolution |
| `psutil>=5.9` | System memory detection for adaptive worker count |

## Building a Release

See [Release](docs/release.md) for the full release process. Quick summary:

```bash
# Linux/macOS
./scripts/build.sh

# Windows
scripts\build.bat
```

## Before Submitting a PR

1. All tests pass: `uv run pytest`
2. Linting passes: `uv run ruff check .`
3. Formatting is clean: `uv run ruff format --check .`
4. No TODO/FIXME comments in new code — implement fully or create a tracked issue.
5. Documentation updated if the change affects public behavior.
