"""Tests for src/tarragon/settings.py — persistence, defaults, serialization."""

import json
from pathlib import Path
from typing import Any

import pytest
from tarragon.db import Database
from tarragon.settings import DEFAULTS, Settings

# ── Fixtures ────────────────────────────────────────────────────


@pytest.fixture()
def settings_db(tmp_path: Path) -> Settings:
    """Settings backed by a Database instance using a temporary SQLite file."""
    db_file = tmp_path / "settings.db"
    database = Database(db_file)
    database.init_schema()
    return Settings(database)


# ── init_defaults ───────────────────────────────────────────────


class TestInitDefaults:
    def test_creates_all_default_keys(self, settings_db: Settings) -> None:
        """init_defaults() populates every key from DEFAULTS."""
        settings_db.init_defaults()
        for key in DEFAULTS:
            value = settings_db.get(key)
            assert value == DEFAULTS[key], f"Key '{key}' mismatch after init_defaults"

    def test_is_idempotent(self, settings_db: Settings) -> None:
        """Calling init_defaults() twice does not corrupt data."""
        settings_db.init_defaults()
        first_count = sum(1 for key in DEFAULTS if settings_db._db.get_setting(key) is not None)

        settings_db.init_defaults()
        second_count = sum(1 for key in DEFAULTS if settings_db._db.get_setting(key) is not None)

        assert first_count == second_count == len(DEFAULTS)


class TestGet:
    def test_returns_default_for_missing_key(self, settings_db: Settings) -> None:
        """Reading a key not in DB returns DEFAULTS value."""
        # No init_defaults — table is empty
        for key, expected in DEFAULTS.items():
            assert settings_db.get(key) == expected

    def test_raises_unknown_key_not_in_defaults(self, settings_db: Settings) -> None:
        """get() raises KeyError for keys absent from both DB and DEFAULTS."""
        with pytest.raises(KeyError, match="phantom_setting_xyz"):
            settings_db.get("phantom_setting_xyz")


class TestSetAndGetRoundTrip:
    @pytest.mark.parametrize(
        ("key", "value"),
        [
            ("max_multi_preview", 42),
            ("large_canvas_threshold_mp", 99.5),
            ("tile_grid_size", "3x3"),
            ("cache_format", "jpeg"),
            ("color_tag_enabled", False),
            ("color_tag_palette_size", 16),
        ],
    )
    def test_roundtrip_preserves_value(self, settings_db: Settings, key: str, value: Any) -> None:
        """set() then get() returns the same value across instances."""
        settings_db.set(key, value)

        # Re-read from a *fresh* Database+Settings to prove persistence
        fresh_db = Database(settings_db._db._db_path)
        fresh = Settings(fresh_db)
        assert fresh.get(key) == value
        fresh_db.close()


class TestTypeSerialization:
    """Verify JSON serialization handles all Python primitive types."""

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
        ],
    )
    def test_serializes_and_deserializes_types(self, settings_db: Settings, value: Any, expected_type: type) -> None:
        key = f"_type_test_{json.dumps(value)}"
        settings_db.set(key, value)
        result = settings_db.get(key)
        assert isinstance(result, expected_type), f"Expected {expected_type.__name__}, got {type(result).__name__}"
        assert result == value

    def test_serializes_nested_json(self, settings_db: Settings) -> None:
        """Complex nested types round-trip correctly."""
        complex_value = {"nested": [1, 2, 3], "flag": True}
        settings_db.set("_complex_key", complex_value)
        assert settings_db.get("_complex_key") == complex_value


class TestSchema:
    def test_all_defaults_keys_exist_in_schema(self, settings_db: Settings) -> None:
        """Every DEFAULTS key is present after init_defaults."""
        settings_db.init_defaults()
        db_keys = {key for key in DEFAULTS if settings_db._db.get_setting(key) is not None}
        assert db_keys == set(DEFAULTS.keys()), f"Missing: {set(DEFAULTS) - db_keys}"


class TestContextManager:
    def test_context_manager_returns_self(self, tmp_path: Path) -> None:  # type: ignore[reportAttributeAccessIssue]
        """Settings used as context manager returns itself and is usable inside the block."""
        db_file = tmp_path / "cm_test.db"
        database = Database(db_file)
        database.init_schema()
        with Settings(database) as s:
            # Should be usable inside the block
            s.set("test_key", 1)
            assert s.get("test_key") == 1
        # Connection lifecycle is owned by Database, not Settings.
        database.close()
