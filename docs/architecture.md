# Tarragon Architecture

## Overview

Tarragon is a read-only image browser and cataloging application for local image collections. It runs as a **single Python process** compiled into a native binary via [Nuitka](https://nuitka.net/). The application uses PySide6 (Qt 6) for the GUI, Pillow for plain image rendering, and psd-tools for PSD/PSB compositing.

## Module Structure

The application follows a layered architecture with clear separation between UI, services, and data access.

```
main.py
  └── main_window.py
        ├── widgets/
        │     ├── sidebar.py            — Library panel (folder tree + favorites)
        │     ├── thumbnail_grid.py     — Gallery grid (icon-mode QListView, custom delegate)
        │     ├── preview_panel.py      — Preview panel (single/multi-image preview + tag management)
        │     ├── filter_bar.py         — Combined filter row (color + tag + folder filters)
        │     ├── color_filter_bar.py   — Color bucket swatches for gallery filtering
        │     ├── tag_filter_bar.py     — Inline tag filter (Add Tag+ button + removable chips)
        │     ├── gallery_tabs.py       — Scope tabs (Folder / All Images)
        │     ├── gallery_info_bar.py   — Folder name, file count, active filter pill
        │     ├── log_panel.py          — Dockable log viewer with color-coded severity
        │     ├── settings_dialog.py    — Modal preferences dialog for all settings
        │     └── flow_layout.py        — Wrapping layout (items flow left-to-right, wrap on overflow)
        ├── theme/
        │     ├── tokens.json           — Design token definitions (colors, spacing, typography)
        │     ├── tokens.py             — Token loader (reads tokens.json from package resources)
        │     ├── colors.py             — Typed QColor constants derived from tokens.json
        │     ├── color_buckets.py      — Color-bucket definitions (display order, hex colors, hue ranges)
        │     ├── spacing.py            — Spacing constants (XS, SM, MD, etc.) from tokens.json
        │     ├── typography.py         — Typography constants and QFont helpers from tokens.json
        │     ├── file_type_badge.py    — File extension badge colors for thumbnail grid
        │     ├── qss_generator.py      — Builds application QSS stylesheet from design tokens
        │     ├── loader.py             — Theme loader (reads QSS + tokens, generates final QSS)
        │     └── icons/
        │           └── search.svg      — Search icon for the search box
        ├── services/
        │     ├── thumbnail_service.py  — Async thumbnail generation orchestration
        │     ├── query_service.py      — SQL filter composition for gallery queries
        │     ├── tag_service.py        — High-level tag CRUD with Qt signals
        │     └── settings_service.py   — Typed settings accessors with validation
        ├── models/
        │     ├── thumbnail_model.py    — Data model backing the thumbnail grid
        │     └── filter_state.py       — Filter state management
        ├── db/                          — SQLite database schema and CRUD (mixin package, see below)
        ├── migrations.py                — Schema migration framework
        ├── scanner.py                  — Folder scanning and file discovery
        ├── thumbnail.py                — Image rendering pipeline (plain + PSD)
        ├── color_tagger.py             — Dominant color extraction algorithm
        ├── settings.py                 — Typed key-value settings store
        ├── editors.py                  — External editor launching
        └── app_paths.py                — Platform-aware directory resolution
```

### Entry Point

`main.py` contains the `main()` function which:

1. Calls `ensure_dirs()` to create data/cache directories.
2. Creates a `QApplication` with the Fusion style and a dark palette.
3. Instantiates `MainWindow`, which auto-creates `Database` and `Settings` if not provided.
4. Calls `setup_widgets()` to wire all dock panels and content widgets.
5. Enters the Qt event loop via `app.exec()`.

### Main Window

`main_window.py` defines `MainWindow(QMainWindow)` with four dockable panels:

| Dock | Title | Widget | Position |
|------|-------|--------|----------|
| `sidebar_dock` | Library | `SidebarWidget` | Left |
| `grid_dock` | Gallery | Gallery container (see below) | Central |
| `preview_dock` | Preview | `PreviewPanel` | Right |
| `log_dock` | Log | `LogPanel` | Bottom (hidden by default) |

The gallery dock contains a vertical stack:

1. `GalleryTabs` — scope tabs (Folder / All Images)
2. `QLineEdit` — filename search box with debounce
3. `GalleryInfoBar` — folder name, file count, active filter pill
4. `FilterBar` — combined color + tag + folder filter row
5. `ThumbnailGrid` — thumbnail gallery (stretches to fill remaining space)

## Widget Layer

### SidebarWidget (`sidebar.py`)

Folder tree navigation using `QTreeView` backed by `QFileSystemModel`, plus a favorites list backed by a custom `QAbstractListModel`. Emits `folder_navigated` and `favorite_clicked` signals.

### ThumbnailGrid (`thumbnail_grid.py`)

Icon-mode `QListView` with a custom `ThumbnailDelegate` for cell rendering. Supports single and multi-selection, double-click to launch editor, right-click context menu (regenerate thumbnail), and hover tooltips. Emits `selection_changed`, `file_double_clicked`, and `regenerate_requested` signals.

### PreviewPanel (`preview_panel.py`)

Displays a single image preview with metadata (dimensions, file size, path) or a mosaic of multiple selected images. Includes inline tag management: shows tags for the current selection with checkboxes for toggling, supports tri-state display for multi-selection. Emits `tags_changed` when tags are modified.

### FilterBar (`filter_bar.py`)

Composite filter widget hosting three sub-components in a single wrapping row (via `FlowLayout`):

- **ColorFilterBar** — clickable color bucket swatches
- **TagFilterBar** — "Add Tag+" button with removable filter chips
- **Folder chips** — "Add Folder+" button with removable chips (visible only in "All Images" global scope)

Forwards child signals as `color_filter_changed`, `tag_filter_changed`, and `folder_filter_changed`.

### GalleryTabs (`gallery_tabs.py`)

Tab widget for switching between folder-scoped and global image view. Emits `scope_changed(bool)` where `True` = global (All Images).

### GalleryInfoBar (`gallery_info_bar.py`)

Horizontal bar showing `"{folder_name} · {file_count} files"` on the left and an active filter count pill on the right (hidden when no filters are active).

### LogPanel (`log_panel.py`)

Dockable log viewer with color-coded severity levels (DEBUG through CRITICAL). Includes a Clear button and Debug toggle checkbox. `QtLogHandler` bridges Python's `logging` module to the panel via Qt signals for thread-safe delivery.

### SettingsDialog (`settings_dialog.py`)

Modal preferences dialog organized into sections: Performance (PSD workers, multi-preview cap, large canvas threshold), Grid & Layout (tile grid size), Color Tagging (enable, palette size, min share, neutral threshold), Cache (directory, format), and Debug (logging toggle). Settings are persisted via `SettingsService` on OK.

### FlowLayout (`flow_layout.py`)

Custom `QLayout` that arranges items left-to-right, wrapping to the next line when width is exceeded. Items are vertically centered within each line. Used by `FilterBar` for responsive filter layout.

## Service Layer

Services sit between the UI widgets and the data layer, providing business logic and Qt signal integration.

### ThumbnailService

`services/thumbnail_service.py` — Coordinates thumbnail generation, caching, and UI signal emission.

- Inherits `QObject` to emit Qt signals (`thumbnailReady`, `errorOccurred`).
- Uses a `QThreadPool` for all render tasks (`_RenderAllTask`), covering plain image renders, PSD dispatch wrappers, and CLIP extraction.
- Delegates PSD/PSB compositing to the module-level `ProcessPoolExecutor` (called from within `_RenderAllTask`).
- On render completion, persists thumbnail metadata to the database and optionally extracts color tags.
- Checks cache validity (mtime + size) before dispatching renders.

### QueryService

`services/query_service.py` — Composes SQL filter queries for the gallery.

- Combines folder scope, filename text search, tag ID filters (AND semantics), and color tag filters (OR semantics) into a single parameterized SQL statement.
- Returns `list[Path]` ordered alphabetically.
- Escapes LIKE special characters (`%`, `_`) for safe filename matching.

### TagService

`services/tag_service.py` — High-level tag CRUD with Qt signal integration.

- Wraps `Database` tag operations into a service interface.
- Emits `tagsChanged` signal on any tag mutation.
- Supports tri-state resolution for multi-selection tag checkboxes.

### SettingsService

`services/settings_service.py` — Typed accessors and validation for settings.

- Enforces type coercion and range clamping for numeric values.
- Validates string formats (e.g., tile grid size must match `NxN`).
- Prevents invalid state from reaching the persistence layer.

## Theme System

The theme is driven by design tokens stored in `tokens.json` and exposed through typed Python modules:

- **`tokens.json`** — Single source of truth for colors, spacing, and typography values.
- **`tokens.py`** — Loads and parses `tokens.json` from package resources.
- **`colors.py`** — Exports each color token as a `QColor` constant (e.g., `BG_PRIMARY`, `CORAL_STRONG`).
- **`color_buckets.py`** — Defines the 10 color buckets (display order, hex colors, hue ranges) used by the color tagger and filter UI.
- **`spacing.py`** — Exports spacing tokens as integer constants (`XS`, `SM`, `MD`, `LG`, `XL`).
- **`typography.py`** — Exports font size constants and `QFont` factory functions (`body_font()`, `heading_font()`).
- **`file_type_badge.py`** — Maps file extensions to `(background, text)` QColor pairs for thumbnail badges.
- **`qss_generator.py`** — Generates the full application QSS stylesheet from tokens.
- **`loader.py`** — Entry point for theme initialization; calls `load_and_generate_qss()` to produce the final stylesheet.
- **`icons/`** — SVG icon assets (currently `search.svg`).

## Data Layer

### Database (`db/`)

SQLite-backed repository using WAL journal mode for concurrent read/write access. All operations are thread-safe via a `threading.Lock`. Implemented as a mixin package rather than a single module:

- `_base.py` — Connection management, thread-safe SQL helpers (`_execute`, `_executemany`, `_executescript`, `_commit`), schema initialization, and the `normalize_path()` helper.
- `_thumbnails.py`, `_tags.py`, `_favorites.py`, `_folder_cache.py`, `_editors.py` — CRUD operations for each table, each as a mixin class.
- `database.py` — The final `Database` class, composed from `_Base` plus all the CRUD mixins.
- `__init__.py` — Public surface: exports `Database` and `normalize_path`; everything else is an internal implementation detail.

See [database.md](database.md) for full schema documentation.

### Migrations (`migrations.py`)

`MigrationRunner` orchestrates schema migrations. Currently bootstraps to version 1 on first run. The framework is designed to accept migration functions in future versions.

### Scanner (`scanner.py`)

Walks a directory tree, filters files by supported extensions (`.jpg`, `.jpeg`, `.png`, `.webp`, `.tiff`, `.tif`, `.psd`, `.psb`, `.clip`), and returns a sorted list of `FileInfo` dataclass instances containing path, mtime, size, and extension.

## Rendering Pipeline

### Plain Images (`thumbnail.py` — `render_plain_image`)

1. Open with Pillow (supports JPEG, PNG, WebP, TIFF).
2. Apply EXIF rotation via `ImageOps.exif_transpose`.
3. Convert non-RGB/RGBA modes to RGBA.
4. Resize to master long edge (2048px) using Lanczos resampling.
5. Return the PIL Image, or `None` on failure. (Cache persistence is handled by `ThumbnailService._render_all_resolutions()`.)

### PSD/PSB (`thumbnail.py` — `render_psd_image`)

1. Dispatch compositing to a `ProcessPoolExecutor` worker subprocess.
2. In the worker: open with `psd-tools`, check canvas size against threshold.
3. Small canvases (below threshold): direct `psd.composite(force=True)`.
4. Large canvases (above threshold): tiled 2x2 composite with per-tile error isolation.
5. Resize result to master long edge, return as PNG bytes.
6. Main process deserializes PNG bytes back to a PIL Image.
7. Return the deserialized PIL Image, or `None` on cancellation or failure.
   Cache persistence and color tag extraction are handled by `ThumbnailService._render_all_resolutions()`.

### CLIP (`thumbnail.py` — `render_clip_image`)

1. Read the .clip file and locate the embedded SQLite3 database by scanning for the SQLite header.
2. Write the SQLite portion to a temporary file and query the `CanvasPreview` table for the `ImageData` blob.
3. Extract the PNG from the blob by locating the PNG signature and IEND chunk boundary.
4. Decode the PNG via Pillow, convert to RGB/RGBA, and optionally resize.
5. Clean up the temporary SQLite file. Returns `None` on any failure.

See [rendering-pipeline.md](rendering-pipeline.md) for full details.

## Threading Model

Tarragon uses three concurrency mechanisms, each with a specific role:

| Mechanism | Purpose | Thread Safety |
|-----------|---------|---------------|
| **Main thread** | Qt UI event loop, user interaction | Single-threaded |
| **QThreadPool** | All render tasks (`_RenderAllTask`) — plain, PSD dispatch, CLIP | Worker threads; results marshalled back via signals |
| **ProcessPoolExecutor** | PSD/PSB compositing (`_composite_psd_in_process`) | Isolated subprocesses; communicate via pickled arguments and PNG byte results |

The `ProcessPoolExecutor` is a shared singleton (`_shared_executor`) with double-checked locking. Worker count is adaptive based on available RAM (default 3, clamped to [1, 8]) or user-configured via `max_psd_workers` setting.

## Cache Strategy

Cache entries are organized by per-folder UUID under `<user_data_dir>/cache/{resolution}/{folder_name}_{uuid}/`, with three resolution tiers (256, 1024, full). Cache validity is checked by comparing `mtime` and `size` against the database record. Cache format (PNG or JPEG) is configurable via settings. See [Rendering Pipeline](rendering-pipeline.md) for full cache details.
