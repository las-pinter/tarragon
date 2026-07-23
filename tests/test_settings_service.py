"""Tests for src/tarragon/services/settings_service.py — typed accessors and validation."""

from pathlib import Path

import pytest
from tarragon.db.database import Database
from tarragon.services.settings import Settings
from tarragon.services.settings_service import SettingsService

# ── Fixtures ──────────────────────────────────────────────────────────────────


@pytest.fixture()
def service() -> SettingsService:
    """SettingsService backed by an in-memory SQLite database with defaults."""
    database = Database(Path(":memory:"))
    database.init_schema()
    settings = Settings(database)
    settings.init_defaults()
    return SettingsService(settings)


# ── max_psd_workers ───────────────────────────────────────────────────────────


class TestMaxPsdWorkers:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_max_psd_workers() == 3

    def test_set_and_get(self, service: SettingsService) -> None:
        service.set_max_psd_workers(5)
        assert service.get_max_psd_workers() == 5

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.set_max_psd_workers(99)
        assert service.get_max_psd_workers() == 8

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.set_max_psd_workers(0)
        assert service.get_max_psd_workers() == 1

    def test_clamps_negative(self, service: SettingsService) -> None:
        service.set_max_psd_workers(-5)
        assert service.get_max_psd_workers() == 1

    def test_boundary_low(self, service: SettingsService) -> None:
        service.set_max_psd_workers(1)
        assert service.get_max_psd_workers() == 1

    def test_boundary_high(self, service: SettingsService) -> None:
        service.set_max_psd_workers(8)
        assert service.get_max_psd_workers() == 8


# ── max_multi_preview ─────────────────────────────────────────────────────────


class TestMaxMultiPreview:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_max_multi_preview() == 9

    def test_set_and_get(self, service: SettingsService) -> None:
        service.set_max_multi_preview(25)
        assert service.get_max_multi_preview() == 25

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.set_max_multi_preview(200)
        assert service.get_max_multi_preview() == 100

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.set_max_multi_preview(0)
        assert service.get_max_multi_preview() == 1


# ── large_canvas_threshold_mp ─────────────────────────────────────────────────


class TestLargeCanvasThresholdMp:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_large_canvas_threshold_mp() == 20.0

    def test_set_and_get(self, service: SettingsService) -> None:
        service.set_large_canvas_threshold_mp(50.5)
        assert service.get_large_canvas_threshold_mp() == 50.5

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.set_large_canvas_threshold_mp(9999.0)
        assert service.get_large_canvas_threshold_mp() == 1000.0

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.set_large_canvas_threshold_mp(0.0)
        assert service.get_large_canvas_threshold_mp() == 0.1


# ── tile_grid_size ────────────────────────────────────────────────────────────


class TestTileGridSize:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_tile_grid_size() == "2x2"

    def test_set_and_get(self, service: SettingsService) -> None:
        service.set_tile_grid_size("3x3")
        assert service.get_tile_grid_size() == "3x3"

    def test_set_asymmetric(self, service: SettingsService) -> None:
        service.set_tile_grid_size("4x2")
        assert service.get_tile_grid_size() == "4x2"

    def test_invalid_format_raises(self, service: SettingsService) -> None:
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.set_tile_grid_size("abc")

    def test_invalid_empty_raises(self, service: SettingsService) -> None:
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.set_tile_grid_size("")

    def test_invalid_single_number_raises(self, service: SettingsService) -> None:
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.set_tile_grid_size("4")

    def test_get_returns_fallback_for_corrupt_value(self, service: SettingsService) -> None:
        """If the underlying store has a corrupt value, getter returns safe default."""
        service._settings.set("tile_grid_size", "garbage")
        assert service.get_tile_grid_size() == "2x2"


# ── color_tag_enabled ─────────────────────────────────────────────────────────


class TestColorTagEnabled:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_color_tag_enabled() is True

    def test_set_false(self, service: SettingsService) -> None:
        service.set_color_tag_enabled(False)
        assert service.get_color_tag_enabled() is False

    def test_set_true(self, service: SettingsService) -> None:
        service.set_color_tag_enabled(False)
        service.set_color_tag_enabled(True)
        assert service.get_color_tag_enabled() is True


