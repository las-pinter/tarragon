"""Tests for the async folder-scan worker (_ScanFolderTask)."""

from __future__ import annotations

import threading
import time
from collections.abc import Callable, Generator
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from PySide6.QtWidgets import QApplication

from tarragon.db.database import Database
from tarragon.scanner import FileInfo, scan_folder
from tarragon.services.thumbnail_service import ThumbnailService, _ScanFolderTask
from tarragon.theme.constants import PSD_WORKER_DEFAULT


@pytest.fixture
def real_db() -> Generator[Database, None, None]:
    """Provide a real in-memory Database for scan integration tests."""
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


@pytest.fixture
def scan_service(real_db: Database) -> ThumbnailService:
    """ThumbnailService with a real DB and a pool that runs render tasks inline."""
    with patch("tarragon.services.thumbnail_service.get_executor"):
        svc = ThumbnailService(db=real_db, settings_service=_make_settings(), tag_service=MagicMock())
    mock_pool = MagicMock()
    mock_pool.start.side_effect = lambda task: task.run()
    svc._threadpool = mock_pool
    return svc


def _make_folder(tmp_path: Path, names: list[str]) -> Path:
    """Create a temp folder containing dummy image files."""
    folder = tmp_path / "images"
    folder.mkdir()
    for name in names:
        (folder / name).write_bytes(b"fake-image-data")
    return folder


def _wait_until(condition: Callable[[], bool], timeout_ms: int = 5000) -> bool:
    """Spin the Qt event loop until *condition* holds or the timeout elapses."""
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        QApplication.processEvents()
        if condition():
            return True
        time.sleep(0.01)
    return condition()


