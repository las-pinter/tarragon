# Tarragon Rendering Pipeline

## Overview

Tarragon renders image thumbnails through three distinct pipelines depending on the file format:

1. **Plain images** (JPEG, PNG, WebP, TIFF) — rendered directly via Pillow in a QThreadPool worker thread.
2. **PSD/PSB files** — composited via psd-tools in an isolated subprocess (ProcessPoolExecutor), then resized and cached.
3. **CLIP files** (Clip Studio Paint) — thumbnail extracted from an embedded SQLite3 database, then resized and cached.

All three pipelines produce full-resolution, thumbnail (256px), and preview (1024px) images that are saved to the on-disk cache and displayed in the UI.

## Plain Image Pipeline

### Entry Point

`thumbnail.py` → `render_plain_image(file_path: Path, target_size: int | None = None) -> Image.Image | None`

### Steps

1. **Open** the file with `PIL.Image.open(file_path)`.
2. **EXIF transpose** — Apply orientation correction via `ImageOps.exif_transpose(img)`. This handles camera rotation metadata so the image displays upright.
3. **Mode conversion** — If the image mode is not `RGB` or `RGBA` (e.g., `P`, `L`, `CMYK`, `I;16`), convert to `RGBA`. This ensures a consistent pixel format for downstream processing.
4. **Resize** — If *target_size* is specified, call `img.thumbnail((target_size, target_size), Image.LANCZOS)`. This shrinks the image in-place so the longest side is at most *target_size* pixels, preserving aspect ratio. Lanczos resampling provides high-quality downscaling. If *target_size* is `None`, no resizing is performed (full original resolution).
5. **Return** the PIL Image, or `None` if any `OSError` or `ValueError` occurs (failure isolation — skip the file rather than crashing).

### Threading

Plain image renders are dispatched via `_RenderAllTask(QRunnable)` to a `QThreadPool`. The task calls `_render_all_resolutions()` in the worker thread, which:

1. Calls `render_plain_image()` to render the full-resolution image.
2. Generates cache paths via `generate_cache_paths()` for all three resolution tiers.
3. Calls `save_to_cache()` to write each rendered resolution to disk.
4. Persists metadata to the database and emits `thumbnailReady` for each resolution.

## PSD/PSB Compositing Pipeline

### Entry Point

`thumbnail.py` → `render_psd_image(file_path, large_canvas_threshold_mp, tile_grid_x, tile_grid_y, target_size: int | None = None, cancel_event: threading.Event | None = None) -> Image.Image | None`

### Architecture

PSD compositing is CPU-intensive and can consume significant memory for large files. To avoid blocking the UI or destabilizing the main process, compositing runs in an **isolated subprocess** via `ProcessPoolExecutor`.

### Process Isolation

The `ProcessPoolExecutor` is a shared singleton (`_shared_executor`) with double-checked locking:

- Worker count is adaptive: `max(1, min(available_ram // 200MB, 8))`.
- Can be overridden via the `max_psd_workers` setting (clamped to [1, 8]).
- Registered with `atexit` for clean shutdown.
- Communicates with the main process via pickled arguments (strings, floats, ints) and PNG byte results.

### Steps (in subprocess worker)

The worker function `_composite_psd_in_process()` executes:

1. **Open** the PSD/PSB file with `psd_tools.PSDImage.open(file_path)`.
2. **Calculate canvas size** in megapixels: `(width * height) / 1,000,000`.
3. **Branch on canvas size**:

   **Small canvas** (canvas_mp <= threshold, default 20MP):
   - Call `psd.composite(force=True)` for a direct full-canvas composite.

   **Large canvas** (canvas_mp > threshold):
   - Divide the canvas into a tile grid (default 2x2).
   - For each tile, compute viewport coordinates `(x1, y1, x2, y2)`.
   - Call `psd.composite(viewport=(x1, y1, x2, y2), force=True)` per tile.
   - Paste each tile onto a transparent RGBA canvas.
   - Per-tile error isolation: if a tile fails, skip it (partial render is better than crash).

4. **Resize** the composited image to *target_size* (long edge) if specified. If `None`, no resizing is performed.
5. **Serialize** to PNG bytes via `io.BytesIO` (PIL Images are not picklable across process boundaries).
6. **Return** raw PNG bytes, or `None` on any failure.

### Main Process

`render_psd_image()` in the main process:

1. Submits the work to the `ProcessPoolExecutor`.
2. **Polls** for the result in a loop, checking `cancel_event` every 500 ms. If the event is set (e.g., folder switch or shutdown), the future is cancelled and `None` is returned immediately.
3. If bytes are returned, deserializes via `Image.open(io.BytesIO(result_bytes))`.
4. Returns the PIL Image, or `None` on cancellation or failure.

