"""Tests for SettingsService"""

from __future__ import annotations

import json
import logging
from collections.abc import Callable, Generator
from pathlib import Path
from typing import Any

import pytest

from tarragon.db.database import Database
from tarragon.services.settings_service import _SPECS, Setting, SettingsService, _SettingSpec


@pytest.fixture
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database for each test (isolated)."""
    database = Database(Path(":memory:"))
    database.init_schema()
    yield database
    database.close()


@pytest.fixture
def service(db: Database) -> SettingsService:
    """SettingsService backed by an in-memory database."""
    return SettingsService(db)


def _make_setting(
    db: Database,
    *,
    key: str = "test_key",
    default: object = None,
    min: float | None = None,
    max: float | None = None,
    valid: tuple[str, ...] | None = None,
    validator: Callable[[object], bool] | None = None,
) -> Setting[Any]:
    """Construct a Setting from an inline spec (mirrors SettingsService wiring)."""
    return Setting.from_spec(
        db, _SettingSpec(key=key, default=default, min=min, max=max, valid=valid, validator=validator)
    )


def _service_setting(db: Database, key: str) -> Setting[Any]:
    """Construct a Setting using the exact spec the service wires up."""
    return Setting.from_spec(db, _SPECS[key])


class TestSettingBase:
    """Tests for the generic Setting class behavior."""

    def test_get_returns_default_when_db_empty(self, db: Database) -> None:
        """get() returns the default value when no value is stored in DB."""
        setting = _make_setting(db, default="default_val")
        assert setting.get() == "default_val"

    def test_get_key_returns_key(self, db: Database) -> None:
        """get_key() returns the key string passed at construction."""
        setting = _make_setting(db, key="my_key", default=42)
        assert setting.get_key() == "my_key"

    def test_set_persists_to_db(self, db: Database) -> None:
        """set() JSON-serializes and stores the value in the database."""
        setting = _make_setting(db, default="default")
        setting.set("new_value")
        # Verify raw DB contains JSON-serialized value
        raw = db.get_setting("test_key")
        assert raw is not None
        assert json.loads(raw) == "new_value"

    def test_get_reads_from_db(self, db: Database) -> None:
        """get() reads and deserializes JSON from the database."""
        db.set_setting("preloaded", json.dumps(99))
        setting = _make_setting(db, key="preloaded", default="default")
        assert setting.get() == 99

    def test_set_then_get_roundtrip(self, db: Database) -> None:
        """set() followed by get() returns the same value."""
        setting = _make_setting(db, default=None)
        setting.set(42)
        assert setting.get() == 42

    def test_persistence_across_instances(self, db: Database) -> None:
        """A new Setting instance pointing to the same DB reads the stored value."""
        setting1 = _make_setting(db, key="shared_key", default="default")
        setting1.set("persisted_value")

        setting2 = _make_setting(db, key="shared_key", default="default")
        assert setting2.get() == "persisted_value"

    def test_validate_returns_true_by_default(self, db: Database) -> None:
        """_validate() accepts any value without a valid-list or validator."""
        setting = _make_setting(db)
        assert setting._validate("anything") is True
        assert setting._validate(42) is True
        assert setting._validate(None) is True

    def test_clamp_no_bounds(self, db: Database) -> None:
        """_clamp() with no min/max returns value unchanged."""
        setting = _make_setting(db)
        assert setting._clamp(999) == 999
        assert setting._clamp(-500) == -500

    def test_clamp_min_only(self, db: Database) -> None:
        """_clamp() with min only enforces lower bound."""
        setting = _make_setting(db, min=10)
        assert setting._clamp(5) == 10
        assert setting._clamp(15) == 15

    def test_clamp_max_only(self, db: Database) -> None:
        """_clamp() with max only enforces upper bound."""
        setting = _make_setting(db, max=100)
        assert setting._clamp(200) == 100
        assert setting._clamp(50) == 50

    def test_clamp_both_bounds(self, db: Database) -> None:
        """_clamp() with min and max enforces both bounds."""
        setting = _make_setting(db, min=1, max=10)
        assert setting._clamp(0) == 1
        assert setting._clamp(5) == 5
        assert setting._clamp(11) == 10

    def test_set_invalid_value_does_not_persist(self, db: Database) -> None:
        """When validation fails, set() raises and does not write to DB."""
        setting = _make_setting(db, default="original", valid=("ok",))
        with pytest.raises(ValueError):
            setting.set("bad_value")
        assert db.get_setting("test_key") is None

    @pytest.mark.parametrize(
        ("value", "expected_type"),
        [
            (True, bool),
            (False, bool),
            (42, int),
            (-7, int),
            (3.14, float),
            (0.0, float),
            ("hello", str),
            ("", str),
            (None, type(None)),
        ],
    )
    def test_json_type_roundtrip(self, db: Database, value: Any, expected_type: type) -> None:
        """All Python primitive types survive JSON serialization roundtrip."""
        setting = _make_setting(db, key=f"_type_test_{id(value)}", default=None)
        setting.set(value)
        result = setting.get()
        assert isinstance(result, expected_type)
        assert result == value

    def test_reload_rereads_changed_db_value(self, db: Database) -> None:
        """reload() re-reads a value changed directly in the DB."""
        setting = _make_setting(db, key="reload_key", default=1)
        assert setting.get() == 1
        db.set_setting("reload_key", json.dumps(7))
        assert setting.get() == 1
        setting.reload()
        assert setting.get() == 7


class TestSettingCacheDir:
    """Tests for the cache_dir Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """cache_dir defaults to None when no value is stored."""
        setting = _service_setting(db, "cache_dir")
        assert setting.get() is None

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'cache_dir'."""
        setting = _service_setting(db, "cache_dir")
        assert setting.get_key() == "cache_dir"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a cache directory persists and reads back the path."""
        setting = _service_setting(db, "cache_dir")
        setting.set("/tmp/cache")
        assert setting.get() == "/tmp/cache"

    def test_set_none(self, db: Database) -> None:
        """Setting None clears the stored cache directory."""
        setting = _service_setting(db, "cache_dir")
        setting.set("/tmp/cache")
        setting.set(None)
        assert setting.get() is None


