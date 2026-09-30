"""Tests for the busy indicator widget and its MainWindow wiring."""

from __future__ import annotations

from pathlib import Path
from typing import Any
from unittest.mock import MagicMock

from PySide6.QtWidgets import QLabel, QProgressBar

from tarragon.db.database import Database
from tarragon.main_window import MainWindow
from tarragon.services.tag_service import TagService
from tarragon.widgets.busy_indicator import BusyIndicator


class TestBusyIndicatorWidget:
    """The BusyIndicator ref-counts operations and hides when idle."""

    def test_hidden_and_idle_by_default(self, qapp: Any) -> None:
        """A freshly-created indicator is hidden and not busy."""
        indicator = BusyIndicator()
        try:
            assert not indicator.is_busy
            assert indicator.isHidden()
        finally:
            indicator.close()

    def test_begin_op_shows_and_end_op_hides(self, qapp: Any) -> None:
        """begin_op() shows the strip and end_op() hides it."""
        indicator = BusyIndicator()
        try:
            indicator.begin_op()
            assert indicator.is_busy
            assert not indicator.isHidden()

            indicator.end_op()
            assert not indicator.is_busy
            assert indicator.isHidden()
        finally:
            indicator.close()

    def test_refcount_keeps_visible_until_last_op_ends(self, qapp: Any) -> None:
        """Overlapping operations keep the strip visible until the last one ends."""
        indicator = BusyIndicator()
        try:
            indicator.begin_op()
            indicator.begin_op()
            indicator.end_op()
            assert indicator.is_busy
            assert not indicator.isHidden()

            indicator.end_op()
            assert not indicator.is_busy
        finally:
            indicator.close()

    def test_end_op_without_begin_is_noop(self, qapp: Any) -> None:
        """end_op() on an idle indicator stays idle (count never goes negative)."""
        indicator = BusyIndicator()
        try:
            indicator.end_op()
            indicator.end_op()
            assert not indicator.is_busy
            assert indicator.isHidden()
        finally:
            indicator.close()

    def test_strip_contains_label_and_indeterminate_bar(self, qapp: Any) -> None:
        """The strip is a label plus an indeterminate (range 0,0) progress bar."""
        indicator = BusyIndicator()
        try:
            assert isinstance(indicator._label, QLabel)
            assert isinstance(indicator._bar, QProgressBar)
            assert indicator._bar.minimum() == 0
            assert indicator._bar.maximum() == 0
        finally:
            indicator.close()


class TestBusyIndicatorWiring:
    """MainWindow drives the indicator from async-pipeline signals."""

    def test_scan_signals_toggle_indicator(
        self,
        qapp: Any,
        mock_settings: MagicMock,
    ) -> None:
        """scan_started shows the indicator and scan_finished hides it."""
        window = MainWindow(settings_service=mock_settings)
        try:
            db = Database(Path(":memory:"))
            db.init_schema()
            window.setup_widgets(db, TagService(db=db))
            indicator = window._busy_indicator
            assert indicator is not None

            window._thumbnail_service.scan_started.emit(1, "/a")
            assert indicator.is_busy
            assert not indicator.isHidden()

            window._thumbnail_service.scan_finished.emit(1)
            assert not indicator.is_busy
            assert indicator.isHidden()
        finally:
            window.close()

    def test_stale_scan_finished_keeps_indicator_visible(
        self,
        qapp: Any,
        mock_settings: MagicMock,
    ) -> None:
        """A scan_finished for an old token leaves the active scan's busy flag set."""
        window = MainWindow(settings_service=mock_settings)
        try:
            db = Database(Path(":memory:"))
            db.init_schema()
            window.setup_widgets(db, TagService(db=db))
            indicator = window._busy_indicator
            assert indicator is not None

            window._thumbnail_service.scan_started.emit(2, "/b")
            window._thumbnail_service.scan_finished.emit(1)
            assert indicator.is_busy

            window._thumbnail_service.scan_finished.emit(2)
            assert not indicator.is_busy
        finally:
            window.close()

    def test_purge_signals_toggle_indicator(
        self,
        qapp: Any,
        mock_settings: MagicMock,
    ) -> None:
        """purge_started shows the indicator and cache_purged hides it."""
        window = MainWindow(settings_service=mock_settings)
        try:
            db = Database(Path(":memory:"))
            db.init_schema()
            window.setup_widgets(db, TagService(db=db))
            indicator = window._busy_indicator
            assert indicator is not None

            window._thumbnail_service.purge_started.emit()
            assert indicator.is_busy

            window._thumbnail_service.cache_purged.emit()
            assert not indicator.is_busy
        finally:
            window.close()

    def test_decode_relay_toggles_indicator(
        self,
        qapp: Any,
        mock_settings: MagicMock,
    ) -> None:
        """Multi-select decode busy signals drive the indicator."""
        window = MainWindow(settings_service=mock_settings)
        try:
            db = Database(Path(":memory:"))
            db.init_schema()
            window.setup_widgets(db, TagService(db=db))
            indicator = window._busy_indicator
            assert indicator is not None

            window._gallery_controller.decode_relay.decode_busy_started.emit()
            assert indicator.is_busy

            window._gallery_controller.decode_relay.decode_busy_finished.emit()
            assert not indicator.is_busy
        finally:
            window.close()

    def test_overlapping_ops_keep_indicator_visible(
        self,
        qapp: Any,
        mock_settings: MagicMock,
    ) -> None:
        """The indicator stays visible while any operation is still running."""
        window = MainWindow(settings_service=mock_settings)
        try:
            db = Database(Path(":memory:"))
            db.init_schema()
            window.setup_widgets(db, TagService(db=db))
            indicator = window._busy_indicator
            assert indicator is not None

            window._thumbnail_service.scan_started.emit(1, "/a")
            window._thumbnail_service.purge_started.emit()
            assert indicator.is_busy

            window._thumbnail_service.scan_finished.emit(1)
            assert indicator.is_busy

            window._thumbnail_service.cache_purged.emit()
            assert not indicator.is_busy
        finally:
            window.close()

    def test_busy_during_synchronous_scan(
        self,
        qapp: Any,
        mock_settings: MagicMock,
        tmp_path: Path,
    ) -> None:
        """A synchronous-dispatched scan toggles the indicator on then off."""
        window = MainWindow(settings_service=mock_settings)
        try:
            db = Database(Path(":memory:"))
            db.init_schema()
            window.setup_widgets(db, TagService(db=db))
            indicator = window._busy_indicator
            assert indicator is not None

            folder = tmp_path / "images"
            folder.mkdir()
            (folder / "a.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"\x00" * 10)

            states: list[bool] = []
            window._thumbnail_service.scan_started.connect(lambda *_: states.append(indicator.is_busy))
            window._thumbnail_service.scan_finished.connect(lambda *_: states.append(indicator.is_busy))

            window._navigate_to_folder(folder)

            assert states == [True, False]
        finally:
            window.close()