# ── color_tag_palette_size ────────────────────────────────────────────────────


class TestColorTagPaletteSize:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_color_tag_palette_size() == 8

    def test_set_and_get(self, service: SettingsService) -> None:
        service.set_color_tag_palette_size(16)
        assert service.get_color_tag_palette_size() == 16

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.set_color_tag_palette_size(100)
        assert service.get_color_tag_palette_size() == 32

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.set_color_tag_palette_size(0)
        assert service.get_color_tag_palette_size() == 2


# ── color_tag_min_share ──────────────────────────────────────────────────────


class TestColorTagMinShare:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_color_tag_min_share() == pytest.approx(0.10)

    def test_set_and_get(self, service: SettingsService) -> None:
        service.set_color_tag_min_share(0.25)
        assert service.get_color_tag_min_share() == pytest.approx(0.25)

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.set_color_tag_min_share(1.5)
        assert service.get_color_tag_min_share() == pytest.approx(1.0)

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.set_color_tag_min_share(-0.5)
        assert service.get_color_tag_min_share() == pytest.approx(0.0)

    def test_boundary_zero(self, service: SettingsService) -> None:
        service.set_color_tag_min_share(0.0)
        assert service.get_color_tag_min_share() == pytest.approx(0.0)

    def test_boundary_one(self, service: SettingsService) -> None:
        service.set_color_tag_min_share(1.0)
        assert service.get_color_tag_min_share() == pytest.approx(1.0)


# ── color_tag_neutral_s_threshold ─────────────────────────────────────────────


class TestColorTagNeutralSThreshold:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_color_tag_neutral_s_threshold() == pytest.approx(0.15)

    def test_set_and_get(self, service: SettingsService) -> None:
        service.set_color_tag_neutral_s_threshold(0.30)
        assert service.get_color_tag_neutral_s_threshold() == pytest.approx(0.30)

    def test_clamps_above_max(self, service: SettingsService) -> None:
        service.set_color_tag_neutral_s_threshold(2.0)
        assert service.get_color_tag_neutral_s_threshold() == pytest.approx(1.0)

    def test_clamps_below_min(self, service: SettingsService) -> None:
        service.set_color_tag_neutral_s_threshold(-1.0)
        assert service.get_color_tag_neutral_s_threshold() == pytest.approx(0.0)


# ── cache_format ──────────────────────────────────────────────────────────────


class TestCacheFormat:
    def test_default_value(self, service: SettingsService) -> None:
        assert service.get_cache_format() == "png"

    def test_set_jpeg(self, service: SettingsService) -> None:
        service.set_cache_format("jpeg")
        assert service.get_cache_format() == "jpeg"

    def test_set_png(self, service: SettingsService) -> None:
        service.set_cache_format("jpeg")
        service.set_cache_format("png")
        assert service.get_cache_format() == "png"

    def test_invalid_format_raises(self, service: SettingsService) -> None:
        with pytest.raises(ValueError, match="Invalid cache_format"):
            service.set_cache_format("bmp")

    def test_get_returns_fallback_for_corrupt_value(self, service: SettingsService) -> None:
        """If the underlying store has a corrupt value, getter returns safe default."""
        service._settings.set("cache_format", "tiff")
        assert service.get_cache_format() == "png"


# ── Type coercion ─────────────────────────────────────────────────────────────


class TestTypeCoercion:
    """Verify that getters coerce values from the underlying store."""

    def test_int_coercion_from_string(self, service: SettingsService) -> None:
        """If the DB stores a numeric string, int getter still works."""
        service._settings.set("max_psd_workers", "5")
        assert service.get_max_psd_workers() == 5

    def test_float_coercion_from_int(self, service: SettingsService) -> None:
        """If the DB stores an int where float is expected, coercion works."""
        service._settings.set("color_tag_min_share", 1)
        assert service.get_color_tag_min_share() == pytest.approx(1.0)