class TestSettingCacheFormat:
    """Tests for the cache_format Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """cache_format defaults to 'PNG'."""
        setting = _service_setting(db, "cache_format")
        assert setting.get() == "PNG"

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'cache_format'."""
        setting = _service_setting(db, "cache_format")
        assert setting.get_key() == "cache_format"

    def test_set_png(self, db: Database) -> None:
        """Setting 'PNG' is accepted and stored."""
        setting = _service_setting(db, "cache_format")
        setting.set("PNG")
        assert setting.get() == "PNG"

    def test_set_jpeg(self, db: Database) -> None:
        """Setting 'JPEG' is accepted and stored."""
        setting = _service_setting(db, "cache_format")
        setting.set("JPEG")
        assert setting.get() == "JPEG"

    def test_invalid_format_raises(self, db: Database) -> None:
        """Setting an unsupported format raises ValueError."""
        setting = _service_setting(db, "cache_format")
        with pytest.raises(ValueError, match="Invalid cache_format"):
            setting.set("BMP")

    def test_lowercase_invalid(self, db: Database) -> None:
        """Validation is case-sensitive; lowercase is rejected."""
        setting = _service_setting(db, "cache_format")
        with pytest.raises(ValueError, match="Invalid cache_format"):
            setting.set("png")

    def test_get_valid_formats(self, db: Database) -> None:
        """get_valid_formats() returns the supported formats."""
        setting = _service_setting(db, "cache_format")
        assert setting.get_valid_formats() == ["PNG", "JPEG"]

    def test_invalid_value_not_persisted(self, db: Database) -> None:
        """Failed validation does not write to DB."""
        setting = _service_setting(db, "cache_format")
        with pytest.raises(ValueError):
            setting.set("GIF")
        assert db.get_setting("cache_format") is None


