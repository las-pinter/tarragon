"""Tests for src/tarragon/services/settings_service.py — attribute-based Setting subclasses."""

from pathlib import Path

import pytest
from tarragon.db.database import Database
from tarragon.services.settings_service import (
    Setting,
    SettingsService,
    _SettingCacheDir,
    _SettingCacheFormat,
    _SettingColorTagEnabled,
    _SettingColorTagMinShare,
    _SettingColorTagNeutralSThreshold,
    _SettingColorTagPaletteSize,
    _SettingDebugMode,
    _SettingLargeCanvasThresholdMp,
    _SettingMaxMultiPreview,
    _SettingMaxPsdWorkers,
    _SettingTileGridSize,
    _SettingWindowGeometryState,
    _SettingWindowLayoutState,
)

# ── Fixtures ──────────────────────────────────────────────────────────────────


@pytest.fixture()
def db() -> Database:
    """In-memory SQLite database with schema initialized."""
    database = Database(Path(":memory:"))
    database.init_schema()
    return database


@pytest.fixture()
def service(db: Database) -> SettingsService:
    """SettingsService backed by an in-memory SQLite database."""
    return SettingsService(db)


# ── SettingsService initialization ────────────────────────────────────────────


class TestSettingsServiceInit:
    def test_creates_all_settings(self, service: SettingsService) -> None:
        """SettingsService creates all expected setting attributes."""
        assert hasattr(service, "cache_dir")
        assert hasattr(service, "cache_format")
        assert hasattr(service, "color_tag_enabled")
        assert hasattr(service, "color_tag_palette_size")
        assert hasattr(service, "color_tag_min_share")
        assert hasattr(service, "color_tag_neutral_s_threshold")
        assert hasattr(service, "debug_mode")
        assert hasattr(service, "large_canvas_threshold_mp")
        assert hasattr(service, "max_multi_preview")
        assert hasattr(service, "max_psd_workers")
        assert hasattr(service, "tile_grid_size")
        assert hasattr(service, "window_layout_state")
        assert hasattr(service, "window_geometry_state")

    def test_settings_are_setting_instances(self, service: SettingsService) -> None:
        """All setting attributes are Setting subclass instances."""
        assert isinstance(service.cache_dir, _SettingCacheDir)
        assert isinstance(service.cache_format, _SettingCacheFormat)
        assert isinstance(service.color_tag_enabled, _SettingColorTagEnabled)
        assert isinstance(service.color_tag_palette_size, _SettingColorTagPaletteSize)
        assert isinstance(service.color_tag_min_share, _SettingColorTagMinShare)
        assert isinstance(service.color_tag_neutral_s_threshold, _SettingColorTagNeutralSThreshold)
        assert isinstance(service.debug_mode, _SettingDebugMode)
        assert isinstance(service.large_canvas_threshold_mp, _SettingLargeCanvasThresholdMp)
        assert isinstance(service.max_multi_preview, _SettingMaxMultiPreview)
        assert isinstance(service.max_psd_workers, _SettingMaxPsdWorkers)
        assert isinstance(service.tile_grid_size, _SettingTileGridSize)
        assert isinstance(service.window_layout_state, _SettingWindowLayoutState)
        assert isinstance(service.window_geometry_state, _SettingWindowGeometryState)

    def test_all_settings_inherit_from_setting(self, service: SettingsService) -> None:
        """All settings inherit from the base Setting class."""
        assert isinstance(service.cache_dir, Setting)
        assert isinstance(service.cache_format, Setting)
        assert isinstance(service.max_psd_workers, Setting)


# ── max_psd_workers ───────────────────────────────────────────────────────────


class TestMaxPsdWorkers:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.max_psd_workers.get() == 3

    def test_set_and_get(self, service: SettingsService) -> None:
        service.max_psd_workers.set(5)
        assert service.max_psd_workers.get() == 5

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.max_psd_workers.set(99)
        assert service.max_psd_workers.get() == 8

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.max_psd_workers.set(0)
        assert service.max_psd_workers.get() == 1

    def test_clamps_negative(self, service: SettingsService) -> None:
        service.max_psd_workers.set(-5)
        assert service.max_psd_workers.get() == 1

    def test_boundary_low(self, service: SettingsService) -> None:
        service.max_psd_workers.set(1)
        assert service.max_psd_workers.get() == 1

    def test_boundary_high(self, service: SettingsService) -> None:
        service.max_psd_workers.set(8)
        assert service.max_psd_workers.get() == 8

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.max_psd_workers.get_key() == "max_psd_workers"

    def test_setting_min_max(self, service: SettingsService) -> None:
        assert service.max_psd_workers.get_min() == 1
        assert service.max_psd_workers.get_max() == 8


