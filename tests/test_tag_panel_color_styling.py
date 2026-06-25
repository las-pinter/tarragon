"""Tests for TagPanel auto-color tag visual styling.

WAAAGH! Wrenchbasha's verifyin' dat auto-color tags look different from
manual tags in da tag panel!

Testing patterns applied (from python-testing-patterns skill):
  - Arrange-Act-Assert (AAA)
  - pytest fixtures for service and widget setup
  - Widget inspection for verifying visual state
  - Parametrized tests for different color buckets
"""

from __future__ import annotations

from pathlib import Path

import pytest
from PySide6.QtWidgets import QApplication, QLabel, QWidget
from tarragon.db import Database
from tarragon.services.tag_service import TagService
from tarragon.widgets.tag_panel import TagPanel

# =========================================================================
# Fixtures
# =========================================================================


@pytest.fixture(autouse=True)
def qapp():
    """Provide a shared QApplication instance for all Qt tests."""
    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


@pytest.fixture
def db() -> Database:
    """Create an in-memory Database with initialised schema."""
    database = Database(Path(":memory:"))
    database.init_schema()
    return database


@pytest.fixture
def service(db: Database) -> TagService:
    """Create a TagService backed by an in-memory database."""
    return TagService(db=db)


@pytest.fixture
def panel(service: TagService) -> TagPanel:
    """Create a TagPanel that is cleaned up after the test."""
    w = TagPanel(service)
    yield w
    w.close()


# =========================================================================
# Helpers
# =========================================================================

ALL_COLOR_BUCKETS = [
    "red",
    "orange",
    "yellow",
    "green",
    "teal",
    "cyan",
    "blue",
    "purple",
    "magenta",
    "neutral",
]

EXPECTED_HEX: dict[str, str] = {
    "red": "#E74C3C",
    "orange": "#F39C12",
    "yellow": "#F1C40F",
    "green": "#27AE60",
    "teal": "#1ABC9C",
    "cyan": "#00BCD4",
    "blue": "#3498DB",
    "purple": "#9B59B6",
    "magenta": "#E91E63",
    "neutral": "#7F8C8D",
}


def _find_row_for_tag(panel: TagPanel, tag_name: str) -> QWidget | None:
    """Find the row widget that contains a label with *tag_name*."""
    # Walk up from the label to find the row (direct child of scroll layout)
    for label in panel.findChildren(QLabel):
        if label.text().startswith(f"{tag_name} "):
            # The row is the parent QWidget that holds the layout
            row = label.parent()
            while row is not None and not isinstance(row, QWidget):
                row = row.parent()
            return row
    return None


def _get_swatch_in_row(row: QWidget) -> QLabel | None:
    """Return the color swatch QLabel inside *row*, or None."""
    for child in row.findChildren(QLabel):
        if child.objectName() == "colorSwatch":
            return child
    return None


# =========================================================================
# TestAutoColorTagSwatch
# =========================================================================


class TestAutoColorTagSwatch:
    """Auto-color tags display a color swatch widget."""

    def test_auto_color_tag_has_swatch(self, service: TagService, panel: TagPanel) -> None:
        """Auto-color tag row contains a color swatch QLabel."""
        # Arrange
        service.get_or_create_tag("color:red")
        panel._refresh_tags()

        # Act
        row = _find_row_for_tag(panel, "color:red")

        # Assert
        assert row is not None, "Row for 'color:red' should exist"
        swatch = _get_swatch_in_row(row)
        assert swatch is not None, "Auto-color tag row should have a color swatch"
        assert swatch.width() == 16
        assert swatch.height() == 16

    def test_manual_tag_no_swatch(self, service: TagService, panel: TagPanel) -> None:
        """Manual tag row does NOT contain a color swatch."""
        # Arrange
        service.get_or_create_tag("landscape")
        panel._refresh_tags()

        # Act
        row = _find_row_for_tag(panel, "landscape")

        # Assert
        assert row is not None, "Row for 'landscape' should exist"
        swatch = _get_swatch_in_row(row)
        assert swatch is None, "Manual tag row should NOT have a color swatch"


# =========================================================================
# TestAutoColorTagTooltip
# =========================================================================


