"""Typed key-value store backed by SQLite."""

from __future__ import annotations

import json
import logging
import re
from typing import TYPE_CHECKING, Any, cast

if TYPE_CHECKING:
    from tarragon.db.database import Database

from tarragon.db.database import Database
from tarragon.theme.constants import MULTI_PREVIEW_MAX_DEFAULT

logger = logging.getLogger(__name__)

SettingValue = str | int | float | bool | None


class Setting:
    """Typed setting that stores its values in a Database instance."""

    def __init__(
        self,
        db: Database,
        key: str,
        default: Any,
        min: int | float | None = None,
        max: int | float | None = None,
    ) -> None:
        self._key = key
        self._db = db
        self._default = default
        self._min = min
        self._max = max
        self._value = None
        self._loaded = False

    def _get_from_db(self) -> Any:
        """Read a setting value from Database"""
        raw = self._db.get_setting(self._key)
        if raw is None:
            logger.debug("Setting %s was not in the DB, returning default value: %s", self._key, self._default)
            return self._default
        return json.loads(raw)

    def _validate(self, value: SettingValue) -> bool:
        """Validating the value with a function given in the arguments"""
        return True

    def _clamp(self, value: Any) -> Any:
        return_val = value
        if self._min is not None:
            return_val = max(self._min, return_val)
        if self._max is not None:
            return_val = min(self._max, return_val)
        return return_val

    def get_key(self) -> str:
        """Returning the key for the setting"""
        return self._key

    def get_valid_formats(self) -> list[Any]:
        return []

    def get_min(self) -> int | float | None:
        return self._min

    def get_max(self) -> int | float | None:
        return self._max

    def get(self) -> SettingValue:
        """Read a setting value"""
        if not self._loaded:
            logger.debug("Setting %s was not yet loaded from DB", self._key)
            self._value = self._get_from_db()
            self._loaded = True
        return self._value

    def set(self, value: SettingValue) -> None:
        """Persist a setting value"""
        logger.debug("Updating setting: %s, with value: %s", self._key, value)
        if self._validate(value):
            clamped_value = self._clamp(value)
            self._db.set_setting(self._key, json.dumps(clamped_value))
            self._value = clamped_value


class _SettingCacheDir(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "cache_dir", None)

    def get(self) -> str | None:
        return cast(str | None, super().get())


class _SettingCacheFormat(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "cache_format", "PNG")
        self._valid_cache_formats = ["PNG", "JPEG"]

    def _validate(self, value: SettingValue) -> bool:
        if str(value) not in self._valid_cache_formats:
            error = f"Invalid cache_format: {value!r}. Expected one of {self._valid_cache_formats}."
            logger.error(error)
            raise ValueError(error)
        return True

    def get_valid_formats(self) -> list[str]:
        return self._valid_cache_formats

    def get(self) -> str:
        return cast(str, super().get())


class _SettingColorTagEnabled(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "color_tag_enabled", True)

    def get(self) -> bool:
        return cast(bool, super().get())


class _SettingColorTagPaletteSize(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "color_tag_palette_size", 8, 2, 32)

    def get(self) -> int:
        return cast(int, super().get())


class _SettingColorTagMinShare(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "color_tag_min_share", 0.10, 0.0, 1.0)

    def get(self) -> float:
        return cast(float, super().get())


class _SettingColorTagNeutralSThreshold(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "color_tag_neutral_s_threshold", 0.15, 0, 1.0)

    def get(self) -> float:
        return cast(float, super().get())


class _SettingDebugMode(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "debug_mode", False)

    def get(self) -> bool:
        return cast(bool, super().get())


class _SettingLargeCanvasThresholdMp(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "large_canvas_threshold_mp", 20.0, 0.1, 1000.0)

    def get(self) -> float:
        return cast(float, super().get())


class _SettingMaxMultiPreview(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "max_multi_preview", MULTI_PREVIEW_MAX_DEFAULT, 1, 100)

    def get(self) -> int:
        return cast(int, super().get())


class _SettingMaxPsdWorkers(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "max_psd_workers", 3, 1, 8)

    def get(self) -> int:
        return cast(int, super().get())


class _SettingTileGridSize(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "tile_grid_size", "2x2")
        self._tile_grid_pattern = re.compile(r"^\d+x\d+$")
        self._valid_cache_formats = ["1x1", "2x2", "3x3", "4x4"]

    def _validate(self, value: SettingValue) -> bool:
        if not self._tile_grid_pattern.match(str(value)):
            error = f"Invalid tile_grid_size: {value!r}. Expected format 'NxN' (e.g. '2x2')."
            logger.error(error)
            raise ValueError(error)
        return True

    def get_valid_formats(self) -> list[str]:
        return self._valid_cache_formats

    def get(self) -> str:
        return cast(str, super().get())


class _SettingWindowLayoutState(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "window_layout_state", None)

    def get(self) -> str | None:
        return cast(str | None, super().get())


class _SettingWindowGeometryState(Setting):
    def __init__(self, db: Database) -> None:
        super().__init__(db, "window_geometry_state", None)

    def get(self) -> str | None:
        return cast(str | None, super().get())


class SettingsService:
    """Typed settings repository that delegates storage to a Database instance."""

    def __init__(self, db: Database) -> None:
        self._db = db

        self.cache_dir = _SettingCacheDir(db)
        self.cache_format = _SettingCacheFormat(db)
        self.color_tag_enabled = _SettingColorTagEnabled(db)
        self.color_tag_palette_size = _SettingColorTagPaletteSize(db)
        self.color_tag_min_share = _SettingColorTagMinShare(db)
        self.color_tag_neutral_s_threshold = _SettingColorTagNeutralSThreshold(db)
        self.debug_mode = _SettingDebugMode(db)
        self.large_canvas_threshold_mp = _SettingLargeCanvasThresholdMp(db)
        self.max_multi_preview = _SettingMaxMultiPreview(db)
        self.max_psd_workers = _SettingMaxPsdWorkers(db)
        self.tile_grid_size = _SettingTileGridSize(db)
        self.window_layout_state = _SettingWindowLayoutState(db)
        self.window_geometry_state = _SettingWindowGeometryState(db)