class TestSettingColorTagEnabled:
    """Tests for the color_tag_enabled Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """color_tag_enabled defaults to True."""
        setting = _service_setting(db, "color_tag_enabled")
        assert setting.get() is True

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'color_tag_enabled'."""
        setting = _service_setting(db, "color_tag_enabled")
        assert setting.get_key() == "color_tag_enabled"

    def test_set_false(self, db: Database) -> None:
        """Setting False disables color tagging."""
        setting = _service_setting(db, "color_tag_enabled")
        setting.set(False)
        assert setting.get() is False

    def test_set_true_after_false(self, db: Database) -> None:
        """Setting True after False re-enables color tagging."""
        setting = _service_setting(db, "color_tag_enabled")
        setting.set(False)
        setting.set(True)
        assert setting.get() is True


class TestSettingClearFullResOnExit:
    """Tests for the clear_full_res_on_exit Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """clear_full_res_on_exit defaults to True."""
        setting = _service_setting(db, "clear_full_res_on_exit")
        assert setting.get() is True

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'clear_full_res_on_exit'."""
        setting = _service_setting(db, "clear_full_res_on_exit")
        assert setting.get_key() == "clear_full_res_on_exit"

    def test_set_false(self, db: Database) -> None:
        """Setting False disables full-res cache cleanup on exit."""
        setting = _service_setting(db, "clear_full_res_on_exit")
        setting.set(False)
        assert setting.get() is False

    def test_set_true_after_false(self, db: Database) -> None:
        """Setting True after False re-enables full-res cache cleanup."""
        setting = _service_setting(db, "clear_full_res_on_exit")
        setting.set(False)
        setting.set(True)
        assert setting.get() is True


class TestSettingColorTagPaletteSize:
    """Tests for the color_tag_palette_size Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """color_tag_palette_size defaults to 8."""
        setting = _service_setting(db, "color_tag_palette_size")
        assert setting.get() == 8

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'color_tag_palette_size'."""
        setting = _service_setting(db, "color_tag_palette_size")
        assert setting.get_key() == "color_tag_palette_size"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a palette size within bounds persists the value."""
        setting = _service_setting(db, "color_tag_palette_size")
        setting.set(16)
        assert setting.get() == 16

    def test_clamps_above_max(self, db: Database) -> None:
        """Values above the maximum are clamped to 32."""
        setting = _service_setting(db, "color_tag_palette_size")
        setting.set(100)
        assert setting.get() == 32

    def test_clamps_below_min(self, db: Database) -> None:
        """Values below the minimum are clamped to 2."""
        setting = _service_setting(db, "color_tag_palette_size")
        setting.set(0)
        assert setting.get() == 2

    def test_boundary_low(self, db: Database) -> None:
        """The minimum boundary value 2 is accepted unchanged."""
        setting = _service_setting(db, "color_tag_palette_size")
        setting.set(2)
        assert setting.get() == 2

    def test_boundary_high(self, db: Database) -> None:
        """The maximum boundary value 32 is accepted unchanged."""
        setting = _service_setting(db, "color_tag_palette_size")
        setting.set(32)
        assert setting.get() == 32

    def test_read_clamps_out_of_range_stored_value(self, db: Database) -> None:
        """An out-of-range stored value is clamped to 32 on first load."""
        db.set_setting("color_tag_palette_size", json.dumps(99))
        setting = _service_setting(db, "color_tag_palette_size")
        assert setting.get() == 32

    def test_reload_reapplies_clamp_to_db_changes(self, db: Database) -> None:
        """reload() re-reads and clamps a value changed directly in the DB."""
        setting = _service_setting(db, "color_tag_palette_size")
        assert setting.get() == 8
        db.set_setting("color_tag_palette_size", json.dumps(99))
        setting.reload()
        assert setting.get() == 32

    def test_wrong_typed_stored_value_falls_back_to_default_with_warning(
        self, db: Database, caplog: pytest.LogCaptureFixture
    ) -> None:
        """A wrong-typed stored value falls back to 8 on first load and warns."""
        db.set_setting("color_tag_palette_size", json.dumps("abc"))
        setting = _service_setting(db, "color_tag_palette_size")
        with caplog.at_level(logging.WARNING, logger="tarragon.services.settings_service"):
            assert setting.get() == 8
        warning_messages = [
            record.message
            for record in caplog.records
            if record.name == "tarragon.services.settings_service" and record.levelname == "WARNING"
        ]
        assert any(
            "color_tag_palette_size" in message and "falling back to default" in message for message in warning_messages
        )

    def test_set_wrong_type_raises_value_error(self, db: Database) -> None:
        """set() with a wrong-typed value raises ValueError, not TypeError."""
        setting = _service_setting(db, "color_tag_palette_size")
        with pytest.raises(ValueError, match="Expected type"):
            setting.set("abc")


class TestSettingColorTagMinShare:
    """Tests for the color_tag_min_share Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """color_tag_min_share defaults to 0.10."""
        setting = _service_setting(db, "color_tag_min_share")
        assert setting.get() == pytest.approx(0.10)

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'color_tag_min_share'."""
        setting = _service_setting(db, "color_tag_min_share")
        assert setting.get_key() == "color_tag_min_share"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a share within bounds persists the value."""
        setting = _service_setting(db, "color_tag_min_share")
        setting.set(0.25)
        assert setting.get() == pytest.approx(0.25)

    def test_clamps_above_max(self, db: Database) -> None:
        """Values above the maximum are clamped to 1.0."""
        setting = _service_setting(db, "color_tag_min_share")
        setting.set(1.5)
        assert setting.get() == pytest.approx(1.0)

    def test_clamps_below_min(self, db: Database) -> None:
        """Values below the minimum are clamped to 0.0."""
        setting = _service_setting(db, "color_tag_min_share")
        setting.set(-0.5)
        assert setting.get() == pytest.approx(0.0)

    def test_boundary_zero(self, db: Database) -> None:
        """The minimum boundary value 0.0 is accepted unchanged."""
        setting = _service_setting(db, "color_tag_min_share")
        setting.set(0.0)
        assert setting.get() == pytest.approx(0.0)

    def test_boundary_one(self, db: Database) -> None:
        """The maximum boundary value 1.0 is accepted unchanged."""
        setting = _service_setting(db, "color_tag_min_share")
        setting.set(1.0)
        assert setting.get() == pytest.approx(1.0)


