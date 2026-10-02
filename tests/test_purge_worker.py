"""Tests for the async cache-purge worker (_PurgeCacheTask)."""

from __future__ import annotations

import threading
import time
from collections.abc import Callable, Generator
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from PySide6.QtWidgets import QApplication

from tarragon.db.database import Database
from tarragon.scanner import FileInfo
from tarragon.services.thumbnail_service import (
    POOL_DRAIN_TIMEOUT_MS,
    ThumbnailService,
    _RenderAllTask,
)
from tarragon.theme.constants import PSD_WORKER_DEFAULT


@pytest.fixture
def real_db() -> Generator[Database, None, None]:
    """Provide a real in-memory Database for purge integration tests."""
    db = Database(Path(":memory:"))
    db.init_schema()
    yield db
    db.close()


def _make_settings() -> MagicMock:
    """MagicMock settings with the fields ThumbnailService touches at runtime."""
    settings = MagicMock()
    settings.cache_format.get.return_value = "PNG"
    settings.max_psd_workers.get.return_value = PSD_WORKER_DEFAULT
    settings.color_tag_enabled.get.return_value = False
    settings.clear_full_res_on_exit.get.return_value = False
    return settings


def _wait_until(condition: Callable[[], bool], timeout_ms: int = 15000) -> bool:
    """Spin the Qt event loop until *condition* holds or the timeout elapses."""
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        QApplication.processEvents()
        if condition():
            return True
        time.sleep(0.01)
    return condition()


@pytest.fixture
def purge_service() -> Generator[ThumbnailService, None, None]:
    """ThumbnailService with a mocked DB and a pool that reports drain outcomes."""
    db = MagicMock()
    settings = MagicMock()
    settings.cache_format.get.return_value = "PNG"
    settings.max_psd_workers.get.return_value = PSD_WORKER_DEFAULT
    settings.color_tag_enabled.get.return_value = False
    settings.clear_full_res_on_exit.get.return_value = False
    with patch("tarragon.services.thumbnail_service.get_executor"):
        svc = ThumbnailService(db=db, settings_service=settings, tag_service=MagicMock())
    mock_pool = MagicMock()
    svc._threadpool = mock_pool
    yield svc


class TestPurgeWorker:
    """purge_cache dispatches a worker that drains, unlinks, and reports completion."""

    def test_purge_emits_started_and_completed_signals(
        self,
        purge_service: ThumbnailService,
    ) -> None:
        """A fulfilled purge emits purge_started then cache_purged exactly once each."""
        purge_service._threadpool.waitForDone.return_value = True  # type: ignore[attr-defined]
        started: list[bool] = []
        purged: list[bool] = []
        purge_service.purge_started.connect(lambda: started.append(True))
        purge_service.cache_purged.connect(lambda: purged.append(True))
        with patch("tarragon.services.thumbnail_service.clear_cache") as mock_clear:
            purge_service.purge_cache()

        assert started == [True]
        assert purged == [True]
        mock_clear.assert_called_once_with()
        purge_service._db.clear_thumbnails.assert_called_once_with()  # type: ignore[attr-defined]
        assert not purge_service._purge_in_progress

    def test_purge_abort_still_emits_completion(
        self,
        purge_service: ThumbnailService,
    ) -> None:
        """When the pool does not drain, no cache clears happen but cache_purged still fires."""
        purge_service._threadpool.waitForDone.return_value = False  # type: ignore[attr-defined]
        purged: list[bool] = []
        purge_service.cache_purged.connect(lambda: purged.append(True))
        with patch("tarragon.services.thumbnail_service.clear_cache") as mock_clear:
            purge_service.purge_cache()

        assert purged == [True]
        mock_clear.assert_not_called()
        purge_service._db.clear_thumbnails.assert_not_called()  # type: ignore[attr-defined]

    def test_purge_uses_drain_timeout(self, purge_service: ThumbnailService) -> None:
        """The purge worker waits for the pool with the standard drain timeout."""
        purge_service.purge_cache()
        purge_service._threadpool.waitForDone.assert_called_once_with(POOL_DRAIN_TIMEOUT_MS)  # type: ignore[attr-defined]

    def test_purge_single_flight_ignores_second_request(
        self,
        purge_service: ThumbnailService,
    ) -> None:
        """A second purge request while one is in progress is ignored."""
        purge_service._purge_in_progress = True
        started: list[bool] = []
        purge_service.purge_started.connect(lambda: started.append(True))

        purge_service.purge_cache()

        assert started == []
        purge_service._threadpool.start.assert_not_called()  # type: ignore[attr-defined]

    def test_purge_resets_in_progress_flag_after_completion(
        self,
        purge_service: ThumbnailService,
    ) -> None:
        """Submitting a real purge leaves the single-flight flag cleared."""
        purge_service._threadpool.waitForDone.return_value = True  # type: ignore[attr-defined]
        with patch("tarragon.services.thumbnail_service.clear_cache"):
            purge_service.purge_cache()
        assert not purge_service._purge_in_progress

        # A subsequent request is accepted.
        started: list[bool] = []
        purge_service.purge_started.connect(lambda: started.append(True))
        with patch("tarragon.services.thumbnail_service.clear_cache"):
            purge_service.purge_cache()
        assert started == [True]


