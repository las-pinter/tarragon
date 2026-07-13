"""Tests for ThumbnailService — async orchestration for thumbnail generation.

WAAAGH! Gutslicka's torture chamber for da ThumbnailService! Every edge
case, every crash, every weird input — we break it ALL so da code gets
TOUGH!
"""

from __future__ import annotations

from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image

from tarragon.scanner import FileInfo
from tarragon.services.thumbnail_service import ThumbnailService
from tarragon.thumbnail import RESOLUTION_FULL, RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL

# =========================================================================
# Constants for edge case testing
# =========================================================================

SUPER_LONG_PATH = "/" + "a" * 4096  # Exceeds typical FS path limits
UNICODE_PATH = "照片/图像/画像/הוראה/ਤਸਵੀਰ/file.png"

# =========================================================================
# Fixtures
# =========================================================================


@pytest.fixture
def db_mock() -> MagicMock:
    """Mock Database with all CRUD methods as MagicMock."""
    mock = MagicMock()
    mock.get_folder_uuid.return_value = None
    mock.get_or_create_folder_uuid.return_value = "mock-uuid"
    return mock


@pytest.fixture
def settings_mock() -> MagicMock:
    """Mock SettingsService with typed return values matching DEFAULTS."""
    mock = MagicMock()
    mock.get_cache_format.return_value = "png"
    mock.get_max_psd_workers.return_value = 3
    mock.get_large_canvas_threshold_mp.return_value = 20.0
    mock.get_tile_grid_size.return_value = "2x2"
    mock.get_color_tag_enabled.return_value = True
    mock.get_color_tag_palette_size.return_value = 8
    mock.get_color_tag_min_share.return_value = 0.10
    mock.get_color_tag_neutral_s_threshold.return_value = 0.15
    return mock


@pytest.fixture
def service(db_mock: MagicMock, settings_mock: MagicMock) -> ThumbnailService:
    """Create a ThumbnailService with mocked DB, settings_service, and QThreadPool."""
    with patch("tarragon.services.thumbnail_service._get_executor"):
        svc = ThumbnailService(db=db_mock, settings_service=settings_mock)
    # Replace the real QThreadPool with a mock that executes tasks synchronously
    # so tests can verify render_func dispatch through check_and_render().
    # QThreadPool.start() returns None (void) in real Qt — mock matches that.
    mock_pool = MagicMock()
    mock_pool.start.side_effect = lambda task: _run_task(task)
    svc._threadpool = mock_pool
    return svc


def _run_task(task: object) -> None:
    """Execute a QRunnable's run() method synchronously for testing."""
    task.run()  # type: ignore[attr-defined]


# =========================================================================
# Instantiation
# =========================================================================


class TestInstantiation:
    """ThumbnailService creation and basic structure."""

    def test_service_instantiation(self, db_mock: MagicMock, settings_mock: MagicMock) -> None:
        """Creating a ThumbnailService stores dependencies, reads cache_format, and initializes PSD pool."""
        with patch("tarragon.services.thumbnail_service._get_executor") as mock_get_executor:
            svc = ThumbnailService(db=db_mock, settings_service=settings_mock)
        assert svc._db is db_mock
        assert svc._settings_service is settings_mock
        assert svc._cache_format == "png"
        settings_mock.get_cache_format.assert_called()
        settings_mock.get_max_psd_workers.assert_called()
        mock_get_executor.assert_called_once_with(max_workers=3)

    def test_signals_exist(self, service: ThumbnailService) -> None:
        """ThumbnailService exposes the required signals."""
        assert hasattr(service, "thumbnailReady")
        assert hasattr(service, "errorOccurred")

    def test_cache_format_from_settings(self, service: ThumbnailService) -> None:
        """_cache_format is initialised from settings_service."""
        assert service._cache_format == "png"


# =========================================================================
# check_and_render — cache hit / miss / stale / corrupt
# =========================================================================