class TestSettingColorTagNeutralSThreshold:
    """Tests for the color_tag_neutral_s_threshold Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """color_tag_neutral_s_threshold defaults to 0.15."""
        setting = _service_setting(db, "color_tag_neutral_s_threshold")
        assert setting.get() == pytest.approx(0.15)

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'color_tag_neutral_s_threshold'."""
        setting = _service_setting(db, "color_tag_neutral_s_threshold")
        assert setting.get_key() == "color_tag_neutral_s_threshold"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a threshold within bounds persists the value."""
        setting = _service_setting(db, "color_tag_neutral_s_threshold")
        setting.set(0.30)
        assert setting.get() == pytest.approx(0.30)

    def test_clamps_above_max(self, db: Database) -> None:
        """Values above the maximum are clamped to 1.0."""
        setting = _service_setting(db, "color_tag_neutral_s_threshold")
        setting.set(2.0)
        assert setting.get() == pytest.approx(1.0)

    def test_clamps_below_min(self, db: Database) -> None:
        """Values below the minimum are clamped to 0.0."""
        setting = _service_setting(db, "color_tag_neutral_s_threshold")
        setting.set(-1.0)
        assert setting.get() == pytest.approx(0.0)


class TestSettingDebugMode:
    """Tests for the debug_mode Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """debug_mode defaults to False."""
        setting = _service_setting(db, "debug_mode")
        assert setting.get() is False

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'debug_mode'."""
        setting = _service_setting(db, "debug_mode")
        assert setting.get_key() == "debug_mode"

    def test_set_true(self, db: Database) -> None:
        """Setting True enables debug mode."""
        setting = _service_setting(db, "debug_mode")
        setting.set(True)
        assert setting.get() is True

    def test_set_false_after_true(self, db: Database) -> None:
        """Setting False after True disables debug mode."""
        setting = _service_setting(db, "debug_mode")
        setting.set(True)
        setting.set(False)
        assert setting.get() is False

    def test_malformed_stored_json_falls_back_to_default_with_warning(
        self, db: Database, caplog: pytest.LogCaptureFixture
    ) -> None:
        """A malformed JSON stored value falls back to False on first load and warns."""
        db.set_setting("debug_mode", "{not-json")
        setting = _service_setting(db, "debug_mode")
        with caplog.at_level(logging.WARNING, logger="tarragon.services.settings_service"):
            assert setting.get() is False
        warning_messages = [
            record.message
            for record in caplog.records
            if record.name == "tarragon.services.settings_service" and record.levelname == "WARNING"
        ]
        assert any("debug_mode" in message and "falling back to default" in message for message in warning_messages)

    def test_set_rejects_int_and_does_not_persist(self, db: Database) -> None:
        """set() rejects int 1 and 0 for debug_mode and does not persist."""
        setting = _service_setting(db, "debug_mode")
        with pytest.raises(ValueError):
            setting.set(1)
        with pytest.raises(ValueError):
            setting.set(0)
        assert db.get_setting("debug_mode") is None
        assert setting.get() is False


class TestSettingLargeCanvasThresholdMp:
    """Tests for the large_canvas_threshold_mp Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """large_canvas_threshold_mp defaults to 20.0."""
        setting = _service_setting(db, "large_canvas_threshold_mp")
        assert setting.get() == pytest.approx(20.0)

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'large_canvas_threshold_mp'."""
        setting = _service_setting(db, "large_canvas_threshold_mp")
        assert setting.get_key() == "large_canvas_threshold_mp"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a threshold within bounds persists the value."""
        setting = _service_setting(db, "large_canvas_threshold_mp")
        setting.set(50.5)
        assert setting.get() == pytest.approx(50.5)

    def test_clamps_above_max(self, db: Database) -> None:
        """Values above the maximum are clamped to 1000.0."""
        setting = _service_setting(db, "large_canvas_threshold_mp")
        setting.set(9999.0)
        assert setting.get() == pytest.approx(1000.0)

    def test_clamps_below_min(self, db: Database) -> None:
        """Values below the minimum are clamped to 0.1."""
        setting = _service_setting(db, "large_canvas_threshold_mp")
        setting.set(0.0)
        assert setting.get() == pytest.approx(0.1)

    def test_boundary_low(self, db: Database) -> None:
        """The minimum boundary value 0.1 is accepted unchanged."""
        setting = _service_setting(db, "large_canvas_threshold_mp")
        setting.set(0.1)
        assert setting.get() == pytest.approx(0.1)

    def test_boundary_high(self, db: Database) -> None:
        """The maximum boundary value 1000.0 is accepted unchanged."""
        setting = _service_setting(db, "large_canvas_threshold_mp")
        setting.set(1000.0)
        assert setting.get() == pytest.approx(1000.0)