### Threading

PSD renders are dispatched via `_RenderAllTask(QRunnable)` to the `QThreadPool`. This task wraps the `render_psd_image()` call (which internally uses `ProcessPoolExecutor`) in a worker thread, keeping the main UI thread free.

## CLIP File Pipeline

### Entry Point

`thumbnail.py` → `render_clip_image(file_path: Path, target_size: int | None = None) -> Image.Image | None`

### Overview

Clip Studio Paint `.clip` files contain an embedded SQLite3 database with a `CanvasPreview` table that stores a PNG thumbnail in its `ImageData` column. Rather than compositing layers (as with PSD), the CLIP pipeline extracts this pre-rendered PNG preview directly from the file's binary content. No external libraries are needed beyond Python's stdlib `sqlite3` module and Pillow.

### Steps

1. **Read** the entire `.clip` file into memory via `file_path.read_bytes()`.
2. **Locate the SQLite database** — scan the binary data for the SQLite header magic (`SQLite format 3\0`). If no header is found, return `None`.
3. **Extract the SQLite portion** — slice the binary data from the header offset to the end of the file.
4. **Write to a temporary file** — the SQLite data is written to a `NamedTemporaryFile` so that `sqlite3.connect()` can open it. The temp file is deleted in a `finally` block.
5. **Query the preview** — execute `SELECT ImageData FROM CanvasPreview LIMIT 1` to retrieve the PNG blob.
6. **Extract the PNG from the blob** — the `ImageData` blob may contain extra bytes before or after the PNG. The function locates the PNG signature (`\x89PNG\r\n\x1a\n`) and the `IEND` chunk marker, then slices out the exact PNG bytes (including the 4-byte CRC after `IEND`).
7. **Decode** the PNG bytes via `Image.open(io.BytesIO(png_bytes))` and call `img.load()` to force immediate decoding (catches corrupt data early).
8. **Mode conversion** — If the image mode is not `RGB` or `RGBA`, convert to `RGBA`.
9. **Resize** — If *target_size* is specified, call `img.thumbnail((target_size, target_size), Image.LANCZOS)`. If `None`, no resizing is performed.
10. **Return** the PIL Image, or `None` on any failure (file not found, no SQLite header, missing table, corrupt data, decode failure, …).

### Error Handling

Every step is wrapped in `try`/`except` blocks. On any failure, a warning is logged and `None` is returned. This ensures that a corrupt or malformed `.clip` file never crashes the pipeline — it is simply skipped, and the caller emits an error signal.

### Dependencies

- **Python stdlib `sqlite3`** — for querying the embedded database.
- **Pillow** — for PNG decoding and image manipulation.
- No third-party CLIP parsing library is required.

### Threading

CLIP renders are dispatched via `_RenderAllTask(QRunnable)` to the `QThreadPool`, using the same unified task class as plain images and PSD files. The `render_clip_image()` call runs in the worker thread.

## Render Dispatch Logic

### Format Detection

The `ThumbnailService._render_all_resolutions()` method selects the appropriate render function based on the file extension from `FileInfo.extension`:

| Extension(s) | Render Function | Pipeline |
|---|---|---|
| `.psd`, `.psb` | `render_psd_image()` | PSD/PSB compositing via subprocess |
| `.clip` | `render_clip_image()` | SQLite extraction from embedded database |
| `.jpg`, `.jpeg`, `.png`, `.webp`, `.tiff`, `.tif` | `render_plain_image()` | Direct Pillow decode |

### Resolution Tiers

After the full-resolution image is rendered, two smaller sizes are derived via `derive_smaller_sizes()`:

| Tier | Constant | Long Edge | Description |
|------|----------|-----------|-------------|
| Thumbnail | `RESOLUTION_THUMBNAIL` | 256px | Grid thumbnails, list views |
| Preview | `RESOLUTION_PREVIEW` | 1024px | Detail preview, zoom level 1 |
| Full | `RESOLUTION_FULL` (`None`) | Original | Full source resolution (no resizing) |

All three tiers are saved to the cache and emitted via `thumbnailReady` signals. The `derive_smaller_sizes()` function never upscales — if the source is smaller than a target tier, a copy is included as-is so that all cache tiers are populated.

### Cancellation Support

The `_render_all_resolutions()` method checks `self._cancel_event` at multiple points:
- Before any work begins.
- After the expensive full-resolution render completes.
- Between each smaller resolution save.

When the event is set (e.g., folder switch or app shutdown), stale work is aborted early.

## Cache System

### Cache Key Generation

Cache entries are organized using a per-folder UUID and human-readable filenames:

