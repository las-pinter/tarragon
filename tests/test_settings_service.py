"""Tests for SettingsService"""

from __future__ import annotations

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


@pytest.fixture
def db() -> Database:
    """In-memory SQLite database with schema initialized."""
    database = Database(Path(":memory:"))
    database.init_schema()
    return database


@pytest.fixture
def service(db: Database) -> SettingsService:
    """SettingsService backed by an in-memory SQLite database."""
    return SettingsService(db)


class TestSettingsServiceInit:
    """SettingsService initialization and attribute wiring."""

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


class TestMaxPsdWorkers:
    """Tests for the max_psd_workers setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """max_psd_workers defaults to 3."""
        assert service.max_psd_workers.get() == 3

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a worker count persists and reads back the value."""
        service.max_psd_workers.set(5)
        assert service.max_psd_workers.get() == 5

    def test_clamps_above_max(self, service: SettingsService) -> None:
        """Values above the maximum are clamped to 8."""
        service.max_psd_workers.set(99)
        assert service.max_psd_workers.get() == 8

    def test_clamps_below_min(self, service: SettingsService) -> None:
        """Values below the minimum are clamped to 1."""
        service.max_psd_workers.set(0)
        assert service.max_psd_workers.get() == 1

    def test_clamps_negative(self, service: SettingsService) -> None:
        """Negative values are clamped to the minimum of 1."""
        service.max_psd_workers.set(-5)
        assert service.max_psd_workers.get() == 1

    def test_boundary_low(self, service: SettingsService) -> None:
        """The minimum boundary value 1 is accepted unchanged."""
        service.max_psd_workers.set(1)
        assert service.max_psd_workers.get() == 1

    def test_boundary_high(self, service: SettingsService) -> None:
        """The maximum boundary value 8 is accepted unchanged."""
        service.max_psd_workers.set(8)
        assert service.max_psd_workers.get() == 8

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'max_psd_workers'."""
        assert service.max_psd_workers.get_key() == "max_psd_workers"

    def test_setting_min_max(self, service: SettingsService) -> None:
        """get_min() and get_max() return the configured bounds."""
        assert service.max_psd_workers.get_min() == 1
        assert service.max_psd_workers.get_max() == 8


class TestMaxMultiPreview:
    """Tests for the max_multi_preview setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """max_multi_preview defaults to 9."""
        assert service.max_multi_preview.get() == 9

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a preview count persists and reads back the value."""
        service.max_multi_preview.set(25)
        assert service.max_multi_preview.get() == 25

    def test_clamps_above_max(self, service: SettingsService) -> None:
        """Values above the maximum are clamped to 100."""
        service.max_multi_preview.set(200)
        assert service.max_multi_preview.get() == 100

    def test_clamps_below_min(self, service: SettingsService) -> None:
        """Values below the minimum are clamped to 1."""
        service.max_multi_preview.set(0)
        assert service.max_multi_preview.get() == 1

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'max_multi_preview'."""
        assert service.max_multi_preview.get_key() == "max_multi_preview"


class TestLargeCanvasThresholdMp:
    """Tests for the large_canvas_threshold_mp setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """large_canvas_threshold_mp defaults to 20.0."""
        assert service.large_canvas_threshold_mp.get() == 20.0

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a threshold persists and reads back the value."""
        service.large_canvas_threshold_mp.set(50.5)
        assert service.large_canvas_threshold_mp.get() == 50.5

    def test_clamps_above_max(self, service: SettingsService) -> None:
        """Values above the maximum are clamped to 1000.0."""
        service.large_canvas_threshold_mp.set(9999.0)
        assert service.large_canvas_threshold_mp.get() == 1000.0

    def test_clamps_below_min(self, service: SettingsService) -> None:
        """Values below the minimum are clamped to 0.1."""
        service.large_canvas_threshold_mp.set(0.0)
        assert service.large_canvas_threshold_mp.get() == 0.1

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'large_canvas_threshold_mp'."""
        assert service.large_canvas_threshold_mp.get_key() == "large_canvas_threshold_mp"


class TestTileGridSize:
    """Tests for the tile_grid_size setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """tile_grid_size defaults to '2x2'."""
        assert service.tile_grid_size.get() == "2x2"

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a valid grid size persists and reads back the value."""
        service.tile_grid_size.set("3x3")
        assert service.tile_grid_size.get() == "3x3"

    def test_set_asymmetric(self, service: SettingsService) -> None:
        """Asymmetric grids like '4x2' pass validation."""
        service.tile_grid_size.set("4x2")
        assert service.tile_grid_size.get() == "4x2"

    def test_invalid_format_raises(self, service: SettingsService) -> None:
        """Setting a non-grid string raises ValueError."""
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.tile_grid_size.set("abc")

    def test_invalid_empty_raises(self, service: SettingsService) -> None:
        """Setting an empty string raises ValueError."""
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.tile_grid_size.set("")

    def test_invalid_single_number_raises(self, service: SettingsService) -> None:
        """Setting a single number raises ValueError."""
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.tile_grid_size.set("4")

    def test_valid_formats(self, service: SettingsService) -> None:
        """get_valid_formats() returns all supported grid sizes."""
        valid = service.tile_grid_size.get_valid_formats()
        assert "1x1" in valid
        assert "2x2" in valid
        assert "3x3" in valid
        assert "4x4" in valid

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'tile_grid_size'."""
        assert service.tile_grid_size.get_key() == "tile_grid_size"