class TestSettingMaxMultiPreview:
    """Tests for the max_multi_preview Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """max_multi_preview defaults to 9."""
        setting = _service_setting(db, "max_multi_preview")
        assert setting.get() == 9

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'max_multi_preview'."""
        setting = _service_setting(db, "max_multi_preview")
        assert setting.get_key() == "max_multi_preview"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a preview count within bounds persists the value."""
        setting = _service_setting(db, "max_multi_preview")
        setting.set(25)
        assert setting.get() == 25

    def test_clamps_above_max(self, db: Database) -> None:
        """Values above the maximum are clamped to 100."""
        setting = _service_setting(db, "max_multi_preview")
        setting.set(200)
        assert setting.get() == 100

    def test_clamps_below_min(self, db: Database) -> None:
        """Values below the minimum are clamped to 1."""
        setting = _service_setting(db, "max_multi_preview")
        setting.set(0)
        assert setting.get() == 1

    def test_boundary_low(self, db: Database) -> None:
        """The minimum boundary value 1 is accepted unchanged."""
        setting = _service_setting(db, "max_multi_preview")
        setting.set(1)
        assert setting.get() == 1

    def test_boundary_high(self, db: Database) -> None:
        """The maximum boundary value 100 is accepted unchanged."""
        setting = _service_setting(db, "max_multi_preview")
        setting.set(100)
        assert setting.get() == 100


class TestSettingMaxPsdWorkers:
    """Tests for the max_psd_workers Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """max_psd_workers defaults to 3."""
        setting = _service_setting(db, "max_psd_workers")
        assert setting.get() == 3

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'max_psd_workers'."""
        setting = _service_setting(db, "max_psd_workers")
        assert setting.get_key() == "max_psd_workers"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a worker count within bounds persists the value."""
        setting = _service_setting(db, "max_psd_workers")
        setting.set(5)
        assert setting.get() == 5

    def test_clamps_above_max(self, db: Database) -> None:
        """Values above the maximum are clamped to 8."""
        setting = _service_setting(db, "max_psd_workers")
        setting.set(99)
        assert setting.get() == 8

    def test_clamps_below_min(self, db: Database) -> None:
        """Values below the minimum are clamped to 1."""
        setting = _service_setting(db, "max_psd_workers")
        setting.set(0)
        assert setting.get() == 1

    def test_boundary_low(self, db: Database) -> None:
        """The minimum boundary value 1 is accepted unchanged."""
        setting = _service_setting(db, "max_psd_workers")
        setting.set(1)
        assert setting.get() == 1

    def test_boundary_high(self, db: Database) -> None:
        """The maximum boundary value 8 is accepted unchanged."""
        setting = _service_setting(db, "max_psd_workers")
        setting.set(8)
        assert setting.get() == 8

    def test_stored_bool_falls_back_to_default_with_warning(
        self, db: Database, caplog: pytest.LogCaptureFixture
    ) -> None:
        """A stored bool in max_psd_workers falls back to 3 on first load and warns."""
        db.set_setting("max_psd_workers", json.dumps(True))
        setting = _service_setting(db, "max_psd_workers")
        with caplog.at_level(logging.WARNING, logger="tarragon.services.settings_service"):
            assert setting.get() == 3
        warning_messages = [
            record.message
            for record in caplog.records
            if record.name == "tarragon.services.settings_service" and record.levelname == "WARNING"
        ]
        assert any(
            "max_psd_workers" in message and "falling back to default" in message for message in warning_messages
        )

    def test_stored_float_falls_back_to_int_default_with_warning(
        self, db: Database, caplog: pytest.LogCaptureFixture
    ) -> None:
        """A stored float in max_psd_workers falls back to the int default 3 and warns."""
        db.set_setting("max_psd_workers", json.dumps(3.5))
        setting = _service_setting(db, "max_psd_workers")
        with caplog.at_level(logging.WARNING, logger="tarragon.services.settings_service"):
            value = setting.get()
        assert value == 3
        assert isinstance(value, int)
        warning_messages = [
            record.message
            for record in caplog.records
            if record.name == "tarragon.services.settings_service" and record.levelname == "WARNING"
        ]
        assert any(
            "max_psd_workers" in message and "falling back to default" in message for message in warning_messages
        )

    def test_set_rejects_float_and_string_and_does_not_persist(self, db: Database) -> None:
        """set() rejects non-int values for max_psd_workers and does not persist."""
        setting = _service_setting(db, "max_psd_workers")
        with pytest.raises(ValueError):
            setting.set(4.7)
        with pytest.raises(ValueError):
            setting.set("abc")
        assert db.get_setting("max_psd_workers") is None
        assert setting.get() == 3


