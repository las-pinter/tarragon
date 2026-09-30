"""Tests for async multi-select preview decode offload."""

from __future__ import annotations

import threading
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image
from PySide6.QtCore import QTimer
from PySide6.QtWidgets import QApplication, QLineEdit

import tarragon.gallery_controller as gallery_controller
from tarragon.db.database import Database
from tarragon.gallery_controller import GalleryController
from tarragon.models.filter_state import FilterState
from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.services.query_service import QueryService
from tarragon.services.tag_service import TagService
from tarragon.widgets.filter_bar import FilterBar
from tarragon.widgets.gallery_info_bar import GalleryInfoBar
from tarragon.widgets.gallery_tabs import GalleryTabs
from tarragon.widgets.preview_panel import PreviewPanel

_MULTI_CAP = 9


def _write_png(path: Path, color: str = "red") -> Path:
    """Create a small real PNG for decode workers to load."""
    Image.new("RGB", (64, 64), color=color).save(path)
    return path


def _make_controller(synchronous_workers: bool | None = None) -> GalleryController:
    """Build a GalleryController wired to real widgets and an in-memory DB."""
    db = Database(Path(":memory:"))
    db.init_schema()
    tag_service = TagService(db=db)
    settings = MagicMock()
    settings.max_multi_preview.get.return_value = _MULTI_CAP
    preview_panel = PreviewPanel(settings_service=settings)
    return GalleryController(
        query_service=QueryService(db=db),
        filter_state=FilterState(),
        thumbnail_model=ThumbnailModel(),
        gallery_tabs=GalleryTabs(),
        gallery_info_bar=GalleryInfoBar(),
        filter_bar=FilterBar(tag_service=tag_service, db=db),
        search_edit=QLineEdit(),
        search_timer=QTimer(),
        preview_panel=preview_panel,
        tag_service=tag_service,
        db=db,
        max_multi_preview=_MULTI_CAP,
        synchronous_workers=synchronous_workers,
    )


def _wait_until(condition: Callable[[], bool], timeout_ms: int = 5000) -> bool:
    """Spin the Qt event loop until *condition* holds or the timeout elapses."""
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        QApplication.processEvents()
        if condition():
            return True
        time.sleep(0.01)
    return condition()


