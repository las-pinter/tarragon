# Tarragon Architecture

## Overview

Tarragon is a read-only image browser and cataloging application for local image collections. It runs as a **single Python process** compiled into a native binary via [Nuitka](https://nuitka.net/). The application uses PySide6 (Qt 6) for the GUI, Pillow for plain image rendering, and psd-tools for PSD/PSB compositing.

## Module Structure

The application follows a layered architecture with clear separation between UI, services, and data access.

```
main.py
  └── main_window.py
        ├── widgets/
        │     ├── sidebar.py            — Library panel (folder navigation, favorites)
        │     ├── thumbnail_grid.py     — Gallery panel (thumbnail display, selection)
        │     ├── preview_panel.py      — Preview panel (single/multi-image preview)
        │     ├── tag_panel.py          — Tags panel (tag management, filtering)
        │     └── color_filter_bar.py   — Color filter swatches for gallery filtering
        ├── services/
        │     ├── thumbnail_service.py  — Async thumbnail generation orchestration
        │     ├── query_service.py      — SQL filter composition for gallery queries
        │     ├── tag_service.py        — High-level tag CRUD with Qt signals
        │     └── settings_service.py   — Typed settings accessors with validation
        ├── models/
        │     ├── thumbnail_model.py    — Data model backing the thumbnail grid
        │     └── filter_state.py       — Filter state management
        ├── db.py                       — SQLite database schema and CRUD
        ├── migrations.py               — Schema migration framework
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
| `grid_dock` | Gallery | `ThumbnailGrid` + search + color filter | Top |
| `preview_dock` | Preview | `PreviewPanel` | Bottom |
| `tags_dock` | Tags | `TagPanel` | Right |

The gallery dock contains a vertical stack: search box (`QLineEdit`), color filter bar (`ColorFilterBar`), and the thumbnail grid (`ThumbnailGrid`).

## Service Layer

Services sit between the UI widgets and the data layer, providing business logic and Qt signal integration.

### ThumbnailService

`services/thumbnail_service.py` — Coordinates thumbnail generation, caching, and UI signal emission.

- Inherits `QObject` to emit Qt signals (`thumbnailReady`, `errorOccurred`).
- Uses a `QThreadPool` for plain image renders (worker threads via `_RenderTask`).
- Delegates PSD/PSB compositing to the module-level `ProcessPoolExecutor` (via `_RenderPSDTask`).
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

## Data Layer

### Database (`db.py`)

SQLite-backed repository using WAL journal mode for concurrent read/write access. All operations are thread-safe via a `threading.Lock`. See [database.md](database.md) for full schema documentation.

### Migrations (`migrations.py`)

`MigrationRunner` orchestrates schema migrations. Currently bootstraps to version 1 on first run. The framework is designed to accept migration functions in future versions.

### Scanner (`scanner.py`)

Walks a directory tree, filters files by supported extensions (`.jpg`, `.jpeg`, `.png`, `.webp`, `.tiff`, `.tif`, `.psd`, `.psb`), and returns a sorted list of `FileInfo` dataclass instances containing path, mtime, size, and extension.

## Rendering Pipeline

### Plain Images (`thumbnail.py` — `render_plain_image`)

1. Open with Pillow (supports JPEG, PNG, WebP, TIFF).
2. Apply EXIF rotation via `ImageOps.exif_transpose`.
3. Convert non-RGB/RGBA modes to RGBA.
4. Resize to master long edge (2048px) using Lanczos resampling.
5. Save to cache as PNG (default) or JPEG.

### PSD/PSB (`thumbnail.py` — `render_psd_image`)

1. Dispatch compositing to a `ProcessPoolExecutor` worker subprocess.
2. In the worker: open with `psd-tools`, check canvas size against threshold.
3. Small canvases (below threshold): direct `psd.composite(force=True)`.
4. Large canvases (above threshold): tiled 2x2 composite with per-tile error isolation.
5. Resize result to master long edge, return as PNG bytes.
6. Main process deserializes PNG bytes back to a PIL Image.
7. 2-minute timeout on the future; returns `None` on failure.

See [rendering-pipeline.md](rendering-pipeline.md) for full details.

## Threading Model

Tarragon uses three concurrency mechanisms, each with a specific role:

| Mechanism | Purpose | Thread Safety |
|-----------|---------|---------------|
| **Main thread** | Qt UI event loop, user interaction | Single-threaded |
| **QThreadPool** | Plain image renders (`_RenderTask`), PSD dispatch wrappers (`_RenderPSDTask`) | Worker threads; results marshalled back via signals |
| **ProcessPoolExecutor** | PSD/PSB compositing (`_composite_psd_in_process`) | Isolated subprocesses; communicate via pickled arguments and PNG byte results |

The `ProcessPoolExecutor` is a shared singleton (`_shared_executor`) with double-checked locking. Worker count is adaptive based on available RAM (default 3, clamped to [1, 8]) or user-configured via `max_psd_workers` setting.

## Cache Strategy

### Cache Key

The cache filename is derived from a **SHA-1 hash of the source file's absolute path**:

```python
sha = hashlib.sha1(str(file_abs_path.resolve()).encode()).hexdigest()
```

This makes the cache deterministic and unique per file path.

### Cache Location

```
<platformdirs.user_data_dir("tarragon")>/cache/previews/<sha1>.png
```

Platform-specific paths:
- Linux: `~/.local/share/tarragon/cache/previews/`
- macOS: `~/Library/Application Support/tarragon/cache/previews/`
- Windows: `%APPDATA%\tarragon\cache\previews\`

### Master Resolution

All cached images are resized to a maximum long edge of **2048 pixels** (`MASTER_LONG_EDGE`), preserving aspect ratio. This provides sufficient resolution for preview and color analysis while keeping memory usage manageable.

### Cache Invalidation

Cache entries are validated by comparing two fields stored in the `thumbnails` database table:

- **mtime**: File modification time (integer).
- **size**: File size in bytes.

If either value differs from the current file's stat, the cache entry is considered stale and the image is re-rendered. Corrupt cache files (failed `Image.open`) also trigger re-rendering.

### Cache Format

Configurable via the `cache_format` setting:
- **PNG** (default): Lossless, supports RGBA natively.
- **JPEG**: Smaller file sizes; RGBA images are flattened onto a white background before saving. Quality set to 90.