class TestSettingTileGridSize:
    """Tests for the tile_grid_size Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """tile_grid_size defaults to '3x3'."""
        setting = _service_setting(db, "tile_grid_size")
        assert setting.get() == "3x3"

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'tile_grid_size'."""
        setting = _service_setting(db, "tile_grid_size")
        assert setting.get_key() == "tile_grid_size"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a valid grid size persists the value."""
        setting = _service_setting(db, "tile_grid_size")
        setting.set("3x3")
        assert setting.get() == "3x3"

    def test_set_asymmetric_rejected(self, db: Database) -> None:
        """Asymmetric grids like '4x2' are rejected (valid-list is authoritative)."""
        setting = _service_setting(db, "tile_grid_size")
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            setting.set("4x2")

    def test_invalid_format_raises(self, db: Database) -> None:
        """Setting a non-grid string raises ValueError."""
        setting = _service_setting(db, "tile_grid_size")
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            setting.set("abc")

    def test_invalid_empty_raises(self, db: Database) -> None:
        """Setting an empty string raises ValueError."""
        setting = _service_setting(db, "tile_grid_size")
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            setting.set("")

    def test_invalid_single_number_raises(self, db: Database) -> None:
        """Setting a single number raises ValueError."""
        setting = _service_setting(db, "tile_grid_size")
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            setting.set("4")

    def test_invalid_value_not_persisted(self, db: Database) -> None:
        """Failed validation does not write to DB."""
        setting = _service_setting(db, "tile_grid_size")
        with pytest.raises(ValueError):
            setting.set("garbage")
        assert db.get_setting("tile_grid_size") is None

    def test_get_valid_formats(self, db: Database) -> None:
        """get_valid_formats() returns all supported grid sizes."""
        setting = _service_setting(db, "tile_grid_size")
        assert setting.get_valid_formats() == ["1x1", "2x2", "3x3", "4x4"]

    def test_out_of_range_grid_rejected_and_not_persisted(self, db: Database) -> None:
        """Out-of-range grids such as '99x99' raise and leave the DB unchanged."""
        setting = _service_setting(db, "tile_grid_size")
        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            setting.set("99x99")
        assert db.get_setting("tile_grid_size") is None

    @pytest.mark.parametrize("grid_size", ["1x1", "2x2", "3x3", "4x4"])
    def test_valid_grid_size_accepted_and_persisted(self, db: Database, grid_size: str) -> None:
        """Each supported grid size is accepted and persisted."""
        setting = _service_setting(db, "tile_grid_size")
        setting.set(grid_size)
        assert setting.get() == grid_size
        assert json.loads(db.get_setting("tile_grid_size")) == grid_size

    def test_invalid_stored_grid_falls_back_to_default_with_warning(
        self, db: Database, caplog: pytest.LogCaptureFixture
    ) -> None:
        """An invalid stored grid falls back to '3x3' on first load and warns."""
        db.set_setting("tile_grid_size", json.dumps("99x99"))
        setting = _service_setting(db, "tile_grid_size")
        with caplog.at_level(logging.WARNING, logger="tarragon.services.settings_service"):
            assert setting.get() == "3x3"
        warning_messages = [
            record.message
            for record in caplog.records
            if record.name == "tarragon.services.settings_service" and record.levelname == "WARNING"
        ]
        assert any("falling back to default" in message for message in warning_messages)


class TestSettingWindowLayoutState:
    """Tests for the window_layout_state Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """window_layout_state defaults to None."""
        setting = _service_setting(db, "window_layout_state")
        assert setting.get() is None

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'window_layout_state'."""
        setting = _service_setting(db, "window_layout_state")
        assert setting.get_key() == "window_layout_state"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a layout blob persists the value."""
        setting = _service_setting(db, "window_layout_state")
        setting.set("layout_blob_data")
        assert setting.get() == "layout_blob_data"