```python
cache_uuid = generate_cache_uuid()  # 8-character hex string from UUID4
cache_paths = generate_cache_paths(source_path, cache_uuid)
# Returns: {'256': Path(...), '1024': Path(...), 'full': Path(...)}
```

The `cache_uuid` is stored in the database per-folder (via `get_or_create_folder_uuid()`), ensuring all images in the same folder share a consistent cache directory. Atomic insert prevents race conditions when two threads process images from the same folder simultaneously.

### Cache Layout

```
<user_data_dir>/cache/
├── 256/
│   └── <folder_name>_<uuid>/
│       └── <filename>.png
├── 1024/
│   └── <folder_name>_<uuid>/
│       └── <filename>.png
└── full/
    └── <folder_name>_<uuid>/
        └── <filename>.png
```

Each resolution tier has its own top-level directory. Within each tier, a per-folder subdirectory (named `<folder_name>_<uuid>`) contains the cached PNG files, preserving the original filename stem for easy identification.

Platform-specific base directories:
- Linux: `~/.local/share/tarragon/cache/`
- macOS: `~/Library/Application Support/tarragon/cache/`
- Windows: `%APPDATA%\tarragon\cache\`

Directories are created on demand by `generate_cache_paths()` via `mkdir(parents=True, exist_ok=True)`.

> **Note:** A legacy flat cache layout (`cache/previews/<sha1>.png`) was used in earlier versions. The function `_cache_file_path()` is retained for backward compatibility but is deprecated. All new code paths use `generate_cache_paths()`.

### Cache Format

Configurable via the `cache_format` setting:

| Format | Extension | Notes |
|--------|-----------|-------|
| PNG (default) | `.png` | Lossless, supports RGBA natively |
| JPEG | `.jpg` | Smaller files; RGBA flattened onto white background; quality=90 |

### Cache Invalidation

Cache validity is checked in `ThumbnailService.check_and_render()`:

1. Look up the thumbnail record in the database by path.
2. Compare stored `mtime` (as integer) and `size` against the current file's stat values.
3. If both match, check that **all three** resolution cache files exist on disk (thumbnail, preview, and full).
4. If all files exist → cache hit, load and emit each resolution directly.
5. If some resolutions are missing but others exist → derive missing tiers from the largest available cached image (see `_derive_missing_resolutions()`).
6. Otherwise → cache miss or stale, dispatch a full re-render via `_render_all_resolutions()`.

This means the cache is invalidated when:
- The source file is modified (mtime changes).
- The source file is replaced with a different-size file.
- Any of the three cache files are deleted or corrupted.
- The source file is moved/renamed (different path = different DB record).

### Cache File Deletion

`invalidate_cache_files(db, source_path)` deletes all three cached PNG files from disk and removes the database record. It uses `Path.unlink(missing_ok=True)` so missing files do not raise. If no DB record exists, the function is a no-op.

## Color Tagging Integration

After a successful render, if `color_tag_enabled` is `True`, the `_render_all_resolutions()` method extracts dominant color tags from the rendered full-resolution image and persists them to the database via `replace_auto_color_tags()`.

The color tagging algorithm runs on the in-memory PIL Image (at full resolution), so it operates on the cached master — not the original file. See [color-tagging.md](color-tagging.md) for the full algorithm description.

## Performance Characteristics

| Operation | Typical Time | Bottleneck |
|-----------|-------------|------------|
| Plain image render (cache miss) | 50–200ms | Disk I/O + decode |
| Plain image render (cache hit) | 5–20ms | Disk I/O + decode from cache |
| PSD composite (small, <20MP) | 500ms–3s | psd-tools compositing |
| PSD composite (large, tiled) | 2–10s | psd-tools per-tile compositing |
| CLIP extraction | 100–500ms | File read + SQLite query + PNG decode |
| Cache write (PNG) | 20–100ms | Disk I/O + PNG encode |
| Color tag extraction | 5–20ms | Quantize + HSV conversion |

The `ProcessPoolExecutor` allows multiple PSD files to be composited in parallel (up to `max_psd_workers`), while the `QThreadPool` manages overall concurrency for all render tasks (plain, PSD, and CLIP).

## Supported File Formats

The scanner (`scanner.py`) discovers files with the following extensions:

| Extension | Format | Render Pipeline |
|-----------|--------|-----------------|
| `.jpg`, `.jpeg` | JPEG | Plain image |
| `.png` | PNG | Plain image |
| `.webp` | WebP | Plain image |
| `.tiff`, `.tif` | TIFF | Plain image |
| `.psd` | Photoshop Document | PSD/PSB compositing |
| `.psb` | Photoshop Big | PSD/PSB compositing |
| `.clip` | Clip Studio Paint | CLIP SQLite extraction |

Extension matching is case-insensitive (`.JPG` matches `.jpg`).
