"""Renderers — image rendering pipelines for thumbnail generation.

Re-exports all public names for backward compatibility with
``from tarragon.renderers import <name>``.
"""

from tarragon.renderers.cache import (
    MASTER_LONG_EDGE,
    RESOLUTION_FULL,
    RESOLUTION_PREVIEW,
    RESOLUTION_THUMBNAIL,
    _cache_file_path,
    derive_smaller_sizes,
    generate_cache_paths,
    generate_cache_uuid,
    invalidate_cache_files,
    save_to_cache,
)
from tarragon.renderers.clip import render_clip_image
from tarragon.renderers.plain import render_plain_image
from tarragon.renderers.psd import (
    _compute_worker_count,
    _composite_psd_in_process,
    _get_executor,
    _shutdown_executor,
    render_psd_image,
)

__all__ = [
    "MASTER_LONG_EDGE",
    "RESOLUTION_FULL",
    "RESOLUTION_PREVIEW",
    "RESOLUTION_THUMBNAIL",
    "_cache_file_path",
    "_compute_worker_count",
    "_composite_psd_in_process",
    "_get_executor",
    "_shutdown_executor",
    "derive_smaller_sizes",
    "generate_cache_paths",
    "generate_cache_uuid",
    "invalidate_cache_files",
    "render_clip_image",
    "render_plain_image",
    "render_psd_image",
    "save_to_cache",
]
