# Tarragon Database Schema

## Overview

Tarragon uses a single SQLite database to store thumbnail metadata, tags, favorites, settings, and editor associations. The database file is located at:

```
<platformdirs.user_data_dir("tarragon")>/tarragon.db
```

Platform-specific paths:
- Linux: `~/.local/share/tarragon/tarragon.db`
- macOS: `~/Library/Application Support/tarragon/tarragon.db`
- Windows: `%APPDATA%\tarragon\tarragon.db`

The database uses **WAL (Write-Ahead Logging)** journal mode for concurrent read/write access, and all operations are serialized through a `threading.Lock` for thread safety.

## Schema Tables

### `schema_version`

Tracks the current database schema version for migration support.

```sql
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);
```

| Column | Type | Description |
|--------|------|-------------|
| `version` | INTEGER | Current schema version number |

The table holds at most one row. Version 0 indicates an uninitialized database. The `MigrationRunner` bootstraps to version 1 on first run.

### `thumbnails`

Stores metadata and cache paths for rendered image thumbnails.

```sql
CREATE TABLE IF NOT EXISTS thumbnails (
    path TEXT PRIMARY KEY,
    mtime INTEGER NOT NULL,
    size INTEGER NOT NULL,
    thumb_hash TEXT,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    master_cache_path TEXT
);
```

| Column | Type | Description |
|--------|------|-------------|
| `path` | TEXT (PK) | Absolute path to the source image file |
| `mtime` | INTEGER | File modification time (Unix timestamp, integer) |
| `size` | INTEGER | File size in bytes |
| `thumb_hash` | TEXT | Reserved for future ThumbHash integration |
| `width` | INTEGER | Rendered image width in pixels |
| `height` | INTEGER | Rendered image height in pixels |
| `created_at` | TEXT | ISO timestamp of record creation |
| `master_cache_path` | TEXT | Absolute path to the cached master PNG/JPEG file |

**Cache invalidation**: A thumbnail record is considered valid when both `mtime` and `size` match the current file's stat values. If either differs, the entry is stale and the image is re-rendered.

### `tags`

Registry of all tag names used in the system.

```sql
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL
);
```

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER (PK) | Auto-incrementing tag identifier |
| `name` | TEXT (UNIQUE) | Tag name (e.g., `"color:red"`, `"landscape"`) |

Tags are created on demand via `INSERT OR IGNORE` / `ON CONFLICT` patterns. Color tags follow the naming convention `color:<bucket_name>` (e.g., `color:red`, `color:blue`).

### `file_tags`

Associates files with tags, supporting multiple tag sources.

```sql
CREATE TABLE IF NOT EXISTS file_tags (
    path TEXT NOT NULL,
    tag_id INTEGER NOT NULL,
    source TEXT NOT NULL DEFAULT 'user',
    PRIMARY KEY (path, tag_id, source),
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);
```

| Column | Type | Description |
|--------|------|-------------|
| `path` | TEXT | Absolute path to the source image file |
| `tag_id` | INTEGER (FK) | Reference to `tags.id` |
| `source` | TEXT | Origin of the tag assignment |

**Composite primary key**: `(path, tag_id, source)` — a file can have the same tag from different sources.

**Tag sources**:
- `user` — Manually assigned by the user through the tag panel.
- `auto_color` — Automatically assigned by the color tagging algorithm.

### `favorites`

User-curated list of favorite image files.

```sql
CREATE TABLE IF NOT EXISTS favorites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    label TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0
);
```

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER (PK) | Auto-incrementing identifier |
| `path` | TEXT (UNIQUE) | Absolute path to the favorited file |
| `label` | TEXT | Optional user-defined label |
| `sort_order` | INTEGER | Display ordering (lower = first) |

### `settings`

Key-value store for application configuration.