class TestCheckAndRender:
    """check_and_render logic for different cache states."""

    def test_check_and_render_cache_hit(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Valid cache entry with all 3 resolution files → emit thumbnailReady for each."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        thumb_path = cache_dir / "thumb.png"
        preview_path = cache_dir / "preview.png"
        full_path = cache_dir / "full.png"
        # Write real images so Image.open succeeds
        ref_img = Image.new("RGB", (64, 64), color="red")
        ref_img.save(thumb_path)
        ref_img.save(preview_path)
        ref_img.save(full_path)

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 64,
            "height": 64,
            "cache_uuid": "test-uuid",
            "thumbnail_cache_path": str(thumb_path),
            "preview_cache_path": str(preview_path),
            "full_cache_path": str(full_path),
        }

        emitted: list[tuple[str, object, object]] = []
        service.thumbnailReady.connect(lambda p, i, r: emitted.append((p, i, r)))

        service.check_and_render(file_info)

        # Should emit 3 signals (one per resolution)
        assert len(emitted) == 3, f"Expected 3 emissions, got {len(emitted)}"
        assert emitted[0][0] == str(file_info.path)
        # Resolution sizes: RESOLUTION_THUMBNAIL, RESOLUTION_PREVIEW, RESOLUTION_FULL (full)
        resolution_sizes = [e[2] for e in emitted]
        assert RESOLUTION_THUMBNAIL in resolution_sizes
        assert RESOLUTION_PREVIEW in resolution_sizes
        assert RESOLUTION_FULL in resolution_sizes
        # No render dispatch since all cached
        db_mock.upsert_thumbnail.assert_not_called()

    def test_check_and_render_cache_miss(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """No cache entry → calls _render_all_resolutions."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_stale_cache(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry with mismatched mtime → calls _render_all_resolutions (stale)."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=2000.0,
            size=500,
            extension=".png",
        )
        # DB says mtime=1000 but file says 2000 → stale
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 64,
            "height": 64,
            "cache_uuid": "old-uuid",
            "thumbnail_cache_path": str(tmp_path / "cache" / "old.png"),
            "preview_cache_path": None,
            "full_cache_path": None,
        }

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_stale_cache_different_size(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry with mismatched size → calls _render_all_resolutions (stale)."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=999,
            extension=".png",
        )
        # DB says size=500 but file says 999 → stale
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 64,
            "height": 64,
            "cache_uuid": "old-uuid",
            "thumbnail_cache_path": str(tmp_path / "cache" / "old.png"),
            "preview_cache_path": None,
            "full_cache_path": None,
        }

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_corrupt_cache(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry exists but cache files missing → calls _render_all_resolutions."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        cache_path = cache_dir / "missing.png"
        # Do NOT create the file — it's missing/corrupt

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 64,
            "height": 64,
            "cache_uuid": "old-uuid",
            "thumbnail_cache_path": str(cache_path),
            "preview_cache_path": None,
            "full_cache_path": None,
        }

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            # Should call _render_all_resolutions since cache files are missing
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_no_cache_paths(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry without cache paths → calls _render_all_resolutions."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 64,
            "height": 64,
            "cache_uuid": "uuid-no-paths",
            "thumbnail_cache_path": None,
            "preview_cache_path": None,
            "full_cache_path": None,
        }

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_psd(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """.psd extension → calls _render_all_resolutions (which handles PSD internally)."""
        file_info = FileInfo(
            path=tmp_path / "document.psd",
            mtime=1000.0,
            size=500,
            extension=".psd",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_psb(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """.psb extension → calls _render_all_resolutions (which handles PSB internally)."""
        file_info = FileInfo(
            path=tmp_path / "big_document.psb",
            mtime=1000.0,
            size=500,
            extension=".psb",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_clip(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """.clip extension → calls _render_all_resolutions (which handles CLIP internally)."""
        file_info = FileInfo(
            path=tmp_path / "illustration.clip",
            mtime=1000.0,
            size=500,
            extension=".clip",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)


# =========================================================================
# Callback handlers
# =========================================================================


class TestCallbacks:
    """Internal callback methods (_on_error)."""

    def test_on_error_emits_both_signals(
        self,
        tmp_path: Path,
        service: ThumbnailService,
    ) -> None:
        """_on_error emits errorOccurred and thumbnailReady (with None image)."""
        file_info = FileInfo(
            path=tmp_path / "fail.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        ready_emitted: list[tuple[str, object]] = []
        error_emitted: list[tuple[str, str]] = []
        service.thumbnailReady.connect(lambda p, i: ready_emitted.append((p, i)))
        service.errorOccurred.connect(lambda p, e: error_emitted.append((p, e)))

        service._on_error(file_info, "Something went wrong")

        assert len(error_emitted) == 1
        assert error_emitted[0][0] == str(file_info.path)
        assert error_emitted[0][1] == "Something went wrong"

        assert len(ready_emitted) == 1
        assert ready_emitted[0][0] == str(file_info.path)
        assert ready_emitted[0][1] is None


# =========================================================================
# Edge cases — check_and_render wiv corrupt data, weird paths, format changes
# =========================================================================


class TestCheckAndRenderEdgeCases:
    """check_and_render wiv corrupt files, odd inputs, an' format switches."""

    def test_check_and_render_corrupt_cache_file_with_garbage(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache file exists but contains garbage bytes → Image.open raises → re-render."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        cache_path = cache_dir / "corrupt.png"
        # Write garbage data so Image.open raises an exception
        cache_path.write_bytes(b"\x00\x00\x00\x00\x00\x00\x00\x00NOT A VALID IMAGE\xff\xff\xff\xff")

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 64,
            "height": 64,
            "cache_uuid": "old-uuid",
            "thumbnail_cache_path": str(cache_path),
            "preview_cache_path": None,
            "full_cache_path": None,
        }

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            # Should call _render_all_resolutions since cache file is corrupt
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_cache_format_change_affects_render(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """After changing _cache_format, a cache miss still calls _render_all_resolutions."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        service._cache_format = "jpeg"

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_empty_extension_goes_plain(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """FileInfo wiv empty extension string → calls _render_all_resolutions."""
        file_info = FileInfo(
            path=tmp_path / "source",
            mtime=1000.0,
            size=500,
            extension="",  # No extension
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_unknown_extension_goes_plain(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """FileInfo wiv unsupported extension → calls _render_all_resolutions."""
        file_info = FileInfo(
            path=tmp_path / "source.bmp",
            mtime=1000.0,
            size=500,
            extension=".bmp",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_upper_case_psd(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Uppercase .PSD extension → calls _render_all_resolutions (case-insensitive)."""
        file_info = FileInfo(
            path=tmp_path / "document.PSD",
            mtime=1000.0,
            size=500,
            extension=".PSD",  # Uppercase
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_zero_mtime_and_size(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Edge case: mtime=0 and size=0 should not crash cache-hit check."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        cache_path = cache_dir / "thumb.png"
        ref_img = Image.new("RGB", (64, 64), color="red")
        ref_img.save(cache_path)

        file_info = FileInfo(
            path=tmp_path / "zero.png",
            mtime=0.0,
            size=0,
            extension=".png",
        )
        # DB also has 0/0 matching
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 0,
            "size": 0,
            "width": 64,
            "height": 64,
            "thumbnail_cache_path": str(cache_path),
        }

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service.check_and_render(file_info)

        # Should be a cache hit since mtime/size match
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        # Fallback path now dispatches render via threadpool (async)
        service._threadpool.start.assert_called_once()  # type: ignore[attr-defined]

    def test_check_and_render_db_returns_none_for_empty_path(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """FileInfo wiv empty Path — db.get_thumbnail called -> returns None -> render."""
        file_info = FileInfo(
            path=Path(""),
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            # Should not crash
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_check_and_render_special_chars_in_path(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Path wiv special characters like brackets, spaces, symbols — no crash."""
        file_info = FileInfo(
            path=tmp_path / "file (copy) [2024] #1 + $!.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)


# =========================================================================
# Edge cases — callback robustness wen fings go wrong
# =========================================================================


class TestCallbackEdgeCases:
    """Callback robustness: unicode paths."""

    def test_on_error_with_unicode_path(
        self,
        tmp_path: Path,
        service: ThumbnailService,
    ) -> None:
        """Unicode path in error signal — emitted correctly."""
        unicode_dir = tmp_path / "照片" / "图像"
        unicode_dir.mkdir(parents=True)
        file_path = unicode_dir / "画像.png"
        file_info = FileInfo(
            path=file_path,
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        ready_emitted: list[tuple[str, object]] = []
        error_emitted: list[tuple[str, str]] = []
        service.thumbnailReady.connect(lambda p, i: ready_emitted.append((p, i)))
        service.errorOccurred.connect(lambda p, e: error_emitted.append((p, e)))

        service._on_error(file_info, "Disk full")

        assert len(error_emitted) == 1
        assert error_emitted[0][0] == str(file_path)
        assert error_emitted[0][1] == "Disk full"

        assert len(ready_emitted) == 1
        assert ready_emitted[0][0] == str(file_path)
        assert ready_emitted[0][1] is None


# =========================================================================
# Cancellation — cancel_pending, reset_cancel, shutdown
# =========================================================================


class TestCancellation:
    """ThumbnailService cancellation: cancel_pending, reset_cancel, shutdown."""

    def test_cancel_event_initially_clear(
        self,
        service: ThumbnailService,
    ) -> None:
        """Cancel event is not set after construction."""
        assert not service._cancel_event.is_set()

    def test_cancel_pending_sets_event_and_clears_pool(
        self,
        service: ThumbnailService,
    ) -> None:
        """cancel_pending() sets the cancel event and clears the threadpool."""
        service.cancel_pending()
        assert service._cancel_event.is_set()
        service._threadpool.clear.assert_called_once()  # type: ignore[attr-defined]

    def test_reset_cancel_clears_event(
        self,
        service: ThumbnailService,
    ) -> None:
        """reset_cancel() clears the cancel event after cancel_pending()."""
        service.cancel_pending()
        assert service._cancel_event.is_set()
        service.reset_cancel()
        assert not service._cancel_event.is_set()

    def test_shutdown_calls_cancel_and_wait_for_done(
        self,
        service: ThumbnailService,
    ) -> None:
        """shutdown() cancels pending work and waits for the threadpool."""
        service.shutdown(timeout_ms=1000)
        assert service._cancel_event.is_set()
        service._threadpool.waitForDone.assert_called_once_with(1000)  # type: ignore[attr-defined]

    def test_shutdown_default_timeout(
        self,
        service: ThumbnailService,
    ) -> None:
        """shutdown() uses 5000 ms timeout by default."""
        service.shutdown()
        service._threadpool.waitForDone.assert_called_once_with(5000)  # type: ignore[attr-defined]


# =========================================================================
# Cancellation — _RenderAllTask honours cancel event
# =========================================================================


class TestRenderAllTaskCancellation:
    """_RenderAllTask checks cancel event before starting work."""

    def test_render_all_task_aborts_when_cancelled(
        self,
        tmp_path: Path,
    ) -> None:
        """_RenderAllTask.run() returns immediately when cancel event is set."""
        import threading

        from tarragon.services.thumbnail_service import _RenderAllTask

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_error = MagicMock()
        render_func = MagicMock()

        cancel_event = threading.Event()
        cancel_event.set()  # Already cancelled

        task = _RenderAllTask(
            file_info=file_info,
            on_error=on_error,
            render_func=render_func,
            cancel_event=cancel_event,
        )

        task.run()

        render_func.assert_not_called()
        on_error.assert_not_called()

    def test_render_all_task_runs_when_not_cancelled(
        self,
        tmp_path: Path,
    ) -> None:
        """_RenderAllTask.run() proceeds normally when cancel event is clear."""
        import threading

        from tarragon.services.thumbnail_service import _RenderAllTask

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_error = MagicMock()
        render_func = MagicMock()

        cancel_event = threading.Event()  # Not set

        task = _RenderAllTask(
            file_info=file_info,
            on_error=on_error,
            render_func=render_func,
            cancel_event=cancel_event,
        )

        task.run()

        render_func.assert_called_once_with(file_info)
        on_error.assert_not_called()

    def test_render_all_task_works_without_cancel_event(
        self,
        tmp_path: Path,
    ) -> None:
        """_RenderAllTask.run() works when cancel_event is None (backward compat)."""
        from tarragon.services.thumbnail_service import _RenderAllTask

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_error = MagicMock()
        render_func = MagicMock()

        task = _RenderAllTask(
            file_info=file_info,
            on_error=on_error,
            render_func=render_func,
            # cancel_event omitted (None)
        )

        task.run()

        render_func.assert_called_once_with(file_info)


# =========================================================================
# Cancellation — _render_all_resolutions honours cancel event
# =========================================================================


class TestRenderAllResolutionsCancellation:
    """_render_all_resolutions checks cancel event between steps."""

    def test_render_all_aborts_before_render_when_cancelled(
        self,
        tmp_path: Path,
        service: ThumbnailService,
    ) -> None:
        """_render_all_resolutions returns early when cancel event is set."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        # Set cancel before calling
        service._cancel_event.set()

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image") as mock_render,
            patch("tarragon.services.thumbnail_service.save_to_cache") as mock_save,
        ):
            service._render_all_resolutions(file_info)

        # Should NOT have called render or save
        mock_render.assert_not_called()
        mock_save.assert_not_called()

    def test_render_all_aborts_after_render_when_cancelled(
        self,
        tmp_path: Path,
        service: ThumbnailService,
    ) -> None:
        """_render_all_resolutions aborts after render if cancel is set mid-flight."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 64
        mock_img.height = 64

        def set_cancel_on_render(*args: object, **kwargs: object) -> MagicMock:
            """Simulate cancel being set during the render step."""
            service._cancel_event.set()
            return mock_img

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", side_effect=set_cancel_on_render),
            patch("tarragon.services.thumbnail_service.save_to_cache") as mock_save,
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # Should NOT have saved anything after render was cancelled
        mock_save.assert_not_called()

    def test_render_all_passes_cancel_event_to_psd_render(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        settings_mock: MagicMock,
    ) -> None:
        """_render_all_resolutions passes cancel_event to render_psd_image for PSD files."""
        file_info = FileInfo(
            path=tmp_path / "document.psd",
            mtime=1000.0,
            size=500,
            extension=".psd",
        )

        def set_cancel_and_return_none(*args: object, **kwargs: object) -> None:
            """Set cancel event during the PSD render call so subsequent steps abort."""
            service._cancel_event.set()

        with (
            patch(
                "tarragon.services.thumbnail_service.render_psd_image", side_effect=set_cancel_and_return_none
            ) as mock_psd,
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # Verify cancel_event was passed to render_psd_image
        mock_psd.assert_called_once()
        call_kwargs = mock_psd.call_args.kwargs
        assert call_kwargs.get("cancel_event") is service._cancel_event

    def test_render_all_calls_render_clip_image_for_clip_files(
        self,
        tmp_path: Path,
        service: ThumbnailService,
    ) -> None:
        """_render_all_resolutions calls render_clip_image (not render_plain_image) for .clip files."""
        file_info = FileInfo(
            path=tmp_path / "illustration.clip",
            mtime=1000.0,
            size=500,
            extension=".clip",
        )

        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 100
        mock_img.height = 80

        with (
            patch(
                "tarragon.services.thumbnail_service.render_clip_image",
                return_value=mock_img,
            ) as mock_clip,
            patch("tarragon.services.thumbnail_service.render_plain_image") as mock_plain,
            patch("tarragon.services.thumbnail_service.render_psd_image") as mock_psd,
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # render_clip_image should have been called with the file path and RESOLUTION_FULL
        mock_clip.assert_called_once_with(file_info.path, target_size=RESOLUTION_FULL)
        # render_plain_image and render_psd_image should NOT have been called
        mock_plain.assert_not_called()
        mock_psd.assert_not_called()


# =========================================================================
# Cancellation — render_psd_image honours cancel event
# =========================================================================


class TestRenderPsdImageCancellation:
    """render_psd_image polls cancel_event while waiting on the future."""

    def test_render_psd_returns_none_when_cancelled(
        self,
        tmp_path: Path,
    ) -> None:
        """render_psd_image returns None when cancel_event is set."""
        import threading
        from concurrent.futures import Future

        from tarragon.thumbnail import render_psd_image

        cancel_event = threading.Event()
        cancel_event.set()  # Already cancelled

        mock_future: Future[bytes | None] = Future()
        # Don't resolve the future — it should be cancelled before waiting

        with (
            patch("tarragon.renderers.psd._get_executor") as mock_exec,
        ):
            mock_executor = MagicMock()
            mock_executor.submit.return_value = mock_future
            mock_exec.return_value = mock_executor

            result = render_psd_image(
                tmp_path / "test.psd",
                20.0,
                2,
                2,
                cancel_event=cancel_event,
            )

        assert result is None
        # Future should have been cancelled
        assert mock_future.cancelled()

    def test_render_psd_works_without_cancel_event(
        self,
        tmp_path: Path,
    ) -> None:
        """render_psd_image works normally when cancel_event is None."""
        import io
        from concurrent.futures import Future

        from PIL import Image

        from tarragon.thumbnail import render_psd_image

        # Create a resolved future with valid PNG bytes
        img = Image.new("RGB", (10, 10), color="red")

        buf = io.BytesIO()
        img.save(buf, "PNG")
        png_bytes = buf.getvalue()

        mock_future: Future[bytes | None] = Future()
        mock_future.set_result(png_bytes)

        with patch("tarragon.renderers.psd._get_executor") as mock_exec:
            mock_executor = MagicMock()
            mock_executor.submit.return_value = mock_future
            mock_exec.return_value = mock_executor

            result = render_psd_image(
                tmp_path / "test.psd",
                20.0,
                2,
                2,
                cancel_event=None,
            )

        assert result is not None
        assert result.size == (10, 10)


# =========================================================================
# Cancellation — check_and_render passes cancel_event to _RenderAllTask
# =========================================================================


class TestCheckAndRenderCancellation:
    """check_and_render wires cancel_event into _RenderAllTask."""

    def test_check_and_render_passes_cancel_event(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """check_and_render creates _RenderAllTask with the service's cancel_event."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        with patch("tarragon.services.thumbnail_service._RenderAllTask") as mock_task_cls:
            mock_task = MagicMock()
            mock_task_cls.return_value = mock_task

            service.check_and_render(file_info)

        # Verify _RenderAllTask was constructed with cancel_event
        mock_task_cls.assert_called_once()
        call_kwargs = mock_task_cls.call_args.kwargs
        assert call_kwargs.get("cancel_event") is service._cancel_event

    def test_cancel_prevents_stale_signals_on_folder_switch(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """After cancel_pending(), _render_all_resolutions aborts (no stale signals)."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        # Simulate folder switch: cancel then reset
        service.cancel_pending()
        service.reset_cancel()

        # Now check_and_render should work normally (cancel was reset)
        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)


# =========================================================================
# _derive_missing_resolutions — small image caching
# =========================================================================


class TestDeriveMissingResolutionsSmallImages:
    """Verify that small images are cached as-is in ALL resolution tiers."""

    def test_small_image_cached_in_all_tiers(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Source image smaller than 256px is saved as-is to 256, 1024, and full caches."""
        # Create a small full-resolution cached image (100x100)
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        full_path = cache_dir / "full.png"
        small_img = Image.new("RGB", (100, 100), color="red")
        small_img.save(full_path)

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        # Cached dict: full exists, but 256 and 1024 are missing
        cached = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 100,
            "height": 100,
            "cache_uuid": "test-uuid",
            "thumbnail_cache_path": None,
            "preview_cache_path": None,
            "full_cache_path": str(full_path),
        }

        emitted: list[tuple[Any, ...]] = []
        service.thumbnailReady.connect(lambda *args: emitted.append(args))

        result = service._derive_missing_resolutions(file_info, cached)

        assert result == "derived"
        # Both 256 and 1024 should have been saved
        resolution_sizes = [e[2] for e in emitted]
        assert RESOLUTION_THUMBNAIL in resolution_sizes, "256 tier should be populated for small images"
        assert RESOLUTION_PREVIEW in resolution_sizes, "1024 tier should be populated for small images"
        # DB should have been updated with all paths
        db_mock.upsert_thumbnail.assert_called_once()
        call_kwargs = db_mock.upsert_thumbnail.call_args.kwargs
        assert call_kwargs["thumbnail_cache_path"] is not None
        assert call_kwargs["preview_cache_path"] is not None

    def test_medium_image_cached_in_1024_tier(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Source image between 256 and 1024px is saved as-is to 1024 cache."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        full_path = cache_dir / "full.png"
        medium_img = Image.new("RGB", (500, 400), color="blue")
        medium_img.save(full_path)

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        cached = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 500,
            "height": 400,
            "cache_uuid": "test-uuid",
            "thumbnail_cache_path": None,
            "preview_cache_path": None,
            "full_cache_path": str(full_path),
        }

        emitted: list[tuple[Any, ...]] = []
        service.thumbnailReady.connect(lambda *args: emitted.append(args))

        result = service._derive_missing_resolutions(file_info, cached)

        assert result == "derived"
        resolution_sizes = [e[2] for e in emitted]
        # 256 should be derived (500 > 256 → resized), 1024 should be included as-is
        assert RESOLUTION_THUMBNAIL in resolution_sizes
        assert RESOLUTION_PREVIEW in resolution_sizes, "1024 tier should be populated for medium images"

    def test_small_image_not_upscaled_in_cache(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Small image saved to 1024 cache retains original dimensions (no upscaling)."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        full_path = cache_dir / "full.png"
        small_img = Image.new("RGB", (200, 150), color="green")
        small_img.save(full_path)

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        cached = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 200,
            "height": 150,
            "cache_uuid": "test-uuid",
            "thumbnail_cache_path": None,
            "preview_cache_path": None,
            "full_cache_path": str(full_path),
        }

        emitted: list[tuple[Any, ...]] = []
        service.thumbnailReady.connect(lambda *args: emitted.append(args))

        service._derive_missing_resolutions(file_info, cached)

        # Find the 1024 emission and verify the image was NOT upscaled
        for emission in emitted:
            if emission[2] == RESOLUTION_PREVIEW:
                cached_preview_img = emission[1]
                assert cached_preview_img.size == (200, 150), (
                    f"Small image should NOT be upscaled to 1024, got size {cached_preview_img.size}"
                )
                break
        else:
            pytest.fail("No emission found for 1024 resolution tier")


# =========================================================================
# Auto-color tag signal — Bug 1 regression test
# =========================================================================


class TestAutoColorTagSignal:
    """Verify that auto-color tagging emits tagsUpdated signal."""

    def test_tags_updated_signal_exists(self, service: ThumbnailService) -> None:
        """ThumbnailService exposes a tagsUpdated signal."""
        assert hasattr(service, "tagsUpdated")

    def test_render_all_emits_tags_updated_when_color_tagging_enabled(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
        settings_mock: MagicMock,
    ) -> None:
        """_render_all_resolutions emits tagsUpdated after persisting auto-color tags."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        emitted = []
        service.tagsUpdated.connect(lambda: emitted.append(True))

        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 64
        mock_img.height = 64

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=mock_img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
            patch("tarragon.services.color_tagger.extract_dominant_color_tags", return_value=["red", "blue"]),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # tagsUpdated should have been emitted exactly once
        assert len(emitted) == 1, f"tagsUpdated should be emitted once after auto-tagging, got {len(emitted)}"
        # DB method should have been called
        db_mock.replace_auto_color_tags.assert_called_once_with(str(file_info.path), ["red", "blue"])

    def test_render_all_no_tags_updated_when_color_tagging_disabled(
        self,
        tmp_path: Path,
        db_mock: MagicMock,
    ) -> None:
        """When color_tag_enabled is False, tagsUpdated is NOT emitted."""
        disabled_settings = MagicMock()
        disabled_settings.get_cache_format.return_value = "png"
        disabled_settings.get_max_psd_workers.return_value = 3
        disabled_settings.get_large_canvas_threshold_mp.return_value = 20.0
        disabled_settings.get_tile_grid_size.return_value = "2x2"
        disabled_settings.get_color_tag_enabled.return_value = False  # Disabled!

        with patch("tarragon.services.thumbnail_service._get_executor"):
            svc = ThumbnailService(db=db_mock, settings_service=disabled_settings)

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        emitted = []
        svc.tagsUpdated.connect(lambda: emitted.append(True))

        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 64
        mock_img.height = 64

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=mock_img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            svc._render_all_resolutions(file_info)

        # tagsUpdated should NOT have been emitted
        assert len(emitted) == 0, "tagsUpdated should NOT be emitted when color tagging is disabled"
        db_mock.replace_auto_color_tags.assert_not_called()

    def test_render_all_no_tags_updated_on_color_extraction_failure(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """When color extraction raises, tagsUpdated is NOT emitted (failure is swallowed)."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        emitted = []
        service.tagsUpdated.connect(lambda: emitted.append(True))

        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 64
        mock_img.height = 64

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=mock_img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
            patch(
                "tarragon.services.color_tagger.extract_dominant_color_tags",
                side_effect=RuntimeError("Color extraction failed"),
            ),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            # Should not raise — color tagging failure is swallowed
            service._render_all_resolutions(file_info)

        # tagsUpdated should NOT have been emitted since extraction failed
        assert len(emitted) == 0, "tagsUpdated should NOT be emitted when color extraction fails"


# =========================================================================
# Per-folder UUID — images from same folder share a cache UUID
# =========================================================================


class TestPerFolderUuid:
    """Verify that images from the same source folder share a cache UUID."""

    def test_render_all_uses_atomic_get_or_create(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """_render_all_resolutions uses get_or_create_folder_uuid atomically."""
        db_mock.get_or_create_folder_uuid.return_value = "existing-uuid"

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=MagicMock(spec=Image.Image)),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="candidate-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # Should have called the atomic get_or_create with folder path and a candidate UUID
        db_mock.get_or_create_folder_uuid.assert_called_once_with(str(tmp_path), "candidate-uuid")
        # generate_cache_paths should have been called with the returned UUID
        mock_paths.assert_called_once_with(file_info.path, "existing-uuid")

    def test_render_all_generates_candidate_uuid_for_atomic_call(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """_render_all_resolutions generates a candidate UUID and passes it to the atomic call."""
        db_mock.get_or_create_folder_uuid.return_value = "new-uuid-1"

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=MagicMock(spec=Image.Image)),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="new-uuid-1"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # Should have called the atomic method with the generated candidate UUID
        db_mock.get_or_create_folder_uuid.assert_called_once_with(str(tmp_path), "new-uuid-1")
        # generate_cache_paths should have been called with the UUID returned by the atomic call
        mock_paths.assert_called_once_with(file_info.path, "new-uuid-1")

    def test_two_images_same_folder_share_uuid(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Two images from the same folder use the same cache UUID via atomic call."""
        # The atomic method always returns the same UUID for the same folder
        db_mock.get_or_create_folder_uuid.return_value = "shared-uuid"

        file_a = FileInfo(path=tmp_path / "image_a.png", mtime=1000.0, size=500, extension=".png")
        file_b = FileInfo(path=tmp_path / "image_b.png", mtime=1000.0, size=600, extension=".png")

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=MagicMock(spec=Image.Image)),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="shared-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_a)
            service._render_all_resolutions(file_b)

        # Both calls to generate_cache_paths should use the same UUID
        assert mock_paths.call_count == 2
        assert mock_paths.call_args_list[0].args[1] == "shared-uuid"
        assert mock_paths.call_args_list[1].args[1] == "shared-uuid"
        # get_or_create_folder_uuid should have been called for both images
        assert db_mock.get_or_create_folder_uuid.call_count == 2


# =========================================================================
# Edge cases — ThumbnailService at the boundary
# =========================================================================


class TestThumbnailServiceEdgeCases:
    """Service-level edge cases: pool full, double start, config edge cases."""

    def test_render_all_resolutions_called_on_cache_miss(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache miss → _render_all_resolutions is called (not threadpool)."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info)
            mock_render.assert_called_once_with(file_info)

    def test_two_rapid_cache_misses_call_render_twice(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Two rapid check_and_render calls for different files → two render calls."""
        file_a = FileInfo(
            path=tmp_path / "alpha.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        file_b = FileInfo(
            path=tmp_path / "beta.png",
            mtime=2000.0,
            size=800,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_a)
            service.check_and_render(file_b)
            assert mock_render.call_count == 2
            mock_render.assert_any_call(file_a)
            mock_render.assert_any_call(file_b)

    def test_check_and_render_same_file_twice_cache_hit_then_miss(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Same file checked twice: first cache hit, second cache miss (mtime changes)."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        thumb_path = cache_dir / "thumb.png"
        preview_path = cache_dir / "preview.png"
        full_path = cache_dir / "full.png"
        ref_img = Image.new("RGB", (64, 64), color="red")
        ref_img.save(thumb_path)
        ref_img.save(preview_path)
        ref_img.save(full_path)

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        # First call — cache hit (all 3 files exist)
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 64,
            "height": 64,
            "cache_uuid": "test-uuid",
            "thumbnail_cache_path": str(thumb_path),
            "preview_cache_path": str(preview_path),
            "full_cache_path": str(full_path),
        }

        emitted: list[tuple[str, object, object]] = []
        service.thumbnailReady.connect(lambda p, i, r: emitted.append((p, i, r)))

        service.check_and_render(file_info)
        assert len(emitted) == 3, "First call should be a cache hit with 3 emissions"
        emitted.clear()

        # Second call — file's mtime changed (stale)
        file_info_updated = FileInfo(
            path=tmp_path / "source.png",
            mtime=2000.0,  # Changed!
            size=500,
            extension=".png",
        )

        with patch.object(service, "_render_all_resolutions") as mock_render:
            service.check_and_render(file_info_updated)
            mock_render.assert_called_once_with(file_info_updated)

    def test_signals_are_distinct_instances(
        self,
        service: ThumbnailService,
    ) -> None:
        """Each signal is a distinct Qt signal object."""
        assert service.thumbnailReady is not service.errorOccurred


# =========================================================================
# invalidate_cache_files — standalone helper in thumbnail.py
# =========================================================================


class TestInvalidateCacheFiles:
    """invalidate_cache_files deletes cache files and DB records."""

    def test_deletes_cache_files_from_disk(
        self,
        tmp_path: Path,
        db_mock: MagicMock,
    ) -> None:
        """Cache files for all 3 resolutions are deleted from disk."""
        from tarragon.thumbnail import invalidate_cache_files

        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        thumb_path = cache_dir / "thumb.png"
        preview_path = cache_dir / "preview.png"
        full_path = cache_dir / "full.png"
        # Create the files
        thumb_path.write_bytes(b"fake_thumb")
        preview_path.write_bytes(b"fake_preview")
        full_path.write_bytes(b"fake_full")

        source_path = str(tmp_path / "source.png")
        db_mock.get_thumbnail.return_value = {
            "thumbnail_cache_path": str(thumb_path),
            "preview_cache_path": str(preview_path),
            "full_cache_path": str(full_path),
        }

        invalidate_cache_files(db_mock, source_path)

        assert not thumb_path.exists()
        assert not preview_path.exists()
        assert not full_path.exists()
        db_mock.delete_thumbnail.assert_called_once_with(source_path)

    def test_no_db_record_is_noop(
        self,
        db_mock: MagicMock,
    ) -> None:
        """When no DB record exists, invalidate_cache_files is a no-op."""
        from tarragon.thumbnail import invalidate_cache_files

        db_mock.get_thumbnail.return_value = None

        invalidate_cache_files(db_mock, "/nonexistent/path.png")

        db_mock.delete_thumbnail.assert_not_called()

    def test_missing_cache_files_handled_gracefully(
        self,
        tmp_path: Path,
        db_mock: MagicMock,
    ) -> None:
        """Cache files that don't exist on disk don't cause errors (missing_ok=True)."""
        from tarragon.thumbnail import invalidate_cache_files

        source_path = str(tmp_path / "source.png")
        db_mock.get_thumbnail.return_value = {
            "thumbnail_cache_path": str(tmp_path / "missing_thumb.png"),
            "preview_cache_path": str(tmp_path / "missing_preview.png"),
            "full_cache_path": str(tmp_path / "missing_full.png"),
        }

        # Should not raise
        invalidate_cache_files(db_mock, source_path)

        db_mock.delete_thumbnail.assert_called_once_with(source_path)

    def test_partial_cache_paths_only_deletes_existing(
        self,
        tmp_path: Path,
        db_mock: MagicMock,
    ) -> None:
        """Only non-None cache paths are processed for deletion."""
        from tarragon.thumbnail import invalidate_cache_files

        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        thumb_path = cache_dir / "thumb.png"
        thumb_path.write_bytes(b"fake_thumb")

        source_path = str(tmp_path / "source.png")
        db_mock.get_thumbnail.return_value = {
            "thumbnail_cache_path": str(thumb_path),
            "preview_cache_path": None,
            "full_cache_path": None,
        }

        invalidate_cache_files(db_mock, source_path)

        assert not thumb_path.exists()
        db_mock.delete_thumbnail.assert_called_once_with(source_path)


# =========================================================================
# invalidate_and_render — ThumbnailService method
# =========================================================================


class TestInvalidateAndRender:
    """invalidate_and_render deletes cache and re-renders from source."""

    def test_invalidates_cache_and_calls_check_and_render(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """invalidate_and_render deletes cache files and calls check_and_render."""
        # Create a source file on disk
        source_file = tmp_path / "source.png"
        source_file.write_bytes(b"fake_image_data")

        # Create cache files
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        thumb_path = cache_dir / "thumb.png"
        preview_path = cache_dir / "preview.png"
        full_path = cache_dir / "full.png"
        thumb_path.write_bytes(b"fake_thumb")
        preview_path.write_bytes(b"fake_preview")
        full_path.write_bytes(b"fake_full")

        db_mock.get_thumbnail.return_value = {
            "thumbnail_cache_path": str(thumb_path),
            "preview_cache_path": str(preview_path),
            "full_cache_path": str(full_path),
        }

        with patch.object(service, "check_and_render", return_value="queued") as mock_render:
            service.invalidate_and_render(source_file)

        # Cache files should be deleted
        assert not thumb_path.exists()
        assert not preview_path.exists()
        assert not full_path.exists()
        # DB record should be deleted
        db_mock.delete_thumbnail.assert_called_once_with(str(source_file))
        # check_and_render should be called with correct FileInfo
        mock_render.assert_called_once()
        call_args = mock_render.call_args[0][0]
        assert isinstance(call_args, FileInfo)
        assert call_args.path == source_file

    def test_no_cache_exists_still_renders(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """invalidate_and_render works when no cache exists (graceful handling)."""
        source_file = tmp_path / "source.png"
        source_file.write_bytes(b"fake_image_data")

        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "check_and_render", return_value="queued") as mock_render:
            service.invalidate_and_render(source_file)

        # delete_thumbnail should NOT be called since no record exists
        db_mock.delete_thumbnail.assert_not_called()
        # check_and_render should still be called
        mock_render.assert_called_once()

    def test_missing_source_file_does_not_render(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """invalidate_and_render does not render when source file doesn't exist."""
        nonexistent = tmp_path / "does_not_exist.png"

        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "check_and_render") as mock_render:
            service.invalidate_and_render(nonexistent)

        # check_and_render should NOT be called since source file doesn't exist
        mock_render.assert_not_called()

    def test_file_info_has_correct_metadata(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """invalidate_and_render creates FileInfo with correct mtime, size, extension."""
        source_file = tmp_path / "photo.PSD"
        source_file.write_bytes(b"fake_psd_data")

        db_mock.get_thumbnail.return_value = None

        with patch.object(service, "check_and_render", return_value="queued") as mock_render:
            service.invalidate_and_render(source_file)

        call_args = mock_render.call_args[0][0]
        assert call_args.path == source_file
        assert call_args.extension == ".psd"  # lowercase
        assert call_args.size == source_file.stat().st_size
        assert call_args.mtime == source_file.stat().st_mtime
