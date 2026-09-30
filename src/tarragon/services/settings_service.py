"""Typed key-value store backed by SQLite.

14 settings are described declaratively by ``_SettingSpec`` entries and exposed
through one generic :class:`Setting` class.  Behavior (defaults, clamping,
validation, valid-formats) is data-driven instead of duplicated across
boilerplate subclasses.
"""

from __future__ import annotations

import json
import logging
import re
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, cast

from tarragon.db.database import Database
from tarragon.theme.constants import MULTI_PREVIEW_MAX_DEFAULT

logger = logging.getLogger(__name__)

_TILE_GRID_PATTERN = re.compile(r"^\d+x\d+$")


def _tile_grid_validator(value: object) -> bool:
    """Regex pre-check that grid strings look like 'NxN'.

    The spec's valid-list membership is authoritative; this validator is an
    additional shape pre-check kept for defense in depth.
    """
    return _TILE_GRID_PATTERN.match(str(value)) is not None


@dataclass(frozen=True)
class _SettingSpec:
    """Immutable description of a single settings entry."""

    key: str
    default: object
    min: float | None = None
    max: float | None = None
    # Choice list: governs validation AND get_valid_formats().
    valid: tuple[str, ...] | None = None
    # Optional pre-check (e.g. tile-grid regex); membership in ``valid`` wins.
    validator: Callable[[object], bool] | None = None


class Setting[T]:
    """Typed setting that stores its values in a Database instance.

    ``T`` is the runtime value type (str, int, float, bool, or str | None).
    Values are loaded lazily from the DB; on first load (and on reload) numeric
    bounds are clamped and validation is applied, falling back to the spec
    default when a stored value is invalid.
    """

    def __init__(self, db: Database, spec: _SettingSpec) -> None:
        self._db = db
        self._key = spec.key
        self._default: T = cast(T, spec.default)
        self._min = spec.min
        self._max = spec.max
        self._valid = spec.valid
        self._validator = spec.validator
        self._value: T = self._default
        self._loaded = False

    @classmethod
    def from_spec(cls, db: Database, spec: _SettingSpec) -> Setting[T]:
        """Build a Setting from a spec (``T`` inferred from the assignment context)."""
        return cls(db, spec)

    def _get_from_db(self) -> object:
        """Read a setting value from Database."""
        raw = self._db.get_setting(self._key)
        if raw is None:
            logger.debug("Setting %s was not in the DB, returning default value: %s", self._key, self._default)
            return self._default
        return cast(object, json.loads(raw))

    def _clamp(self, value: object) -> object:
        """Clamp *value* into the configured [min, max] bounds."""
        return_val = cast(Any, value)
        if self._min is not None:
            return_val = max(self._min, return_val)
        if self._max is not None:
            return_val = min(self._max, return_val)
        return cast(object, return_val)

    def _validate(self, value: object) -> bool:
        """Return True when *value* satisfies the spec's valid-list and validator."""
        if self._valid is not None and str(value) not in self._valid:
            return False
        if self._validator is not None and not self._validator(value):
            return False
        return True

    def _validation_error_message(self, value: object) -> str:
        """Build the ValueError message for an invalid *value*."""
        if self._valid is not None:
            return f"Invalid {self._key}: {value!r}. Expected one of {list(self._valid)}."
        return f"Invalid {self._key}: {value!r}."

    def _load(self) -> None:
        """Read from the DB and re-apply clamp/validation with default fallback."""
        value: object = self._default
        try:
            value = self._get_from_db()
            # Exact-type read-repair: numeric/bool settings reject any stored
            # value whose type differs from the default (float for int, bool
            # for int/float) before clamp/validate, which would otherwise
            # silently coerce comparable-but-wrong types or crash startup.
            if isinstance(self._default, (int, float, bool)) and type(value) is not type(self._default):
                logger.warning(
                    "Setting %s has unreadable stored value %r; falling back to default %r",
                    self._key,
                    value,
                    self._default,
                )
                self._value = self._default
            else:
                value = self._clamp(value)
                if not self._validate(value):
                    logger.warning("Setting %s has invalid stored value %r; falling back to default", self._key, value)
                    self._value = self._default
                else:
                    self._value = cast(T, value)
        except Exception:
            # Read-repair contract: any bad stored row (wrong type, malformed
            # shape) must warn and fall back, never crash startup.
            logger.warning("Setting %s has unreadable stored value %r; falling back to default", self._key, value)
            self._value = self._default
        self._loaded = True

    def get_key(self) -> str:
        """Returning the key for the setting"""
        return self._key

    def get_valid_formats(self) -> list[str]:
        """Return the choice list (empty when the setting has no valid-list)."""
        return list(self._valid) if self._valid is not None else []

    def get_min(self) -> float | None:
        return self._min

    def get_max(self) -> float | None:
        return self._max

    def get(self) -> T:
        """Read a setting value, loading it from the DB on first access."""
        if not self._loaded:
            self._load()
        return self._value

    def set(self, value: T) -> None:
        """Persist a setting value after validation and clamping.

        Raises ValueError when *value* fails the spec's validation or does not
        match the setting's declared type.
        """
        logger.debug("Updating setting: %s, with value: %s", self._key, value)
        if self._default is not None and type(value) is not type(self._default):
            error = f"Invalid {self._key}: {value!r}. Expected type {type(self._default).__name__}."
            logger.error(error)
            raise ValueError(error)
        if not self._validate(value):
            error = self._validation_error_message(value)
            logger.error(error)
            raise ValueError(error)
        self._value = cast(T, self._clamp(value))
        self._db.set_setting(self._key, json.dumps(self._value))

    def reload(self) -> None:
        """Re-read the value from the DB and re-apply clamp/validate rules."""
        self._load()