```sql
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

| Column | Type | Description |
|--------|------|-------------|
| `key` | TEXT (PK) | Setting name |
| `value` | TEXT | JSON-serialized setting value |

Values are JSON-serialized by the `Settings` class before storage and deserialized on read. The `SettingsService` provides typed accessors with validation and range clamping.

**Default settings** (initialized on first run via `Settings.init_defaults()`):

| Key | Default | Type | Description |
|-----|---------|------|-------------|
| `max_multi_preview` | `9` | int | Maximum images in multi-preview mosaic |
| `large_canvas_threshold_mp` | `20.0` | float | PSD canvas size threshold for tiled compositing (megapixels) |
| `tile_grid_size` | `"2x2"` | str | Tile grid dimensions for large PSD compositing |
| `max_psd_workers` | `3` | int | Maximum PSD compositing worker processes |
| `color_tag_enabled` | `true` | bool | Enable/disable automatic color tagging |
| `color_tag_palette_size` | `8` | int | Number of colors in quantized palette |
| `color_tag_min_share` | `0.10` | float | Minimum pixel share for a color tag (10%) |
| `color_tag_neutral_s_threshold` | `0.15` | float | Saturation threshold for neutral classification |
| `cache_format` | `"png"` | str | Cache image format (`"png"` or `"jpeg"`) |

### `editor_associations`

Maps file extensions to external editor command templates.

```sql
CREATE TABLE IF NOT EXISTS editor_associations (
    extension TEXT PRIMARY KEY,
    command_template TEXT NOT NULL
);
```

| Column | Type | Description |
|--------|------|-------------|
| `extension` | TEXT (PK) | File extension including dot (e.g., `".psd"`, `"*"`) |
| `command_template` | TEXT | Command template with `{file}` placeholder |

The `"*"` extension serves as a wildcard fallback. When launching an editor, the system first checks for a specific extension match, then falls back to `"*"`, and finally uses the OS default handler if no association exists.

## CRUD Operations

### Thumbnails

| Operation | Method | Description |
|-----------|--------|-------------|
| Create/Update | `upsert_thumbnail(path, mtime, size, width, height, master_cache_path)` | INSERT or UPDATE on conflict(path) |
| Read | `get_thumbnail(path)` | Fetch single record as dict, or None |
| List | `list_thumbnails_for_folder(folder_path)` | All records with path prefix match |
| Delete | `delete_thumbnail(path)` | Remove record by path |

### Tags

| Operation | Method | Description |
|-----------|--------|-------------|
| Create | `ensure_tag(name)` | Insert if absent; always returns tag id |
| Associate | `add_file_tags(paths, tag_id, source)` | Associate paths with a tag (INSERT OR IGNORE) |
| Disassociate | `remove_file_tags(paths, tag_id)` | Remove tag associations for paths |
| Query | `get_file_tag_ids(path)` | Return set of tag IDs for a path |
| Replace auto | `replace_auto_color_tags(path, tags)` | Delete old auto_color tags, insert new ones |

### Favorites

| Operation | Method | Description |
|-----------|--------|-------------|
| Add | `add_favorite(path, label, sort_order)` | INSERT OR IGNORE |
| Remove | `remove_favorite(path)` | Delete by path |
| List | `list_favorites()` | All favorites ordered by sort_order, then path |

### Settings

| Operation | Method | Description |
|-----------|--------|-------------|
| Read | `get_setting(key)` | Raw string value, or None if absent |
| Write | `set_setting(key, value)` | INSERT OR REPLACE |

### Editor Associations

| Operation | Method | Description |
|-----------|--------|-------------|
| Read | `get_editor_command(extension)` | Command template, or None |
| Create/Update | `upsert_editor_association(extension, command_template)` | INSERT OR REPLACE |
| Delete | `remove_editor_association(extension)` | Delete by extension |

## Migration Framework

The `MigrationRunner` class in `migrations.py` manages schema evolution:

1. Calls `db.init_schema()` to ensure all tables exist (idempotent).
2. Checks `get_schema_version()` — if 0, bootstraps to version 1.
3. Returns the current schema version.

Future migrations will be added as versioned functions that transform the schema incrementally. The runner checks the current version and applies only pending migrations.

## Thread Safety

The `Database` class uses a `threading.Lock` to serialize all SQL operations:

- `_execute(sql, params)` — Single statement execution.
- `_executemany(sql, seq)` — Batch execution.
- `_executescript(sql)` — Multi-statement script execution.
- `_commit()` — Transaction commit.

This allows the database to be safely shared across the main thread, QThreadPool workers, and service layer without race conditions.
