"""Image render pipeline — backward-compatible facade.

All implementation has been moved to :mod:`tarragon.renderers` sub-modules.
This module re-exports every public and private name so that existing
``from tarragon.thumbnail import X`` statements continue to work.
"""

from __future__ import annotations

# Re-export everything from the renderers sub-packages so that
# ``from tarragon.thumbnail import <name>`` keeps working.
from tarragon.renderers.cache import (  # noqa: F401
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
from tarragon.renderers.clip import render_clip_image  # noqa: F401
from tarragon.renderers.plain import render_plain_image  # noqa: F401
from tarragon.renderers.psd import (  # noqa: F401
    _composite_psd_in_process,
    _compute_worker_count,
    _get_executor,
    _shutdown_executor,
    render_psd_image,
)

__all__ = [
    "MASTER_LONG_EDGE",
    "RESOLUTION_FULL",
    "RESOLUTION_PREVIEW",
    "RESOLUTION_THUMBNAIL",
    "derive_smaller_sizes",
    "generate_cache_paths",
    "generate_cache_uuid",
    "invalidate_cache_files",
    "render_clip_image",
    "render_plain_image",
    "render_psd_image",
    "save_to_cache",
]
