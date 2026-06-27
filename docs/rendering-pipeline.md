# Tarragon Rendering Pipeline

## Overview

Tarragon renders image thumbnails through two distinct pipelines depending on the file format:

1. **Plain images** (JPEG, PNG, WebP, TIFF) — rendered directly via Pillow in a QThreadPool worker thread.
2. **PSD/PSB files** — composited via psd-tools in an isolated subprocess (ProcessPoolExecutor), then resized and cached.

Both pipelines produce a master-resolution image (max 2048px long edge) that is saved to the on-disk cache and displayed in the UI.

## Plain Image Pipeline

### Entry Point

`thumbnail.py` → `render_plain_image(file_path: Path) -> Image.Image | None`

### Steps

1. **Open** the file with `PIL.Image.open(file_path)`.
2. **EXIF transpose** — Apply orientation correction via `ImageOps.exif_transpose(img)`. This handles camera rotation metadata so the image displays upright.
3. **Mode conversion** — If the image mode is not `RGB` or `RGBA` (e.g., `P`, `L`, `CMYK`, `I;16`), convert to `RGBA`. This ensures a consistent pixel format for downstream processing.
4. **Resize** — Call `img.thumbnail((2048, 2048), Image.LANCZOS)`. This shrinks the image in-place so the longest side is at most 2048 pixels, preserving aspect ratio. Lanczos resampling provides high-quality downscaling.
5. **Return** the PIL Image, or `None` if any `OSError` or `ValueError` occurs (failure isolation — skip the file rather than crashing).

### Threading

Plain image renders are dispatched via `_RenderTask(QRunnable)` to a `QThreadPool`. The task:

1. Calls `render_plain_image()` in the worker thread.
2. Computes the cache file path via `_cache_file_path()`.
3. Calls `save_to_cache()` to write the rendered image to disk.
4. Invokes the `on_done` callback, which persists metadata to the database and emits `thumbnailReady`.

## PSD/PSB Compositing Pipeline

### Entry Point

`thumbnail.py` → `render_psd_image(file_path, large_canvas_threshold_mp, tile_grid_x, tile_grid_y) -> Image.Image | None`

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

4. **Resize** the composited image to master long edge (2048px) if needed.
5. **Serialize** to PNG bytes via `io.BytesIO` (PIL Images are not picklable across process boundaries).
6. **Return** raw PNG bytes, or `None` on any failure.

### Main Process

`render_psd_image()` in the main process:

1. Submits the work to the `ProcessPoolExecutor`.
2. Waits for the result with a **2-minute timeout** (`future.result(timeout=120)`).
3. If bytes are returned, deserializes via `Image.open(io.BytesIO(result_bytes))`.
4. Returns the PIL Image, or `None` on timeout or failure.

### Threading

PSD renders are dispatched via `_RenderPSDTask(QRunnable)` to the `QThreadPool`. This task wraps the `render_psd_image()` call (which internally uses `ProcessPoolExecutor`) in a worker thread, keeping the main UI thread free.

## Cache System

### Cache Key Generation

```python
sha = hashlib.sha1(str(file_abs_path.resolve()).encode()).hexdigest()
```

The cache filename is a SHA-1 hash of the source file's **resolved absolute path**. This ensures:
- Deterministic: the same path always maps to the same cache file.
- Unique: different paths produce different hashes (collision resistance).
- Stable: renaming a file invalidates its cache (different path = different hash).

### Cache Layout

```
<user_data_dir>/cache/previews/<sha1>.png   (PNG format, default)
<user_data_dir>/cache/previews/<sha1>.jpg   (JPEG format, if configured)
```

Platform-specific base directories:
- Linux: `~/.local/share/tarragon/cache/previews/`
- macOS: `~/Library/Application Support/tarragon/cache/previews/`
- Windows: `%APPDATA%\tarragon\cache\previews\`

The cache directory is created on demand by `save_to_cache()` via `cache_path.parent.mkdir(parents=True, exist_ok=True)`.

### Cache Format

Configurable via the `cache_format` setting:

| Format | Extension | Notes |
|--------|-----------|-------|
| PNG (default) | `.png` | Lossless, supports RGBA natively |
| JPEG | `.jpg` | Smaller files; RGBA flattened onto white background; quality=90 |

### Cache Invalidation

Cache validity is checked in `ThumbnailService.check_and_render()`:

1. Look up the thumbnail record in the database by path.
2. Compare stored `mtime` and `size` against the current file's stat values.
3. If both match **and** the cache file exists on disk **and** opens successfully → cache hit, load directly.
4. Otherwise → cache miss or stale, dispatch a re-render.

This means the cache is invalidated when:
- The source file is modified (mtime changes).
- The source file is replaced with a different-size file.
- The cache file is deleted or corrupted.
- The source file is moved/renamed (different path = different cache key).

## Color Tagging Integration

After a successful render, if `color_tag_enabled` is `True`, the `ThumbnailService._on_done()` callback extracts dominant color tags from the rendered image and persists them to the database.

The color tagging algorithm runs on the in-memory PIL Image (already resized to master resolution), so it operates on the cached master — not the original file. See [color-tagging.md](color-tagging.md) for the full algorithm description.

## Performance Characteristics

| Operation | Typical Time | Bottleneck |
|-----------|-------------|------------|
| Plain image render (cache miss) | 50–200ms | Disk I/O + decode |
| Plain image render (cache hit) | 5–20ms | Disk I/O + decode from cache |
| PSD composite (small, <20MP) | 500ms–3s | psd-tools compositing |
| PSD composite (large, tiled) | 2–10s | psd-tools per-tile compositing |
| Cache write (PNG) | 20–100ms | Disk I/O + PNG encode |
| Color tag extraction | 5–20ms | Quantize + HSV conversion |

The `ProcessPoolExecutor` allows multiple PSD files to be composited in parallel (up to `max_psd_workers`), while the `QThreadPool` manages overall concurrency for all render tasks.