class TestColorTagEnabled:
    """Tests for the color_tag_enabled setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """color_tag_enabled defaults to True."""
        assert service.color_tag_enabled.get() is True

    def test_set_false(self, service: SettingsService) -> None:
        """Setting False disables color tagging."""
        service.color_tag_enabled.set(False)
        assert service.color_tag_enabled.get() is False

    def test_set_true(self, service: SettingsService) -> None:
        """Setting True after False re-enables color tagging."""
        service.color_tag_enabled.set(False)
        service.color_tag_enabled.set(True)
        assert service.color_tag_enabled.get() is True

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'color_tag_enabled'."""
        assert service.color_tag_enabled.get_key() == "color_tag_enabled"


class TestColorTagPaletteSize:
    """Tests for the color_tag_palette_size setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """color_tag_palette_size defaults to 8."""
        assert service.color_tag_palette_size.get() == 8

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a palette size persists and reads back the value."""
        service.color_tag_palette_size.set(16)
        assert service.color_tag_palette_size.get() == 16

    def test_clamps_above_max(self, service: SettingsService) -> None:
        """Values above the maximum are clamped to 32."""
        service.color_tag_palette_size.set(100)
        assert service.color_tag_palette_size.get() == 32

    def test_clamps_below_min(self, service: SettingsService) -> None:
        """Values below the minimum are clamped to 2."""
        service.color_tag_palette_size.set(0)
        assert service.color_tag_palette_size.get() == 2

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'color_tag_palette_size'."""
        assert service.color_tag_palette_size.get_key() == "color_tag_palette_size"


class TestColorTagMinShare:
    """Tests for the color_tag_min_share setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """color_tag_min_share defaults to 0.10."""
        assert service.color_tag_min_share.get() == pytest.approx(0.10)

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a share persists and reads back the value."""
        service.color_tag_min_share.set(0.25)
        assert service.color_tag_min_share.get() == pytest.approx(0.25)

    def test_clamps_above_max(self, service: SettingsService) -> None:
        """Values above the maximum are clamped to 1.0."""
        service.color_tag_min_share.set(1.5)
        assert service.color_tag_min_share.get() == pytest.approx(1.0)

    def test_clamps_below_min(self, service: SettingsService) -> None:
        """Values below the minimum are clamped to 0.0."""
        service.color_tag_min_share.set(-0.5)
        assert service.color_tag_min_share.get() == pytest.approx(0.0)

    def test_boundary_zero(self, service: SettingsService) -> None:
        """The minimum boundary value 0.0 is accepted unchanged."""
        service.color_tag_min_share.set(0.0)
        assert service.color_tag_min_share.get() == pytest.approx(0.0)

    def test_boundary_one(self, service: SettingsService) -> None:
        """The maximum boundary value 1.0 is accepted unchanged."""
        service.color_tag_min_share.set(1.0)
        assert service.color_tag_min_share.get() == pytest.approx(1.0)

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'color_tag_min_share'."""
        assert service.color_tag_min_share.get_key() == "color_tag_min_share"