class TestAutoColorTagTooltip:
    """Auto-color tags display the correct tooltip."""

    def test_auto_color_tag_has_tooltip(self, service: TagService, panel: TagPanel) -> None:
        """Auto-color tag row has 'Auto color tag' tooltip."""
        # Arrange
        service.get_or_create_tag("color:blue")
        panel._refresh_tags()

        # Act
        row = _find_row_for_tag(panel, "color:blue")

        # Assert
        assert row is not None, "Row for 'color:blue' should exist"
        assert row.toolTip() == "Auto color tag"

    def test_manual_tag_no_special_tooltip(self, service: TagService, panel: TagPanel) -> None:
        """Manual tag row does NOT have 'Auto color tag' tooltip."""
        # Arrange
        service.get_or_create_tag("portrait")
        panel._refresh_tags()

        # Act
        row = _find_row_for_tag(panel, "portrait")

        # Assert
        assert row is not None, "Row for 'portrait' should exist"
        assert row.toolTip() != "Auto color tag"


# =========================================================================
# TestAutoColorTagDashedBorder
# =========================================================================


class TestAutoColorTagDashedBorder:
    """Auto-color tags use a dashed border style."""

    def test_auto_color_tag_has_dashed_border(self, service: TagService, panel: TagPanel) -> None:
        """Auto-color tag row stylesheet contains 'dashed'."""
        # Arrange
        service.get_or_create_tag("color:green")
        panel._refresh_tags()

        # Act
        row = _find_row_for_tag(panel, "color:green")

        # Assert
        assert row is not None, "Row for 'color:green' should exist"
        style = row.styleSheet()
        assert "dashed" in style, f"Auto-color tag row should have dashed border, got: {style!r}"


# =========================================================================
# TestColorSwatchCorrectColor
# =========================================================================


class TestColorSwatchCorrectColor:
    """Each color bucket produces the correct hex swatch color."""

    @pytest.mark.parametrize(
        "color_name,expected_hex",
        list(EXPECTED_HEX.items()),
        ids=ALL_COLOR_BUCKETS,
    )
    def test_color_swatch_correct_color(
        self,
        service: TagService,
        panel: TagPanel,
        color_name: str,
        expected_hex: str,
    ) -> None:
        """Swatch shows correct hex color for each bucket."""
        # Arrange
        tag_name = f"color:{color_name}"
        service.get_or_create_tag(tag_name)
        panel._refresh_tags()

        # Act
        row = _find_row_for_tag(panel, tag_name)
        swatch = _get_swatch_in_row(row) if row else None

        # Assert
        assert row is not None, f"Row for '{tag_name}' should exist"
        assert swatch is not None, f"Swatch for '{tag_name}' should exist"
        swatch_style = swatch.styleSheet()
        assert expected_hex in swatch_style, (
            f"Swatch for '{color_name}' should contain {expected_hex}, " f"got: {swatch_style!r}"
        )


# =========================================================================
# TestAllColorBucketsRecognized
# =========================================================================


class TestAllColorBucketsRecognized:
    """All 10 color bucket names are identified as auto-color tags."""

    @pytest.mark.parametrize("color_name", ALL_COLOR_BUCKETS, ids=ALL_COLOR_BUCKETS)
    def test_all_color_buckets_recognized(
        self,
        service: TagService,
        panel: TagPanel,
        color_name: str,
    ) -> None:
        """Each color bucket tag gets auto-color styling (swatch + tooltip)."""
        # Arrange
        tag_name = f"color:{color_name}"
        service.get_or_create_tag(tag_name)
        panel._refresh_tags()

        # Act
        row = _find_row_for_tag(panel, tag_name)

        # Assert — auto-color indicators present
        assert row is not None, f"Row for '{tag_name}' should exist"
        assert row.toolTip() == "Auto color tag", f"'{tag_name}' should have auto-color tooltip"
        assert "dashed" in row.styleSheet(), f"'{tag_name}' should have dashed border"
        swatch = _get_swatch_in_row(row)
        assert swatch is not None, f"'{tag_name}' should have a color swatch"


# =========================================================================
# TestGetColorHex
# =========================================================================


class TestGetColorHex:
    """_get_color_hex maps names correctly and handles unknowns."""

    @pytest.mark.parametrize(
        "color_name,expected_hex",
        list(EXPECTED_HEX.items()),
        ids=ALL_COLOR_BUCKETS,
    )
    def test_known_colors(self, color_name: str, expected_hex: str) -> None:
        """Known color names map to their expected hex values."""
        assert TagPanel._get_color_hex(color_name) == expected_hex

    def test_unknown_color_returns_default(self) -> None:
        """Unknown color names fall back to default grey."""
        assert TagPanel._get_color_hex("chartreuse") == "#888888"

    def test_empty_string_returns_default(self) -> None:
        """Empty string falls back to default grey."""
        assert TagPanel._get_color_hex("") == "#888888"
