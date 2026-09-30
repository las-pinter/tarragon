"""Extension-to-renderer dispatch registry shared by scanner and thumbnail_service."""

from __future__ import annotations

from collections.abc import Callable
from pathlib import Path
from typing import Any

from PIL import Image

from tarragon.renderers.clip import render_clip_image
from tarragon.renderers.krita import render_kra_image
from tarragon.renderers.plain import render_plain_image
from tarragon.renderers.psd import render_psd_image

SUPPORTED_EXTENSIONS = {
    ".jpg",
    ".jpeg",
    ".png",
    ".webp",
    ".tiff",
    ".tif",
    ".psd",
    ".psb",
    ".clip",
    ".kra",
}

Renderer = Callable[[Path, int | None, Any], Image.Image | None]

_settings_provider: Callable[[], Any] | None = None


def configure_settings(provider: Callable[[], Any]) -> None:
    """Register a callable that returns the current settings service."""
    global _settings_provider
    _settings_provider = provider


def _render_psd(file_path: Path, target_size: int | None, cancel_event: Any) -> Image.Image | None:
    """Composite PSD/PSB, reading large-canvas settings from the configured service."""
    if _settings_provider is None:
        raise RuntimeError("configure_settings() must be called before PSD rendering")
    settings = _settings_provider()
    threshold = settings.large_canvas_threshold_mp.get()
    grid_str = settings.tile_grid_size.get()
    grid_x, grid_y = (int(d) for d in grid_str.split("x"))
    return render_psd_image(
        file_path,
        threshold,
        grid_x,
        grid_y,
        target_size=target_size,
        cancel_event=cancel_event,
    )


def _render_clip(file_path: Path, target_size: int | None, cancel_event: Any) -> Image.Image | None:
    return render_clip_image(file_path, target_size=target_size)


def _render_kra(file_path: Path, target_size: int | None, cancel_event: Any) -> Image.Image | None:
    return render_kra_image(file_path, target_size=target_size)


def _render_default(file_path: Path, target_size: int | None, cancel_event: Any) -> Image.Image | None:
    return render_plain_image(file_path, target_size=target_size)


FORMAT_DISPATCH: dict[str, Renderer] = {
    ".psd": _render_psd,
    ".psb": _render_psd,
    ".clip": _render_clip,
    ".kra": _render_kra,
}

DEFAULT_RENDERER: Renderer = _render_default