class TestScanWorker:
    """_ScanFolderTask emits correct results and preserves stub-before-query ordering."""

    def test_scan_completed_emits_file_info_list(
        self,
        scan_service: ThumbnailService,
        real_db: Database,
        tmp_path: Path,
    ) -> None:
        """A completed scan emits scan_completed with the discovered FileInfo list."""
        folder = _make_folder(tmp_path, ["b.png", "a.png"])
        completed: list[tuple[int, list[FileInfo]]] = []
        finished: list[int] = []
        scan_service.scan_completed.connect(lambda token, infos: completed.append((token, list(infos))))
        scan_service.scan_finished.connect(lambda token: finished.append(token))

        token = scan_service.request_folder_scan(folder)

        assert len(completed) == 1
        infos = completed[0][1]
        assert [fi.path.name for fi in infos] == ["a.png", "b.png"]
        assert all(isinstance(fi, FileInfo) for fi in infos)
        assert finished == [token]

    def test_stubs_written_before_completion_signal(
        self,
        scan_service: ThumbnailService,
        real_db: Database,
        tmp_path: Path,
    ) -> None:
        """DB stubs exist by the time scan_completed fires so queries see results immediately."""
        folder = _make_folder(tmp_path, ["a.png", "b.png", "c.png"])
        seen: list[tuple[int, int]] = []

        def on_completed(token: int, _infos: object) -> None:
            seen.append((token, len(real_db.list_thumbnails_for_folder(str(folder)))))

        scan_service.scan_completed.connect(on_completed)

        token = scan_service.request_folder_scan(folder)

        assert seen == [(token, 3)]

    def test_empty_folder_completes_with_empty_list(
        self,
        scan_service: ThumbnailService,
        real_db: Database,
        tmp_path: Path,
    ) -> None:
        """Scanning an empty folder emits scan_completed with no files."""
        folder = tmp_path / "empty"
        folder.mkdir()
        completed: list[list[FileInfo]] = []
        scan_service.scan_completed.connect(lambda _token, infos: completed.append(list(infos)))

        scan_service.request_folder_scan(folder)

        assert completed == [[]]

    def test_cancelled_before_start_emits_only_finished(
        self,
        scan_service: ThumbnailService,
        tmp_path: Path,
    ) -> None:
        """A task whose cancel event is set before run() emits no completion signal."""
        folder = _make_folder(tmp_path, ["a.png"])
        completed: list[object] = []
        finished: list[int] = []
        scan_service.scan_completed.connect(lambda *args: completed.append(args))
        scan_service.scan_finished.connect(lambda token: finished.append(token))

        task = _ScanFolderTask(
            folder_path=folder,
            token=42,
            service=scan_service,
            cancel_event=scan_service._cancel_event,
            supersede_event=threading.Event(),
        )
        scan_service._cancel_event.set()
        task.run()

        assert completed == []
        assert finished == [42]

    def test_cancel_mid_dispatch_keeps_stubs_and_skips_completion(
        self,
        scan_service: ThumbnailService,
        real_db: Database,
        tmp_path: Path,
    ) -> None:
        """A cancel during the dispatch loop leaves stubs intact but suppresses completion."""
        folder = _make_folder(tmp_path, ["a.png", "b.png", "c.png"])
        completed: list[object] = []
        finished: list[int] = []
        scan_service.scan_completed.connect(lambda *args: completed.append(args))
        scan_service.scan_finished.connect(lambda token: finished.append(token))

        def cancel_on_check(file_info: FileInfo) -> str:
            """Set the cancel event on the first render check, simulating a new scan request."""
            scan_service._cancel_event.set()
            return "queued"

        with patch.object(scan_service, "check_and_render", side_effect=cancel_on_check):
            task = _ScanFolderTask(
                folder_path=folder,
                token=7,
                service=scan_service,
                cancel_event=scan_service._cancel_event,
                supersede_event=threading.Event(),
            )
            task.run()

        # Stubs were written before the dispatch loop hit the cancel event.
        assert len(real_db.list_thumbnails_for_folder(str(folder))) == 3
        assert completed == []
        assert finished == [7]

    def test_request_returns_incrementing_tokens(
        self,
        scan_service: ThumbnailService,
        tmp_path: Path,
    ) -> None:
        """Each request_folder_scan call returns a fresh token carried by its signals."""
        folder = _make_folder(tmp_path, ["a.png"])
        started: list[tuple[int, str]] = []
        scan_service.scan_started.connect(lambda token, folder_path: started.append((token, folder_path)))

        token_a = scan_service.request_folder_scan(folder)
        token_b = scan_service.request_folder_scan(folder)

        assert token_b == token_a + 1
        assert started == [(token_a, str(folder)), (token_b, str(folder))]

    def test_request_resets_cancel_event_for_new_scan(self, scan_service: ThumbnailService) -> None:
        """Requesting a scan resets the cancel event so the new scan can dispatch renders."""
        scan_service._cancel_event.set()
        completed: list[object] = []
        scan_service.scan_completed.connect(lambda *args: completed.append(args))

        scan_service.request_folder_scan(Path("/nonexistent"))

        assert not scan_service._cancel_event.is_set()
        assert completed != []

    def test_superseded_scan_suppresses_completion(
        self,
        scan_service: ThumbnailService,
        real_db: Database,
        tmp_path: Path,
    ) -> None:
        """A scan superseded mid-dispatch writes stubs but never emits scan_completed."""
        folder = _make_folder(tmp_path, ["a.png", "b.png", "c.png"])
        completed: list[object] = []
        finished: list[int] = []
        scan_service.scan_completed.connect(lambda *args: completed.append(args))
        scan_service.scan_finished.connect(lambda token: finished.append(token))

        # Simulate a newer scan being requested while this task is dispatching renders.
        supersede_event = threading.Event()

        def supersede_on_check(file_info: FileInfo) -> str:
            """Set the supersede event on the first render check, like a new scan request does."""
            supersede_event.set()
            return "queued"

        with patch.object(scan_service, "check_and_render", side_effect=supersede_on_check):
            task = _ScanFolderTask(
                folder_path=folder,
                token=7,
                service=scan_service,
                cancel_event=scan_service._cancel_event,
                supersede_event=supersede_event,
            )
            task.run()

        # Stubs still land (queries see the folder), but the stale completion is suppressed.
        assert len(real_db.list_thumbnails_for_folder(str(folder))) == 3
        assert completed == []
        assert finished == [7]


