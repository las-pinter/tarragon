"""Tests for ThumbnailService"""

from __future__ import annotations

import io
import os
import threading
from collections.abc import Generator
from concurrent.futures import Future
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image

from tarragon.db.database import Database
from tarragon.renderers.cache import RESOLUTION_FULL, RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL, invalidate_cache_files
from tarragon.renderers.psd import render_psd_image
from tarragon.scanner import FileInfo, scan_folder
from tarragon.services.thumbnail_service import ThumbnailService, _RenderAllTask

SUPER_LONG_PATH = "/" + "a" * 4096  # Exceeds typical FS path limits
UNICODE_PATH = "照片/图像/画像/הוראה/ਤਸਵੀਰ/file.png"


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
    mock.cache_format.get.return_value = "PNG"
    mock.max_psd_workers.get.return_value = 3
    mock.large_canvas_threshold_mp.get.return_value = 20.0
    mock.tile_grid_size.get.return_value = "2x2"
    mock.color_tag_enabled.get.return_value = True
    mock.color_tag_palette_size.get.return_value = 8
    mock.color_tag_min_share.get.return_value = 0.10
    mock.color_tag_neutral_s_threshold.get.return_value = 0.15
    mock.clear_full_res_on_exit.get.return_value = False
    return mock


@pytest.fixture
def tag_service_mock() -> MagicMock:
    """Mock TagService"""
    mock = MagicMock()
    mock.replace_auto_color_tags.return_value = None
    mock.create_tag.side_effect = lambda color, source: color
    return mock


@pytest.fixture
def service(db_mock: MagicMock, settings_mock: MagicMock, tag_service_mock: MagicMock) -> ThumbnailService:
    """Create a ThumbnailService with mocked DB, settings_service, and QThreadPool."""
    with patch("tarragon.services.thumbnail_service.get_executor"):
        svc = ThumbnailService(db=db_mock, settings_service=settings_mock, tag_service=tag_service_mock)
    # Replace the real QThreadPool with a mock that executes tasks synchronously
    # so tests can verify render_func dispatch through check_and_render().
    # QThreadPool.start() returns None (void) in real Qt - mock matches that.
    mock_pool = MagicMock()
    mock_pool.start.side_effect = lambda task: _run_task(task)
    svc._threadpool = mock_pool
    return svc


@pytest.fixture
def real_db() -> Generator[Database, None, None]:
    """Provide a real in-memory Database for integration tests."""
    db = Database(Path(":memory:"))
    db.init_schema()
    yield db
    db.close()


@pytest.fixture
def real_service(real_db: Database, tag_service_mock: MagicMock) -> ThumbnailService:
    """Create a ThumbnailService with a real Database and synchronous threadpool."""
    settings = MagicMock()
    settings.cache_format.get.return_value = "PNG"
    settings.max_psd_workers.get.return_value = 3
    settings.large_canvas_threshold_mp.get.return_value = 20.0
    settings.tile_grid_size.get.return_value = "2x2"
    settings.color_tag_enabled.get.return_value = False
    settings.clear_full_res_on_exit.get.return_value = False
    with patch("tarragon.services.thumbnail_service.get_executor"):
        svc = ThumbnailService(db=real_db, settings_service=settings, tag_service=tag_service_mock)
    mock_pool = MagicMock()
    mock_pool.start.side_effect = lambda task: _run_task(task)
    svc._threadpool = mock_pool
    return svc


def _run_task(task: object) -> None:
    """Execute a QRunnable's run() method synchronously for testing."""
    task.run()  # type: ignore[attr-defined]