_SPECS: dict[str, _SettingSpec] = {
    "cache_dir": _SettingSpec(key="cache_dir", default=None),
    "cache_format": _SettingSpec(key="cache_format", default="PNG", valid=("PNG", "JPEG")),
    "clear_full_res_on_exit": _SettingSpec(key="clear_full_res_on_exit", default=True),
    "color_tag_enabled": _SettingSpec(key="color_tag_enabled", default=True),
    "color_tag_palette_size": _SettingSpec(key="color_tag_palette_size", default=8, min=2, max=32),
    "color_tag_min_share": _SettingSpec(key="color_tag_min_share", default=0.10, min=0.0, max=1.0),
    "color_tag_neutral_s_threshold": _SettingSpec(key="color_tag_neutral_s_threshold", default=0.15, min=0.0, max=1.0),
    "debug_mode": _SettingSpec(key="debug_mode", default=False),
    "large_canvas_threshold_mp": _SettingSpec(key="large_canvas_threshold_mp", default=20.0, min=0.1, max=1000.0),
    "max_multi_preview": _SettingSpec(key="max_multi_preview", default=MULTI_PREVIEW_MAX_DEFAULT, min=1, max=100),
    "max_psd_workers": _SettingSpec(key="max_psd_workers", default=3, min=1, max=8),
    "tile_grid_size": _SettingSpec(
        key="tile_grid_size", default="3x3", valid=("1x1", "2x2", "3x3", "4x4"), validator=_tile_grid_validator
    ),
    "window_layout_state": _SettingSpec(key="window_layout_state", default=None),
    "window_geometry_state": _SettingSpec(key="window_geometry_state", default=None),
}


class SettingsService:
    """Typed settings repository that delegates storage to a Database instance.

    Exposes one typed :class:`Setting` attribute per spec entry so consumers
    keep calling ``settings_service.<name>.get()`` / ``.set()`` unchanged.
    """

    def __init__(self, db: Database) -> None:
        self._db = db

        self.cache_dir: Setting[str | None] = Setting.from_spec(db, _SPECS["cache_dir"])
        self.cache_format: Setting[str] = Setting.from_spec(db, _SPECS["cache_format"])
        self.clear_full_res_on_exit: Setting[bool] = Setting.from_spec(db, _SPECS["clear_full_res_on_exit"])
        self.color_tag_enabled: Setting[bool] = Setting.from_spec(db, _SPECS["color_tag_enabled"])
        self.color_tag_palette_size: Setting[int] = Setting.from_spec(db, _SPECS["color_tag_palette_size"])
        self.color_tag_min_share: Setting[float] = Setting.from_spec(db, _SPECS["color_tag_min_share"])
        self.color_tag_neutral_s_threshold: Setting[float] = Setting.from_spec(
            db, _SPECS["color_tag_neutral_s_threshold"]
        )
        self.debug_mode: Setting[bool] = Setting.from_spec(db, _SPECS["debug_mode"])
        self.large_canvas_threshold_mp: Setting[float] = Setting.from_spec(db, _SPECS["large_canvas_threshold_mp"])
        self.max_multi_preview: Setting[int] = Setting.from_spec(db, _SPECS["max_multi_preview"])
        self.max_psd_workers: Setting[int] = Setting.from_spec(db, _SPECS["max_psd_workers"])
        self.tile_grid_size: Setting[str] = Setting.from_spec(db, _SPECS["tile_grid_size"])
        self.window_layout_state: Setting[str | None] = Setting.from_spec(db, _SPECS["window_layout_state"])
        self.window_geometry_state: Setting[str | None] = Setting.from_spec(db, _SPECS["window_geometry_state"])

        self._all_settings: tuple[Setting[Any], ...] = (
            self.cache_dir,
            self.cache_format,
            self.clear_full_res_on_exit,
            self.color_tag_enabled,
            self.color_tag_palette_size,
            self.color_tag_min_share,
            self.color_tag_neutral_s_threshold,
            self.debug_mode,
            self.large_canvas_threshold_mp,
            self.max_multi_preview,
            self.max_psd_workers,
            self.tile_grid_size,
            self.window_layout_state,
            self.window_geometry_state,
        )

    def reload(self) -> None:
        """Re-read every setting from the DB, re-applying clamp/validate rules."""
        for setting in self._all_settings:
            setting.reload()