class TestPurgeWorkerAsync:
    """purge_cache under the real async path (genuine QThreadPool, seam off per-instance)."""

    def test_purge_async_clears_disk_and_db_without_stalling(
        self,
        real_db: Database,
        tmp_path: Path,
    ) -> None:
        """A full async purge drains an in-flight render, deletes disk + DB rows, and reports once."""
        cache_root = tmp_path / "cache"
        with (
            patch("tarragon.services.thumbnail_service.get_executor"),
            patch("tarragon.renderers.cache.cache_dir", return_value=cache_root),
        ):
            svc = ThumbnailService(
                db=real_db,
                settings_service=_make_settings(),
                tag_service=MagicMock(),
                synchronous_workers=False,
            )

            # An in-flight render keeps the render pool busy for a short time; the
            # purge must drain it (own pool) rather than deadlock on the timeout.
            late_file = cache_root / "256" / "late" / "late.png"
            started = threading.Event()

            def slow_render(file_info: FileInfo) -> None:
                """Simulate an inflight render that finishes and writes a cache file."""
                started.set()
                time.sleep(0.3)
                late_file.parent.mkdir(parents=True, exist_ok=True)
                late_file.write_bytes(b"late")

            file_info = FileInfo(path=tmp_path / "source.png", mtime=1.0, size=1, extension=".png")
            svc._threadpool.start(
                _RenderAllTask(
                    file_info=file_info,
                    on_error=lambda *args: None,
                    render_func=slow_render,
                    cancel_event=svc._cancel_event,
                )
            )
            assert started.wait(timeout=5)

            # Seed disk + DB as if thumbnails existed for the purge to remove.
            tier_dir = cache_root / "256" / "folder_abc12345"
            tier_dir.mkdir(parents=True)
            cache_file = tier_dir / "image.png"
            cache_file.write_bytes(b"data")
            real_db.bulk_upsert_stubs([(str(tmp_path / "source.png"), 1, 100)])

            started_signals: list[bool] = []
            purged_signals: list[bool] = []
            svc.purge_started.connect(lambda: started_signals.append(True))
            svc.cache_purged.connect(lambda: purged_signals.append(True))

            begin = time.monotonic()
            svc.purge_cache()
            delivered = _wait_until(lambda: purged_signals != [])
            elapsed = time.monotonic() - begin

        svc._threadpool.waitForDone(5000)
        svc._purge_pool.waitForDone(5000)

        assert delivered, "cache_purged never emitted"
        assert started_signals == [True]
        assert purged_signals == [True]
        # The drain must complete well inside the timeout: a self-drained purge
        # (task waiting on its own pool) would stall to the 5 s abort instead.
        assert elapsed < POOL_DRAIN_TIMEOUT_MS / 1000 - 1.0, f"purge took {elapsed:.2f}s; drain likely stalled"
        assert not cache_file.exists()
        assert not late_file.exists()
        assert list(cache_root.rglob("*")) == []
        assert real_db.list_thumbnails_for_folder(str(tmp_path)) == []
        assert not svc._purge_in_progress