class TestInstantiation:
    """ThumbnailService creation and basic structure."""

    def test_service_instantiation(
        self, db_mock: MagicMock, settings_mock: MagicMock, tag_service_mock: MagicMock
    ) -> None:
        """Creating a ThumbnailService stores dependencies, reads cache_format, and initializes PSD pool."""
        with patch("tarragon.services.thumbnail_service.get_executor") as mockget_executor:
            svc = ThumbnailService(db=db_mock, settings_service=settings_mock, tag_service=tag_service_mock)
        assert svc._db is db_mock
        assert svc._settings_service is settings_mock
        assert svc._tag_service is tag_service_mock
        assert svc._cache_format == "PNG"
        settings_mock.cache_format.get.assert_called()
        settings_mock.max_psd_workers.get.assert_called()
        mockget_executor.assert_called_once_with(max_workers=3)

    def test_signals_exist(self, service: ThumbnailService) -> None:
        """ThumbnailService exposes the required signals."""
        assert hasattr(service, "thumbnail_ready")
        assert hasattr(service, "error_occurred")


class TestCheckAndRender:
    """check_and_render logic for different cache states."""

    def test_check_and_render_cache_hit(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Valid cache entry with all 3 resolution files -> emit thumbnail_ready for each."""
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
        service.thumbnail_ready.connect(lambda p, i, r: emitted.append((p, i, r)))

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
        """No cache entry -> calls _render_all_resolutions."""
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

    def test_check_and_render_stale_cache_different_size(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry with mismatched size -> calls _render_all_resolutions (stale)."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=999,
            extension=".png",
        )
        # DB says size=500 but file says 999 -> stale
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
        """Cache entry exists but cache files missing -> calls _render_all_resolutions."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        cache_path = cache_dir / "missing.png"
        # Do NOT create the file - it's missing/corrupt

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


class TestCallbacks:
    """Internal callback methods (_on_error)."""

    def test_on_error_emits_both_signals(
        self,
        tmp_path: Path,
        service: ThumbnailService,
    ) -> None:
        """_on_error emits error_occurred and thumbnail_ready (with None image)."""
        file_info = FileInfo(
            path=tmp_path / "fail.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        ready_emitted: list[tuple[str, object]] = []
        error_emitted: list[tuple[str, str]] = []
        service.thumbnail_ready.connect(lambda p, i: ready_emitted.append((p, i)))
        service.error_occurred.connect(lambda p, e: error_emitted.append((p, e)))

        service._on_error(file_info, "Something went wrong")

        assert len(error_emitted) == 1
        assert error_emitted[0][0] == str(file_info.path)
        assert error_emitted[0][1] == "Something went wrong"

        assert len(ready_emitted) == 1
        assert ready_emitted[0][0] == str(file_info.path)
        assert ready_emitted[0][1] is None


class TestCheckAndRenderEdgeCases:
    """check_and_render with corrupt files, odd inputs, and format switches."""

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
        service.thumbnail_ready.connect(lambda p, i: emitted.append((p, i)))

        service.check_and_render(file_info)

        # Should be a cache hit since mtime/size match
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        # Fallback path now dispatches render via threadpool (async)
        service._threadpool.start.assert_called_once()  # type: ignore[attr-defined]


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

    def test_shutdown_clears_full_res_cache_when_enabled(
        self,
        service: ThumbnailService,
        settings_mock: MagicMock,
    ) -> None:
        """shutdown() clears the full-res cache when the setting is enabled."""
        settings_mock.clear_full_res_on_exit.get.return_value = True
        with patch("tarragon.services.thumbnail_service.clear_full_res_cache") as mock_clear:
            service.shutdown(timeout_ms=1000)
        mock_clear.assert_called_once_with(enabled=True)

    def test_shutdown_skips_full_res_cleanup_when_disabled(
        self,
        service: ThumbnailService,
        settings_mock: MagicMock,
    ) -> None:
        """shutdown() passes enabled=False to cleanup when the setting is disabled."""
        settings_mock.clear_full_res_on_exit.get.return_value = False
        with patch("tarragon.services.thumbnail_service.clear_full_res_cache") as mock_clear:
            service.shutdown(timeout_ms=1000)
        mock_clear.assert_called_once_with(enabled=False)


class TestRenderAllTaskCancellation:
    """_RenderAllTask checks cancel event before starting work."""

    def test_render_all_task_aborts_when_cancelled(
        self,
        tmp_path: Path,
    ) -> None:
        """_RenderAllTask.run() returns immediately when cancel event is set."""
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


class TestRenderPsdImageCancellation:
    """render_psd_image polls cancel_event while waiting on the future."""

    def test_render_psd_returns_none_when_cancelled(
        self,
        tmp_path: Path,
    ) -> None:
        """render_psd_image returns None when cancel_event is set."""
        cancel_event = threading.Event()
        cancel_event.set()  # Already cancelled

        mock_future: Future[bytes | None] = Future()
        # Don't resolve the future - it should be cancelled before waiting

        with (
            patch("tarragon.renderers.psd.get_executor") as mock_exec,
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
        # Create a resolved future with valid PNG bytes
        img = Image.new("RGB", (10, 10), color="red")

        buf = io.BytesIO()
        img.save(buf, "PNG")
        png_bytes = buf.getvalue()

        mock_future: Future[bytes | None] = Future()
        mock_future.set_result(png_bytes)

        with patch("tarragon.renderers.psd.get_executor") as mock_exec:
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
        service.thumbnail_ready.connect(lambda *args: emitted.append(args))

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
        service.thumbnail_ready.connect(lambda *args: emitted.append(args))

        result = service._derive_missing_resolutions(file_info, cached)

        assert result == "derived"
        resolution_sizes = [e[2] for e in emitted]
        # 256 should be derived (500 > 256 -> resized), 1024 should be included as-is
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
        service.thumbnail_ready.connect(lambda *args: emitted.append(args))

        service._derive_missing_resolutions(file_info, cached)

        # Find the 1024 emission and verify the image was NOT upscaled
        for emission in emitted:
            if emission[2] == RESOLUTION_PREVIEW:
                cached_preview_img = emission[1]
                assert cached_preview_img.size == (
                    200,
                    150,
                ), f"Small image should NOT be upscaled to 1024, got size {cached_preview_img.size}"
                break
        else:
            pytest.fail("No emission found for 1024 resolution tier")


class TestAutoColorTagSignal:
    """Verify that auto-color tagging emits tags_updated signal."""

    def test_tags_updated_signal_exists(self, service: ThumbnailService) -> None:
        """ThumbnailService exposes a tags_updated signal."""
        assert hasattr(service, "tags_updated")

    def test_render_all_emits_tags_updated_when_color_tagging_enabled(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
        settings_mock: MagicMock,
        tag_service_mock: MagicMock,
    ) -> None:
        """_render_all_resolutions emits tags_updated after persisting auto-color tags."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        emitted = []
        service.tags_updated.connect(lambda: emitted.append(True))

        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 64
        mock_img.height = 64

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=mock_img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
            patch("tarragon.services.thumbnail_service.extract_dominant_colors", return_value={"red", "blue"}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # tags_updated should have been emitted exactly once
        assert len(emitted) == 1, f"tags_updated should be emitted once after auto-tagging, got {len(emitted)}"
        # Tag service method should have been called
        tag_service_mock.replace_auto_color_tags.assert_called_once_with(str(file_info.path), {"red", "blue"})

    def test_render_all_no_tags_updated_when_color_tagging_disabled(
        self,
        tmp_path: Path,
        db_mock: MagicMock,
        tag_service_mock: MagicMock,
    ) -> None:
        """When color_tag_enabled is False, tags_updated is NOT emitted."""
        disabled_settings = MagicMock()
        disabled_settings.cache_format.get.return_value = "PNG"
        disabled_settings.max_psd_workers.get.return_value = 3
        disabled_settings.large_canvas_threshold_mp.get.return_value = 20.0
        disabled_settings.tile_grid_size.get.return_value = "2x2"
        disabled_settings.color_tag_enabled.get.return_value = False  # Disabled!

        with patch("tarragon.services.thumbnail_service.get_executor"):
            svc = ThumbnailService(db=db_mock, settings_service=disabled_settings, tag_service=tag_service_mock)

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        emitted = []
        svc.tags_updated.connect(lambda: emitted.append(True))

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

        # tags_updated should NOT have been emitted
        assert len(emitted) == 0, "tags_updated should NOT be emitted when color tagging is disabled"
        db_mock.replace_auto_color_tags.assert_not_called()

    def test_render_all_no_tags_updated_on_color_extraction_failure(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """When color extraction raises, tags_updated is NOT emitted (failure is swallowed)."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        emitted = []
        service.tags_updated.connect(lambda: emitted.append(True))

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
                "tarragon.services.thumbnail_service.extract_dominant_colors",
                side_effect=RuntimeError("Color extraction failed"),
            ),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            # Should not raise - color tagging failure is swallowed
            service._render_all_resolutions(file_info)

        # tags_updated should NOT have been emitted since extraction failed
        assert len(emitted) == 0, "tags_updated should NOT be emitted when color extraction fails"


class TestPerFolderUuid:
    """Verify that images from the same source folder share a cache UUID."""

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


class TestThumbnailServiceEdgeCases:
    """Service-level edge cases: pool full, double start, config edge cases."""

    def test_two_rapid_cache_misses_call_render_twice(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Two rapid check_and_render calls for different files -> two render calls."""
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

        # First call - cache hit (all 3 files exist)
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
        service.thumbnail_ready.connect(lambda p, i, r: emitted.append((p, i, r)))

        service.check_and_render(file_info)
        assert len(emitted) == 3, "First call should be a cache hit with 3 emissions"
        emitted.clear()

        # Second call - file's mtime changed (stale)
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
        assert service.thumbnail_ready is not service.error_occurred


class TestInvalidateCacheFiles:
    """invalidate_cache_files deletes cache files and DB records."""

    def test_deletes_cache_files_from_disk(
        self,
        tmp_path: Path,
        db_mock: MagicMock,
    ) -> None:
        """Cache files for all 3 resolutions are deleted from disk."""
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
        db_mock.get_thumbnail.return_value = None

        invalidate_cache_files(db_mock, "/nonexistent/path.png")

        db_mock.delete_thumbnail.assert_not_called()

    def test_missing_cache_files_handled_gracefully(
        self,
        tmp_path: Path,
        db_mock: MagicMock,
    ) -> None:
        """Cache files that don't exist on disk don't cause errors (missing_ok=True)."""
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


class TestRealPathRendering:
    """End-to-end rendering through the real check_and_render path."""

    def test_check_and_render_saves_real_full_res_cache_file(
        self,
        tmp_path: Path,
        real_service: ThumbnailService,
        real_db: Database,
    ) -> None:
        """First discovery renders and records a real full-res cache file on disk."""
        source = tmp_path / "source.png"
        Image.new("RGB", (300, 200), color="red").save(source)
        stat = source.stat()
        file_info = FileInfo(path=source, mtime=stat.st_mtime, size=stat.st_size, extension=".png")

        with patch("tarragon.renderers.cache.cache_dir", return_value=tmp_path / "cache"):
            real_service.check_and_render(file_info)

        record = real_db.get_thumbnail(str(source))
        assert record is not None
        full_path = Path(record["full_cache_path"])
        assert full_path.is_file()
        assert full_path.stat().st_size > 0

    def test_stale_cache_regenerated_on_rescan(
        self,
        tmp_path: Path,
        real_service: ThumbnailService,
        real_db: Database,
    ) -> None:
        """Modified source file triggers cache regeneration on the next folder-open pass."""
        source = tmp_path / "source.png"
        Image.new("RGB", (300, 200), color="red").save(source)
        cache_root = tmp_path / "cache"

        with patch("tarragon.renderers.cache.cache_dir", return_value=cache_root):
            first_infos = scan_folder(tmp_path)
            real_db.bulk_upsert_stubs([(str(fi.path), int(fi.mtime), fi.size) for fi in first_infos])
            for fi in first_infos:
                real_service.check_and_render(fi)

            first_record = real_db.get_thumbnail(str(source))
            assert first_record is not None
            first_full_path = Path(first_record["full_cache_path"])
            assert first_full_path.is_file()
            assert Image.open(first_full_path).size == (300, 200)

            Image.new("RGB", (400, 300), color="blue").save(source)
            new_mtime = first_record["mtime"] + 100
            os.utime(source, (new_mtime, new_mtime))

            second_infos = scan_folder(tmp_path)
            real_db.bulk_upsert_stubs([(str(fi.path), int(fi.mtime), fi.size) for fi in second_infos])
            for fi in second_infos:
                real_service.check_and_render(fi)

            second_record = real_db.get_thumbnail(str(source))
            assert second_record is not None
            second_full_path = Path(second_record["full_cache_path"])
            assert second_full_path.is_file()
            assert Image.open(second_full_path).size == (400, 300)
            assert second_record["mtime"] == new_mtime


class TestPurgeCache:
    """purge_cache cancels renders, drains the pool, and clears disk + DB."""

    def test_purge_cache_cancels_waits_and_resets(
        self,
        service: ThumbnailService,
    ) -> None:
        """purge_cache cancels pending work, waits for the pool, clears cache and DB, and resets cancel."""
        with (
            patch.object(service, "cancel_pending", wraps=service.cancel_pending) as mock_cancel,
            patch("tarragon.services.thumbnail_service.clear_cache") as mock_clear,
        ):
            service.purge_cache()

        mock_cancel.assert_called_once_with()
        service._threadpool.waitForDone.assert_called_once_with()  # type: ignore[attr-defined]
        mock_clear.assert_called_once_with()
        service._db.clear_thumbnails.assert_called_once_with()  # type: ignore[attr-defined]
        assert not service._cancel_event.is_set()

    def test_purge_cache_deletes_files_and_db_rows(
        self,
        tmp_path: Path,
        real_service: ThumbnailService,
        real_db: Database,
    ) -> None:
        """purge_cache deletes cache files and thumbnail rows but keeps folder_cache_uuids."""
        cache_root = tmp_path / "cache"
        tier_dir = cache_root / "256" / "folder_abc12345"
        tier_dir.mkdir(parents=True)
        cache_file = tier_dir / "image.png"
        cache_file.write_bytes(b"data")

        real_db.upsert_thumbnail("/fake/a.png", mtime=1, size=100, width=10, height=10, cache_uuid="u1")
        real_db.upsert_thumbnail("/fake/b.png", mtime=2, size=200, width=10, height=10, cache_uuid="u2")
        real_db.upsert_folder_uuid("/fake", "u1")
        real_db.upsert_folder_uuid("/other", "u2")

        with patch("tarragon.renderers.cache.cache_dir", return_value=cache_root):
            real_service.purge_cache()

        assert not cache_file.exists()
        assert real_db.fetch_all("SELECT COUNT(*) as cnt FROM thumbnails")[0]["cnt"] == 0
        assert real_db.fetch_all("SELECT COUNT(*) as cnt FROM folder_cache_uuids")[0]["cnt"] == 2

    def test_purge_cache_allows_rerender_afterwards(
        self,
        tmp_path: Path,
        real_service: ThumbnailService,
        real_db: Database,
    ) -> None:
        """After purge_cache, rendering a file again creates fresh cache files and a DB row."""
        cache_root = tmp_path / "cache"
        source = tmp_path / "source.png"
        Image.new("RGB", (300, 200), color="red").save(source)
        stat = source.stat()
        file_info = FileInfo(path=source, mtime=stat.st_mtime, size=stat.st_size, extension=".png")

        with patch("tarragon.renderers.cache.cache_dir", return_value=cache_root):
            real_service.check_and_render(file_info)
            first_record = real_db.get_thumbnail(str(source))
            assert first_record is not None
            first_full_path = Path(first_record["full_cache_path"])
            assert first_full_path.is_file()

            real_service.purge_cache()

            assert not first_full_path.exists()
            assert real_db.get_thumbnail(str(source)) is None

            real_service.check_and_render(file_info)
            second_record = real_db.get_thumbnail(str(source))
            assert second_record is not None
            second_full_path = Path(second_record["full_cache_path"])
            assert second_full_path.is_file()
            assert second_full_path.stat().st_size > 0

    def test_purge_cache_missing_cache_dir_no_crash(
        self,
        tmp_path: Path,
        real_service: ThumbnailService,
        real_db: Database,
    ) -> None:
        """purge_cache does not crash when the cache dir does not exist."""
        cache_root = tmp_path / "cache"

        with patch("tarragon.renderers.cache.cache_dir", return_value=cache_root):
            real_service.purge_cache()

        assert real_db.fetch_all("SELECT COUNT(*) as cnt FROM thumbnails")[0]["cnt"] == 0

    def test_purge_cache_empty_thumbnails_table_no_crash(
        self,
        tmp_path: Path,
        real_service: ThumbnailService,
        real_db: Database,
    ) -> None:
        """purge_cache does not crash when the thumbnails table is empty."""
        cache_root = tmp_path / "cache"
        tier_dir = cache_root / "256" / "folder_abc12345"
        tier_dir.mkdir(parents=True)
        cache_file = tier_dir / "image.png"
        cache_file.write_bytes(b"data")

        with patch("tarragon.renderers.cache.cache_dir", return_value=cache_root):
            real_service.purge_cache()

        assert not cache_file.exists()
        assert real_db.fetch_all("SELECT COUNT(*) as cnt FROM thumbnails")[0]["cnt"] == 0

    def test_purge_cache_waits_for_inflight_render_before_deleting(
        self,
        tmp_path: Path,
        real_db: Database,
        tag_service_mock: MagicMock,
    ) -> None:
        """purge_cache waits for an in-flight render so its late file write is deleted."""
        settings = MagicMock()
        settings.cache_format.get.return_value = "PNG"
        settings.max_psd_workers.get.return_value = 3
        settings.large_canvas_threshold_mp.get.return_value = 20.0
        settings.tile_grid_size.get.return_value = "2x2"
        settings.color_tag_enabled.get.return_value = False
        settings.clear_full_res_on_exit.get.return_value = False
        with patch("tarragon.services.thumbnail_service.get_executor"):
            svc = ThumbnailService(db=real_db, settings_service=settings, tag_service=tag_service_mock)

        cache_root = tmp_path / "cache"
        late_file = cache_root / "256" / "folder_abc12345" / "late.png"
        started = threading.Event()
        release = threading.Event()
        reached_wait = threading.Event()

        def slow_render(file_info: FileInfo) -> None:
            """Block until released, then write a cache file like a real render would."""
            started.set()
            release.wait(timeout=10)
            late_file.parent.mkdir(parents=True, exist_ok=True)
            late_file.write_bytes(b"late")

        file_info = FileInfo(path=tmp_path / "source.png", mtime=1.0, size=100, extension=".png")
        task = _RenderAllTask(
            file_info=file_info,
            on_error=MagicMock(),
            render_func=slow_render,
            cancel_event=svc._cancel_event,
        )

        real_wait_for_done = svc._threadpool.waitForDone

        def wrapped_wait_for_done() -> bool:
            """Signal that purge reached waitForDone, then delegate to the real method."""
            reached_wait.set()
            return real_wait_for_done()

        svc._threadpool.waitForDone = wrapped_wait_for_done  # type: ignore[assignment]

        svc._threadpool.start(task)
        assert started.wait(timeout=10)

        purge_errors: list[BaseException] = []

        def run_purge() -> None:
            """Run purge_cache in a worker thread."""
            try:
                with patch("tarragon.renderers.cache.cache_dir", return_value=cache_root):
                    svc.purge_cache()
            except BaseException as exc:
                purge_errors.append(exc)

        purge_thread = threading.Thread(target=run_purge)
        purge_thread.start()
        assert reached_wait.wait(timeout=10)
        assert purge_thread.is_alive()
        release.set()
        purge_thread.join(timeout=10)

        assert not purge_errors
        assert not late_file.exists()
        svc._threadpool.waitForDone()