class TestScanSupercedeRace:
    """A superseded scan can never emit scan_completed after a newer scan's completion."""

    def test_no_stale_scan_completed_after_supersede(
        self,
        real_db: Database,
        tmp_path: Path,
    ) -> None:
        """Repeated rapid folder switches deliver completions in strictly non-decreasing token order."""
        folder = _make_folder(tmp_path, ["a.png", "b.png", "c.png", "d.png", "e.png"])
        with patch("tarragon.services.thumbnail_service.get_executor"):
            svc = ThumbnailService(
                db=real_db,
                settings_service=_make_settings(),
                tag_service=MagicMock(),
                synchronous_workers=False,
            )

        started_seq: list[int] = []
        completed_seq: list[int] = []
        finished_seq: list[int] = []
        svc.scan_started.connect(lambda token, _folder: started_seq.append(token))
        svc.scan_completed.connect(lambda token, _infos: completed_seq.append(token))
        svc.scan_finished.connect(lambda token: finished_seq.append(token))

        try:
            with patch.object(svc, "check_and_render", return_value="queued"):
                for _ in range(10):
                    started_seq.clear()
                    completed_seq.clear()
                    finished_seq.clear()
                    # Two rapid folder switches: the second always supersedes the first.
                    svc.request_folder_scan(folder)
                    svc.request_folder_scan(folder)
                    last_token = svc._scan_token
                    assert _wait_until(lambda: last_token in finished_seq), "latest scan never finished"

                    # The harmful race is a completion delivered out of token order,
                    # i.e. the superseded scan's completion emitted after the newer
                    # scan's completion. Post-fix the completions are non-decreasing.
                    assert completed_seq == sorted(completed_seq), (
                        f"stale scan_completed after newer one: {completed_seq}"
                    )
                    assert all(token in started_seq for token in completed_seq)
                    assert len(completed_seq) == len(set(completed_seq))
                    assert last_token in completed_seq
        finally:
            svc._threadpool.waitForDone(5000)


class TestScanAsyncRealPool:
    """_ScanFolderTask under the real async path (genuine QThreadPool, seam off)."""

    def test_scan_runs_off_gui_thread_and_completes(
        self,
        real_db: Database,
        tmp_path: Path,
    ) -> None:
        """The scan body runs outside the GUI thread; scan_completed fires once with the FileInfo list."""
        folder = _make_folder(tmp_path, ["b.png", "a.png", "c.png"])
        with patch("tarragon.services.thumbnail_service.get_executor"):
            svc = ThumbnailService(
                db=real_db,
                settings_service=_make_settings(),
                tag_service=MagicMock(),
                synchronous_workers=False,
            )

        worker_threads: list[str] = []
        completed: list[tuple[int, list[FileInfo]]] = []
        finished: list[int] = []
        stub_counts: list[int] = []

        def spy_scan(folder_path: Path) -> list[FileInfo]:
            """Record the thread performing the scan, then delegate to the real scanner."""
            worker_threads.append(threading.current_thread().name)
            return scan_folder(folder_path)

        def on_completed(token: int, infos: object) -> None:
            completed.append((token, list(infos)))  # type: ignore[arg-type]
            stub_counts.append(len(real_db.list_thumbnails_for_folder(str(folder))))

        svc.scan_completed.connect(on_completed)
        svc.scan_finished.connect(lambda token: finished.append(token))

        try:
            with (
                patch("tarragon.services.thumbnail_service.scan_folder", side_effect=spy_scan),
                patch.object(svc, "check_and_render", return_value="queued"),
            ):
                token = svc.request_folder_scan(folder)
                delivered = _wait_until(lambda: token in finished)

            assert delivered, "async scan never finished"
            assert worker_threads, "scan worker thread never observed"
            main_thread = threading.current_thread().name
            assert all(thread != main_thread for thread in worker_threads)
            assert len(completed) == 1
            delivered_token, infos = completed[0]
            assert delivered_token == token
            assert [fi.path.name for fi in infos] == ["a.png", "b.png", "c.png"]
            # Stubs were written before scan_completed fired, so queries see results immediately.
            assert stub_counts == [3]
        finally:
            svc._threadpool.waitForDone(5000)