class TestSettingWindowGeometryState:
    """Tests for the window_geometry_state Setting entry."""

    def test_default_value(self, db: Database) -> None:
        """window_geometry_state defaults to None."""
        setting = _service_setting(db, "window_geometry_state")
        assert setting.get() is None

    def test_get_key(self, db: Database) -> None:
        """get_key() returns 'window_geometry_state'."""
        setting = _service_setting(db, "window_geometry_state")
        assert setting.get_key() == "window_geometry_state"

    def test_set_and_get(self, db: Database) -> None:
        """Setting a geometry blob persists the value."""
        setting = _service_setting(db, "window_geometry_state")
        setting.set("geometry_blob_data")
        assert setting.get() == "geometry_blob_data"


class TestSettingsServiceIntegration:
    """Verify SettingsService wires up all Setting entries correctly."""

    def test_all_settings_have_correct_defaults(self, service: SettingsService) -> None:
        """Every setting exposed by SettingsService returns its expected default."""
        assert service.cache_dir.get() is None
        assert service.cache_format.get() == "PNG"
        assert service.clear_full_res_on_exit.get() is True
        assert service.color_tag_enabled.get() is True
        assert service.color_tag_palette_size.get() == 8
        assert service.color_tag_min_share.get() == pytest.approx(0.10)
        assert service.color_tag_neutral_s_threshold.get() == pytest.approx(0.15)
        assert service.debug_mode.get() is False
        assert service.large_canvas_threshold_mp.get() == pytest.approx(20.0)
        assert service.max_multi_preview.get() == 9
        assert service.max_psd_workers.get() == 3
        assert service.tile_grid_size.get() == "3x3"
        assert service.window_layout_state.get() is None
        assert service.window_geometry_state.get() is None

    def test_set_via_service_persists_to_db(self, db: Database, service: SettingsService) -> None:
        """Setting a value via SettingsService persists to the underlying DB."""
        service.max_psd_workers.set(5)
        # Read raw from DB to confirm JSON persistence
        raw = db.get_setting("max_psd_workers")
        assert raw is not None
        assert json.loads(raw) == 5

    def test_read_via_fresh_service(self, db: Database) -> None:
        """A fresh SettingsService on the same DB reads previously stored values."""
        service1 = SettingsService(db)
        service1.max_psd_workers.set(7)
        service1.cache_format.set("JPEG")

        service2 = SettingsService(db)
        assert service2.max_psd_workers.get() == 7
        assert service2.cache_format.get() == "JPEG"

    def test_clamping_via_service(self, service: SettingsService) -> None:
        """Clamping works when setting values through the service."""
        service.max_psd_workers.set(99)
        assert service.max_psd_workers.get() == 8

        service.max_psd_workers.set(0)
        assert service.max_psd_workers.get() == 1

    def test_validation_via_service(self, service: SettingsService) -> None:
        """Validation works when setting values through the service."""
        with pytest.raises(ValueError, match="Invalid cache_format"):
            service.cache_format.set("TIFF")

        with pytest.raises(ValueError, match="Invalid tile_grid_size"):
            service.tile_grid_size.set("invalid")

    def test_get_min_max(self, service: SettingsService) -> None:
        """get_min() and get_max() return the configured bounds."""
        assert service.max_psd_workers.get_min() == 1
        assert service.max_psd_workers.get_max() == 8
        assert service.color_tag_palette_size.get_min() == 2
        assert service.color_tag_palette_size.get_max() == 32
        assert service.large_canvas_threshold_mp.get_min() == 0.1
        assert service.large_canvas_threshold_mp.get_max() == 1000.0

    def test_reload_refreshes_changed_numeric_and_string_settings(self, db: Database, service: SettingsService) -> None:
        """reload() re-reads numeric and string settings changed directly in the DB."""
        assert service.max_psd_workers.get() == 3
        assert service.cache_format.get() == "PNG"
        db.set_setting("max_psd_workers", json.dumps(6))
        db.set_setting("cache_format", json.dumps("JPEG"))
        service.reload()
        assert service.max_psd_workers.get() == 6
        assert service.cache_format.get() == "JPEG"


class TestSettingsServiceInit:
    """SettingsService initialization and attribute wiring."""

    def test_creates_all_settings(self, service: SettingsService) -> None:
        """SettingsService creates all expected setting attributes."""
        assert hasattr(service, "cache_dir")
        assert hasattr(service, "cache_format")
        assert hasattr(service, "clear_full_res_on_exit")
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
        """All setting attributes are Setting instances built from specs."""
        for name in _SPECS:
            assert isinstance(getattr(service, name), Setting)

    def test_all_settings_inherit_from_setting(self, service: SettingsService) -> None:
        """All settings inherit from the base Setting class."""
        assert isinstance(service.cache_dir, Setting)
        assert isinstance(service.cache_format, Setting)
        assert isinstance(service.max_psd_workers, Setting)


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