class TestDecodeOffloadSync:
    """Multi-select decode under the synchronous test seam is deterministic."""

    def test_multi_select_decodes_inline_and_updates_panel(self, tmp_path: Path) -> None:
        """Selecting multiple files decodes each inline and fills the mosaic immediately."""
        ctl = _make_controller()
        paths = [str(_write_png(tmp_path / f"img{i}.png")) for i in range(3)]
        busy: list[str] = []
        ctl.decode_relay.decode_busy_started.connect(lambda: busy.append("start"))
        ctl.decode_relay.decode_busy_finished.connect(lambda: busy.append("finish"))

        ctl.on_selection_changed(paths)

        assert len(ctl._preview_panel._mosaic_image_infos) == 3
        assert ctl._preview_panel._mosaic_placeholder.isHidden()
        assert ctl._decode_pending == 0
        assert busy == ["start", "finish"]

    def test_multi_select_respects_cap(self, tmp_path: Path) -> None:
        """Only the first max_multi_preview paths are decoded."""
        ctl = _make_controller()
        paths = [str(_write_png(tmp_path / f"img{i}.png")) for i in range(12)]
        ready: list[tuple[Any, ...]] = []
        ctl.decode_relay.decode_ready.connect(lambda *args: ready.append(args))

        ctl.on_selection_changed(paths)

        assert len(ready) == _MULTI_CAP
        assert len(ctl._preview_panel._mosaic_image_infos) == _MULTI_CAP

    def test_single_selection_remains_synchronous(self, tmp_path: Path) -> None:
        """Single selection loads synchronously into set_image (unchanged fast path)."""
        ctl = _make_controller()
        path = _write_png(tmp_path / "single.png")
        started_report: list[bool] = []
        ctl.decode_relay.decode_busy_started.connect(lambda: started_report.append(True))

        ctl.on_selection_changed([str(path)])

        assert started_report == []
        assert ctl._preview_panel._cached_pixmap is not None
        assert ctl._preview_panel._preview_stack.currentWidget() is ctl._preview_panel._image_label

    def test_clear_selection_cancels_pending_batch(self, tmp_path: Path) -> None:
        """Clearing the selection emits a busy-finished for any pending decode batch."""
        ctl = _make_controller()
        ctl._decode_pending = 2
        busy: list[str] = []
        ctl.decode_relay.decode_busy_started.connect(lambda: busy.append("start"))
        ctl.decode_relay.decode_busy_finished.connect(lambda: busy.append("finish"))

        ctl.on_selection_changed([])

        assert busy == ["finish"]
        assert ctl._decode_pending == 0

    def test_stale_decode_result_is_discarded(self, tmp_path: Path) -> None:
        """A decode_ready from a superseded epoch does not add a wrong image to the mosaic."""
        ctl = _make_controller()
        current = _write_png(tmp_path / "current.png")
        other = _write_png(tmp_path / "other.png")
        ctl.on_selection_changed([str(current), str(other)])
        assert len(ctl._preview_panel._mosaic_image_infos) == 2

        stale_epoch = ctl._decode_epoch - 1
        stale_img = Image.new("RGB", (16, 16), color="blue")
        ctl.decode_relay.decode_ready.emit(stale_epoch, str(tmp_path / "stale.png"), stale_img, 16, 16, False)

        infos = ctl._preview_panel._mosaic_image_infos
        assert len(infos) == 2
        assert {info.path for info in infos} == {current, other}

    def test_stale_decode_failure_is_ignored(self, tmp_path: Path) -> None:
        """A decode_failed from a superseded epoch does not corrupt the pending counter."""
        ctl = _make_controller()
        ctl.on_selection_changed([str(_write_png(tmp_path / "a.png"))])
        assert ctl._decode_pending == 0

        ctl.decode_relay.decode_failed.emit(ctl._decode_epoch - 1, "/old/path.png", "boom")

        assert ctl._decode_pending == 0

    def test_decode_failure_counts_toward_completion(
        self,
        tmp_path: Path,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """A failed decode is counted so the batch still reports busy-finished."""
        ctl = _make_controller()
        broken = _write_png(tmp_path / "broken.png")
        good = _write_png(tmp_path / "good.png")

        def boom(_db: Database, _path: Path) -> gallery_controller.LoadedImage:
            """Simulate an unreadable source file."""
            raise OSError("decode failed")

        monkeypatch.setattr(gallery_controller, "_load_preview_image_data", boom)
        busy: list[str] = []
        ctl.decode_relay.decode_busy_started.connect(lambda: busy.append("start"))
        ctl.decode_relay.decode_busy_finished.connect(lambda: busy.append("finish"))

        ctl.on_selection_changed([str(broken), str(good)])

        assert busy == ["start", "finish"]
        assert ctl._decode_pending == 0
        assert ctl._preview_panel._mosaic_image_infos == []
        # A batch with zero successful decodes must not leave the loading
        # placeholder stuck: it shows an empty state instead.
        assert not ctl._preview_panel._mosaic_placeholder.isHidden()
        assert ctl._preview_panel._mosaic_placeholder.text() == "No previews available"

    def test_panel_mutation_failure_does_not_corrupt_decode_state(self, tmp_path: Path) -> None:
        """A raise in add_multi_preview_image still clears pending and emits busy-finished."""
        ctl = _make_controller()
        path = str(_write_png(tmp_path / "boom_add.png"))
        ctl._decode_pending = 1
        busy: list[str] = []
        ctl.decode_relay.decode_busy_finished.connect(lambda: busy.append("finish"))

        with patch.object(ctl._preview_panel, "add_multi_preview_image", side_effect=RuntimeError("mosaic boom")):
            with pytest.raises(RuntimeError):
                ctl._on_preview_decoded(ctl._decode_epoch, path, Image.new("RGB", (8, 8), "red"), 8, 8, False)

        # Completion accounting ran before the panel mutation, so the pending
        # counter is not left stuck and the busy indicator clears exactly once.
        assert ctl._decode_pending == 0
        assert busy == ["finish"]

    def test_clear_resets_accumulated_mosaic_state(self, tmp_path: Path) -> None:
        """Clearing the preview drops the accumulated mosaic infos instead of pinning them."""
        ctl = _make_controller()
        ctl.on_selection_changed([str(_write_png(tmp_path / "a.png")), str(_write_png(tmp_path / "b.png"))])
        assert len(ctl._preview_panel._mosaic_image_infos) == 2

        ctl._preview_panel.clear()

        assert ctl._preview_panel._mosaic_image_infos == []
        assert ctl._preview_panel._mosaic_placeholder.isHidden()


class TestDecodeOffloadAsync:
    """Multi-select decode under the real async path runs off the GUI thread."""

    def test_decode_runs_in_worker_thread_and_fills_mosaic(self, tmp_path: Path) -> None:
        """Decode workers run outside the GUI thread and results arrive per image."""
        ctl = _make_controller(synchronous_workers=False)
        paths = [str(_write_png(tmp_path / f"async{i}.png")) for i in range(3)]
        threads: list[str] = []
        original_load = gallery_controller._load_preview_image_data

        def spy_load(db: Database, path: Path) -> gallery_controller.LoadedImage:
            """Record the thread doing the decode."""
            threads.append(threading.current_thread().name)
            return original_load(db, path)

        gallery_controller._load_preview_image_data = spy_load
        try:
            ctl.on_selection_changed(paths)
            delivered = _wait_until(lambda: len(ctl._preview_panel._mosaic_image_infos) == 3)
        finally:
            gallery_controller._load_preview_image_data = original_load
            ctl._decode_pool.waitForDone(5000)

        assert delivered, "async decode results did not arrive within the timeout"
        assert threads, "decode worker never observed"
        main_thread = threading.current_thread().name
        assert all(thread != main_thread for thread in threads)
        assert ctl._decode_pending == 0

    def test_selection_change_discards_stale_async_results(self, tmp_path: Path) -> None:
        """A later selection replaces the mosaic and late results from the old epoch are dropped."""
        ctl = _make_controller(synchronous_workers=False)
        first_a = _write_png(tmp_path / "first_a.png")
        first_b = _write_png(tmp_path / "first_b.png")
        second_a = _write_png(tmp_path / "second_a.png")
        second_b = _write_png(tmp_path / "second_b.png")

        ctl.on_selection_changed([str(first_a), str(first_b)])
        assert _wait_until(lambda: len(ctl._preview_panel._mosaic_image_infos) == 2)
        ctl._decode_pool.waitForDone(5000)

        ctl.on_selection_changed([str(second_a), str(second_b)])
        assert _wait_until(lambda: len(ctl._preview_panel._mosaic_image_infos) == 2)

        infos = ctl._preview_panel._mosaic_image_infos
        assert {info.path for info in infos} == {second_a, second_b}
        assert ctl._decode_pending == 0
        ctl._decode_pool.waitForDone(5000)

    def test_async_failure_still_completes_batch(self, tmp_path: Path) -> None:
        """A worker failure under the real async path still clears the pending counter."""
        ctl = _make_controller(synchronous_workers=False)
        good = _write_png(tmp_path / "good.png")
        missing = tmp_path / "missing.png"

        ctl.on_selection_changed([str(good), str(missing)])

        assert _wait_until(lambda: ctl._decode_pending == 0)
        assert len(ctl._preview_panel._mosaic_image_infos) == 1
        ctl._decode_pool.waitForDone(5000)