# ── max_multi_preview ─────────────────────────────────────────────────────────


class TestMaxMultiPreview:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.max_multi_preview.get() == 9

    def test_set_and_get(self, service: SettingsService) -> None:
        service.max_multi_preview.set(25)
        assert service.max_multi_preview.get() == 25

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.max_multi_preview.set(200)
        assert service.max_multi_preview.get() == 100

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.max_multi_preview.set(0)
        assert service.max_multi_preview.get() == 1

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.max_multi_preview.get_key() == "max_multi_preview"


# ── large_canvas_threshold_mp ─────────────────────────────────────────────────


class TestLargeCanvasThresholdMp:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.large_canvas_threshold_mp.get() == 20.0

    def test_set_and_get(self, service: SettingsService) -> None:
        service.large_canvas_threshold_mp.set(50.5)
        assert service.large_canvas_threshold_mp.get() == 50.5

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.large_canvas_threshold_mp.set(9999.0)
        assert service.large_canvas_threshold_mp.get() == 1000.0

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.large_canvas_threshold_mp.set(0.0)
        assert service.large_canvas_threshold_mp.get() == 0.1

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.large_canvas_threshold_mp.get_key() == "large_canvas_threshold_mp"


# ── tile_grid_size ────────────────────────────────────────────────────────────


class TestTileGridSize:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.tile_grid_size.get() == "2x2"

    def test_set_and_get(self, service: SettingsService) -> None:
        service.tile_grid_size.set("3x3")
        assert service.tile_grid_size.get() == "3x3"

    def test_set_asymmetric(self, service: SettingsService) -> None:
        service.tile_grid_size.set("4x2")
        assert service.tile_grid_size.get() == "4x2"

    def test_invalid_format_raises(self, service: SettingsService) -> None:
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.tile_grid_size.set("abc")

    def test_invalid_empty_raises(self, service: SettingsService) -> None:
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.tile_grid_size.set("")

    def test_invalid_single_number_raises(self, service: SettingsService) -> None:
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.tile_grid_size.set("4")

    def test_valid_formats(self, service: SettingsService) -> None:
        valid = service.tile_grid_size.get_valid_formats()
        assert "1x1" in valid
        assert "2x2" in valid
        assert "3x3" in valid
        assert "4x4" in valid

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.tile_grid_size.get_key() == "tile_grid_size"


# ── color_tag_enabled ─────────────────────────────────────────────────────────


class TestColorTagEnabled:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.color_tag_enabled.get() is True

    def test_set_false(self, service: SettingsService) -> None:
        service.color_tag_enabled.set(False)
        assert service.color_tag_enabled.get() is False

    def test_set_true(self, service: SettingsService) -> None:
        service.color_tag_enabled.set(False)
        service.color_tag_enabled.set(True)
        assert service.color_tag_enabled.get() is True

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.color_tag_enabled.get_key() == "color_tag_enabled"


# ── color_tag_palette_size ────────────────────────────────────────────────────


class TestColorTagPaletteSize:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.color_tag_palette_size.get() == 8

    def test_set_and_get(self, service: SettingsService) -> None:
        service.color_tag_palette_size.set(16)
        assert service.color_tag_palette_size.get() == 16

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.color_tag_palette_size.set(100)
        assert service.color_tag_palette_size.get() == 32

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.color_tag_palette_size.set(0)
        assert service.color_tag_palette_size.get() == 2

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.color_tag_palette_size.get_key() == "color_tag_palette_size"


# ── color_tag_min_share ──────────────────────────────────────────────────────


class TestColorTagMinShare:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.color_tag_min_share.get() == pytest.approx(0.10)

    def test_set_and_get(self, service: SettingsService) -> None:
        service.color_tag_min_share.set(0.25)
        assert service.color_tag_min_share.get() == pytest.approx(0.25)

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.color_tag_min_share.set(1.5)
        assert service.color_tag_min_share.get() == pytest.approx(1.0)

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.color_tag_min_share.set(-0.5)
        assert service.color_tag_min_share.get() == pytest.approx(0.0)

    def test_boundary_zero(self, service: SettingsService) -> None:
        service.color_tag_min_share.set(0.0)
        assert service.color_tag_min_share.get() == pytest.approx(0.0)

    def test_boundary_one(self, service: SettingsService) -> None:
        service.color_tag_min_share.set(1.0)
        assert service.color_tag_min_share.get() == pytest.approx(1.0)

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.color_tag_min_share.get_key() == "color_tag_min_share"


