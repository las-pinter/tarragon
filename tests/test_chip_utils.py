"""Tests for the shared chip close-button extraction (audit #18)."""

from __future__ import annotations

import pytest
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QPushButton

from tarragon.widgets._chip_utils import create_chip_close_button, create_removable_chip
from tarragon.widgets.tag_pill import TagPillWidget


class TestCreateChipCloseButton:
    """create_chip_close_button builds the shared 16x16 close button."""

    @pytest.mark.parametrize(
        ("text", "object_name"),
        [
            ("x", "tagPillRemoveBtn"),
            ("\u00d7", "filterChipRemoveBtn"),
        ],
    )
    def test_button_configured(self, text: str, object_name: str) -> None:
        """The button carries the requested glyph, object name, and cursor."""
        btn = create_chip_close_button(text, object_name)
        assert isinstance(btn, QPushButton)
        assert btn.text() == text
        assert btn.objectName() == object_name
        assert btn.cursor().shape() == Qt.CursorShape.PointingHandCursor

    @pytest.mark.parametrize("text", ["x", "\u00d7"])
    def test_button_is_fixed_16x16(self, text: str) -> None:
        """The close button is locked to 16x16 pixels for QSS alignment."""
        btn = create_chip_close_button(text, "removeBtn")
        assert btn.minimumWidth() == 16
        assert btn.maximumWidth() == 16
        assert btn.minimumHeight() == 16
        assert btn.maximumHeight() == 16


class TestCloseButtonReuse:
    """Both the chip factory and the tag pill route through the helper."""

    def test_removable_chip_close_button_keeps_chip_identity(self) -> None:
        """Filter chips keep the ``\u00d7`` glyph and filterChipRemoveBtn name."""
        chip = create_removable_chip(label_text="tag", on_remove=lambda: None)
        remove_btns = chip.findChildren(QPushButton)
        assert len(remove_btns) == 1
        assert remove_btns[0].text() == "\u00d7"
        assert remove_btns[0].objectName() == "filterChipRemoveBtn"

    def test_tag_pill_remove_button_keeps_pill_identity(self) -> None:
        """Tag pills keep the 'x' glyph and tagPillRemoveBtn name."""
        pill = TagPillWidget(tag_name="tag", on_remove=lambda: None, on_toggle=lambda: None)
        try:
            assert isinstance(pill._remove_btn, QPushButton)
            assert pill._remove_btn.text() == "x"
            assert pill._remove_btn.objectName() == "tagPillRemoveBtn"
        finally:
            pill.close()