class TestColorTagNeutralSThreshold:
    """Tests for the color_tag_neutral_s_threshold setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """color_tag_neutral_s_threshold defaults to 0.15."""
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(0.15)

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a threshold persists and reads back the value."""
        service.color_tag_neutral_s_threshold.set(0.30)
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(0.30)

    def test_clamps_above_max(self, service: SettingsService) -> None:
        """Values above the maximum are clamped to 1.0."""
        service.color_tag_neutral_s_threshold.set(2.0)
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(1.0)

    def test_clamps_below_min(self, service: SettingsService) -> None:
        """Values below the minimum are clamped to 0.0."""
        service.color_tag_neutral_s_threshold.set(-1.0)
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(0.0)

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'color_tag_neutral_s_threshold'."""
        assert service.color_tag_neutral_s_threshold.get_key() == "color_tag_neutral_s_threshold"


class TestCacheFormat:
    """Tests for the cache_format setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """cache_format defaults to 'PNG'."""
        assert service.cache_format.get() == "PNG"

    def test_set_jpeg(self, service: SettingsService) -> None:
        """Setting 'JPEG' persists and reads back the value."""
        service.cache_format.set("JPEG")
        assert service.cache_format.get() == "JPEG"

    def test_set_png(self, service: SettingsService) -> None:
        """Setting 'PNG' after 'JPEG' persists the value."""
        service.cache_format.set("JPEG")
        service.cache_format.set("PNG")
        assert service.cache_format.get() == "PNG"

    def test_invalid_format_raises(self, service: SettingsService) -> None:
        """Setting an unsupported format raises ValueError."""
        with pytest.raises(ValueError, match="Invalid cache_format"):
            service.cache_format.set("bmp")

    def test_invalid_lowercase_raises(self, service: SettingsService) -> None:
        """Lowercase format names are invalid (must be uppercase)."""
        with pytest.raises(ValueError, match="Invalid cache_format"):
            service.cache_format.set("png")

    def test_valid_formats(self, service: SettingsService) -> None:
        """get_valid_formats() returns the supported formats."""
        valid = service.cache_format.get_valid_formats()
        assert "PNG" in valid
        assert "JPEG" in valid

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'cache_format'."""
        assert service.cache_format.get_key() == "cache_format"


class TestCacheDir:
    """Tests for the cache_dir setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """cache_dir defaults to None."""
        assert service.cache_dir.get() is None

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a cache directory persists and reads back the path."""
        service.cache_dir.set("/tmp/cache")
        assert service.cache_dir.get() == "/tmp/cache"

    def test_set_none(self, service: SettingsService) -> None:
        """Setting None clears the stored cache directory."""
        service.cache_dir.set("/tmp/cache")
        service.cache_dir.set(None)
        assert service.cache_dir.get() is None

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'cache_dir'."""
        assert service.cache_dir.get_key() == "cache_dir"


class TestDebugMode:
    """Tests for the debug_mode setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """debug_mode defaults to False."""
        assert service.debug_mode.get() is False

    def test_set_true(self, service: SettingsService) -> None:
        """Setting True enables debug mode."""
        service.debug_mode.set(True)
        assert service.debug_mode.get() is True

    def test_set_false(self, service: SettingsService) -> None:
        """Setting False after True disables debug mode."""
        service.debug_mode.set(True)
        service.debug_mode.set(False)
        assert service.debug_mode.get() is False

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'debug_mode'."""
        assert service.debug_mode.get_key() == "debug_mode"


class TestWindowLayoutState:
    """Tests for the window_layout_state setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """window_layout_state defaults to None."""
        assert service.window_layout_state.get() is None

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a JSON layout state persists the value."""
        state = '{"x": 100, "y": 200}'
        service.window_layout_state.set(state)
        assert service.window_layout_state.get() == state

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'window_layout_state'."""
        assert service.window_layout_state.get_key() == "window_layout_state"


class TestWindowGeometryState:
    """Tests for the window_geometry_state setting."""

    def test_default_value(self, service: SettingsService) -> None:
        """window_geometry_state defaults to None."""
        assert service.window_geometry_state.get() is None

    def test_set_and_get(self, service: SettingsService) -> None:
        """Setting a JSON geometry state persists the value."""
        state = '{"width": 800, "height": 600}'
        service.window_geometry_state.set(state)
        assert service.window_geometry_state.get() == state

    def test_setting_key(self, service: SettingsService) -> None:
        """get_key() returns 'window_geometry_state'."""
        assert service.window_geometry_state.get_key() == "window_geometry_state"


class TestPersistence:
    """Settings persist across SettingsService instances sharing a database."""

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