# ── color_tag_neutral_s_threshold ─────────────────────────────────────────────


class TestColorTagNeutralSThreshold:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(0.15)

    def test_set_and_get(self, service: SettingsService) -> None:
        service.color_tag_neutral_s_threshold.set(0.30)
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(0.30)

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.color_tag_neutral_s_threshold.set(2.0)
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(1.0)

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.color_tag_neutral_s_threshold.set(-1.0)
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(0.0)

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.color_tag_neutral_s_threshold.get_key() == "color_tag_neutral_s_threshold"


# ── cache_format ──────────────────────────────────────────────────────────────


class TestCacheFormat:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.cache_format.get() == "PNG"

    def test_set_jpeg(self, service: SettingsService) -> None:
        service.cache_format.set("JPEG")
        assert service.cache_format.get() == "JPEG"

    def test_set_png(self, service: SettingsService) -> None:
        service.cache_format.set("JPEG")
        service.cache_format.set("PNG")
        assert service.cache_format.get() == "PNG"

    def test_invalid_format_raises(self, service: SettingsService) -> None:
        with pytest.raises(ValueError, match="Invalid cache_format"):
            service.cache_format.set("bmp")

    def test_invalid_lowercase_raises(self, service: SettingsService) -> None:
        """Lowercase format names are invalid (must be uppercase)."""
        with pytest.raises(ValueError, match="Invalid cache_format"):
            service.cache_format.set("png")

    def test_valid_formats(self, service: SettingsService) -> None:
        valid = service.cache_format.get_valid_formats()
        assert "PNG" in valid
        assert "JPEG" in valid

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.cache_format.get_key() == "cache_format"


# ── cache_dir ─────────────────────────────────────────────────────────────────


class TestCacheDir:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.cache_dir.get() is None

    def test_set_and_get(self, service: SettingsService) -> None:
        service.cache_dir.set("/tmp/cache")
        assert service.cache_dir.get() == "/tmp/cache"

    def test_set_none(self, service: SettingsService) -> None:
        service.cache_dir.set("/tmp/cache")
        service.cache_dir.set(None)
        assert service.cache_dir.get() is None

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.cache_dir.get_key() == "cache_dir"


# ── debug_mode ────────────────────────────────────────────────────────────────


class TestDebugMode:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.debug_mode.get() is False

    def test_set_true(self, service: SettingsService) -> None:
        service.debug_mode.set(True)
        assert service.debug_mode.get() is True

    def test_set_false(self, service: SettingsService) -> None:
        service.debug_mode.set(True)
        service.debug_mode.set(False)
        assert service.debug_mode.get() is False

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.debug_mode.get_key() == "debug_mode"


# ── window_layout_state ───────────────────────────────────────────────────────


class TestWindowLayoutState:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.window_layout_state.get() is None

    def test_set_and_get(self, service: SettingsService) -> None:
        state = '{"x": 100, "y": 200}'
        service.window_layout_state.set(state)
        assert service.window_layout_state.get() == state

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.window_layout_state.get_key() == "window_layout_state"


# ── window_geometry_state ─────────────────────────────────────────────────────


class TestWindowGeometryState:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.window_geometry_state.get() is None

    def test_set_and_get(self, service: SettingsService) -> None:
        state = '{"width": 800, "height": 600}'
        service.window_geometry_state.set(state)
        assert service.window_geometry_state.get() == state

    def test_setting_key(self, service: SettingsService) -> None:
        assert service.window_geometry_state.get_key() == "window_geometry_state"


# ── Persistence ───────────────────────────────────────────────────────────────


class TestPersistence:
    def test_value_persists_in_database(self, db: Database) -> None:
        """Values set via SettingsService persist in the database."""
        service1 = SettingsService(db)
        service1.max_psd_workers.set(5)

        # Create a new service instance pointing to the same DB
        service2 = SettingsService(db)
        assert service2.max_psd_workers.get() == 5

    def test_multiple_settings_persist(self, db: Database) -> None:
        """Multiple settings can be set and retrieved."""
        service = SettingsService(db)
        service.max_psd_workers.set(4)
        service.cache_format.set("JPEG")
        service.debug_mode.set(True)

        # New service instance should see all values
        service2 = SettingsService(db)
        assert service2.max_psd_workers.get() == 4
        assert service2.cache_format.get() == "JPEG"
        assert service2.debug_mode.get() is True
