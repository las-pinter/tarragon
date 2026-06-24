"""Tests for ThumbnailService — async orchestration for thumbnail generation.

WAAAGH! Gutslicka's torture chamber for da ThumbnailService! Every edge
case, every crash, every weird input — we break it ALL so da code gets
TOUGH!
"""

from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image
from tarragon.scanner import FileInfo
from tarragon.services.thumbnail_service import (
    ThumbnailService,
    _RenderPSDTask,
    _RenderTask,
)

# =========================================================================
# Constants for edge case testing
# =========================================================================

SUPER_LONG_PATH = "/" + "a" * 4096  # Exceeds typical FS path limits
UNICODE_PATH = "照片/图像/画像/הוראה/ਤਸਵੀਰ/file.png"

# =========================================================================
# Fixtures
# =========================================================================


@pytest.fixture(autouse=True)
def qapp():
    """Provide a shared QApplication instance for Qt-based tests."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


@pytest.fixture
def db_mock() -> MagicMock:
    """Mock Database with all CRUD methods as MagicMock."""
    mock = MagicMock()
    return mock


@pytest.fixture
def settings_mock() -> MagicMock:
    """Mock Settings with a default cache_format of 'png'."""
    mock = MagicMock()
    mock.get.return_value = "png"
    return mock


@pytest.fixture
def service(db_mock: MagicMock, settings_mock: MagicMock) -> ThumbnailService:
    """Create a ThumbnailService with mocked DB, settings, and QThreadPool."""
    svc = ThumbnailService(db=db_mock, settings=settings_mock)
    # Replace the real QThreadPool with a mock so no real threads are started
    svc._threadpool = MagicMock()
    return svc


# =========================================================================
# Instantiation
# =========================================================================


class TestInstantiation:
    """ThumbnailService creation and basic structure."""

    def test_service_instantiation(self, db_mock: MagicMock, settings_mock: MagicMock) -> None:
        """Creating a ThumbnailService stores dependencies and reads cache_format."""
        svc = ThumbnailService(db=db_mock, settings=settings_mock)
        assert svc._db is db_mock
        assert svc._settings is settings_mock
        assert svc._cache_format == "png"
        settings_mock.get.assert_called_once_with("cache_format")

    def test_signals_exist(self, service: ThumbnailService) -> None:
        """ThumbnailService exposes the three required signals."""
        assert hasattr(service, "thumbnailReady")
        assert hasattr(service, "thumbnailsUpdated")
        assert hasattr(service, "errorOccurred")

    def test_set_cache_format(self, service: ThumbnailService) -> None:
        """set_cache_format updates the internal cache_format."""
        assert service._cache_format == "png"
        service.set_cache_format("jpeg")
        assert service._cache_format == "jpeg"

    def test_set_cache_format_invalid_raises(self, service: ThumbnailService) -> None:
        """set_cache_format with invalid format raises ValueError."""
        with pytest.raises(ValueError, match="Unknown cache_format: 'gif'.*"):
            service.set_cache_format("gif")
        # Internal format should remain unchanged
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
        """Valid cache entry → emit thumbnailReady immediately with cached image."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        cache_path = cache_dir / "thumb.png"
        # Write a real image so Image.open succeeds
        ref_img = Image.new("RGB", (64, 64), color="red")
        ref_img.save(cache_path)

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
            "master_cache_path": str(cache_path),
        }

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service.check_and_render(file_info)

        assert len(emitted) == 1, "Expected exactly one thumbnailReady emission"
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is not None
        # The cached image is returned directly without DB upsert or render dispatch
        db_mock.upsert_thumbnail.assert_not_called()
        service._threadpool.start.assert_not_called()

    def test_check_and_render_cache_miss(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """No cache entry → dispatch plain render via QThreadPool."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask)

    def test_check_and_render_stale_cache(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry with mismatched mtime → dispatch render (stale)."""
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
            "master_cache_path": str(tmp_path / "cache" / "old.png"),
        }

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask)

    def test_check_and_render_stale_cache_different_size(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry with mismatched size → dispatch render (stale)."""
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
            "master_cache_path": str(tmp_path / "cache" / "old.png"),
        }

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask)

    def test_check_and_render_corrupt_cache(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry exists but cache file missing → dispatch render."""
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
            "master_cache_path": str(cache_path),
        }

        service.check_and_render(file_info)

        # Should fall through to render dispatch since cache file is missing
        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask)

    def test_check_and_render_no_master_cache_path(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Cache entry without master_cache_path → dispatch render."""
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
            "master_cache_path": None,
        }

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask)

    def test_check_and_render_psd(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """.psd extension → dispatch PSD render (via _RenderPSDTask)."""
        file_info = FileInfo(
            path=tmp_path / "document.psd",
            mtime=1000.0,
            size=500,
            extension=".psd",
        )
        db_mock.get_thumbnail.return_value = None

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderPSDTask)

    def test_check_and_render_psb(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """.psb extension → dispatch PSD render (via _RenderPSDTask)."""
        file_info = FileInfo(
            path=tmp_path / "big_document.psb",
            mtime=1000.0,
            size=500,
            extension=".psb",
        )
        db_mock.get_thumbnail.return_value = None

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderPSDTask)


# =========================================================================
# _RenderTask — plain image rendering
# =========================================================================


class TestRenderTask:
    """_RenderTask runs plain-image rendering in a worker thread."""

    def test_render_task_runs_successfully(self, tmp_path: Path) -> None:
        """Successful render → calls on_done with the image."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        mock_img = MagicMock(spec=Image.Image)

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=mock_img) as mock_render,
            patch("tarragon.services.thumbnail_service._cache_file_path") as mock_cache_path,
            patch("tarragon.services.thumbnail_service.save_to_cache") as mock_save,
        ):
            mock_cache_path.return_value = (tmp_path / "cache" / "hash.png", "image/png")

            task.run()

        mock_render.assert_called_once_with(file_info.path)
        mock_cache_path.assert_called_once_with(file_info.path, "png")
        mock_save.assert_called_once_with(mock_img, tmp_path / "cache" / "hash.png", "png")
        on_done.assert_called_once_with(file_info, mock_img, tmp_path / "cache" / "hash.png")
        on_error.assert_not_called()

    def test_render_task_returns_none_no_save(self, tmp_path: Path) -> None:
        """render_plain_image returns None → no save_to_cache, on_done called with None."""
        file_info = FileInfo(
            path=tmp_path / "bad.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=None) as mock_render,
            patch("tarragon.services.thumbnail_service.save_to_cache") as mock_save,
        ):
            task.run()

        mock_render.assert_called_once_with(file_info.path)
        mock_save.assert_not_called()
        on_done.assert_called_once_with(file_info, None, None)
        on_error.assert_not_called()

    def test_render_task_handles_exception(self, tmp_path: Path) -> None:
        """Exception during render → calls on_error with message."""
        file_info = FileInfo(
            path=tmp_path / "crash.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        with patch("tarragon.services.thumbnail_service.render_plain_image") as mock_render:
            mock_render.side_effect = RuntimeError("Disk failure")

            task.run()

        on_error.assert_called_once_with(file_info, "Disk failure")
        on_done.assert_not_called()


# =========================================================================
# _RenderPSDTask — PSD/PSB rendering
# =========================================================================


class TestRenderPSDTask:
    """_RenderPSDTask runs PSD compositing in a worker thread."""

    def test_render_psd_task_runs_successfully(self, tmp_path: Path) -> None:
        """Successful PSD render → calls on_done with the image."""
        file_info = FileInfo(
            path=tmp_path / "doc.psd",
            mtime=1000.0,
            size=500,
            extension=".psd",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderPSDTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        mock_img = MagicMock(spec=Image.Image)

        with (
            patch("tarragon.services.thumbnail_service.render_psd_image", return_value=mock_img) as mock_render,
            patch("tarragon.services.thumbnail_service._cache_file_path") as mock_cache_path,
            patch("tarragon.services.thumbnail_service.save_to_cache") as mock_save,
        ):
            mock_cache_path.return_value = (tmp_path / "cache" / "hash.png", "image/png")

            task.run()

        mock_render.assert_called_once_with(file_info.path)
        mock_cache_path.assert_called_once_with(file_info.path, "png")
        mock_save.assert_called_once_with(mock_img, tmp_path / "cache" / "hash.png", "png")
        on_done.assert_called_once_with(file_info, mock_img, tmp_path / "cache" / "hash.png")
        on_error.assert_not_called()

    def test_render_psd_task_returns_none_no_save(self, tmp_path: Path) -> None:
        """render_psd_image returns None → no save_to_cache, on_done with None."""
        file_info = FileInfo(
            path=tmp_path / "empty.psd",
            mtime=1000.0,
            size=500,
            extension=".psd",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderPSDTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        with (
            patch("tarragon.services.thumbnail_service.render_psd_image", return_value=None) as mock_render,
            patch("tarragon.services.thumbnail_service.save_to_cache") as mock_save,
        ):
            task.run()

        mock_render.assert_called_once_with(file_info.path)
        mock_save.assert_not_called()
        on_done.assert_called_once_with(file_info, None, None)
        on_error.assert_not_called()

    def test_render_psd_task_handles_exception(self, tmp_path: Path) -> None:
        """Exception during PSD render → calls on_error with message."""
        file_info = FileInfo(
            path=tmp_path / "crash.psd",
            mtime=1000.0,
            size=500,
            extension=".psd",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderPSDTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        with patch("tarragon.services.thumbnail_service.render_psd_image") as mock_render:
            mock_render.side_effect = RuntimeError("PSD crash")

            task.run()

        on_error.assert_called_once_with(file_info, "PSD crash")
        on_done.assert_not_called()


# =========================================================================
# Callback handlers
# =========================================================================


class TestCallbacks:
    """Internal callback methods (_on_done, _on_error)."""

    def test_on_done_with_image_updates_db(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """_on_done with a valid image and cache_path → upserts DB and emits thumbnailReady."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 128
        mock_img.height = 64
        cache_path = tmp_path / "cache" / "hash.png"

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service._on_done(file_info, mock_img, cache_path)

        db_mock.upsert_thumbnail.assert_called_once_with(
            path=str(file_info.path),
            mtime=1000,
            size=500,
            width=128,
            height=64,
            master_cache_path=str(cache_path),
        )
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is mock_img

    def test_on_done_with_none_skips_db(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """_on_done with None image → no DB upsert, emits thumbnailReady with None."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service._on_done(file_info, None, None)

        db_mock.upsert_thumbnail.assert_not_called()
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is None

    def test_on_done_with_none_cache_path_skips_db(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """_on_done with valid image but None cache_path → no DB upsert, emits."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        mock_img = MagicMock(spec=Image.Image)

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service._on_done(file_info, mock_img, None)

        db_mock.upsert_thumbnail.assert_not_called()
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is mock_img

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
            "master_cache_path": str(cache_path),
        }

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service.check_and_render(file_info)

        # Should NOT emit via cache hit — should fall through to re-render
        assert len(emitted) == 0, "No immediate emit — cache was corrupt, renders async"
        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask)

    def test_check_and_render_cache_format_change_affects_render(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """After set_cache_format, a cache miss dispatches wiv da new format."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        service.set_cache_format("jpeg")
        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask)
        assert task._cache_format == "jpeg", "Should use updated cache_format"

    def test_check_and_render_empty_extension_goes_plain(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """FileInfo wiv empty extension string → treated as plain image."""
        file_info = FileInfo(
            path=tmp_path / "source",
            mtime=1000.0,
            size=500,
            extension="",  # No extension
        )
        db_mock.get_thumbnail.return_value = None

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask), "Empty extension → plain render"

    def test_check_and_render_unknown_extension_goes_plain(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """FileInfo wiv unsupported extension → still dispatched as plain."""
        file_info = FileInfo(
            path=tmp_path / "source.bmp",
            mtime=1000.0,
            size=500,
            extension=".bmp",
        )
        db_mock.get_thumbnail.return_value = None

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask), "Unknown extension → plain render"

    def test_check_and_render_upper_case_psd(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Uppercase .PSD extension → still dispatched as PSD (case-insensitive)."""
        file_info = FileInfo(
            path=tmp_path / "document.PSD",
            mtime=1000.0,
            size=500,
            extension=".PSD",  # Uppercase
        )
        db_mock.get_thumbnail.return_value = None

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderPSDTask), "Upper-case PSD → PSD render"

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
            "master_cache_path": str(cache_path),
        }

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service.check_and_render(file_info)

        # Should be a cache hit since mtime/size match
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        service._threadpool.start.assert_not_called()

    def test_check_and_render_db_returns_none_for_empty_path(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """FileInfo wiv empty Path — db.get_thumbnail called wiv '.' -> returns None."""
        file_info = FileInfo(
            path=Path(""),
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        # Should not crash
        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        task = service._threadpool.start.call_args[0][0]
        assert isinstance(task, _RenderTask)

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

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        assert isinstance(service._threadpool.start.call_args[0][0], _RenderTask)


# =========================================================================
# Edge cases — callback robustness wen fings go wrong
# =========================================================================


class TestCallbackEdgeCases:
    """Callback robustness: DB failures, bad format, unicode paths."""

    def test_on_done_db_upsert_failure_still_emits_thumbnail_ready(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """DB upsert raises but thumbnailReady still emitted (graceful degradation)."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 128
        mock_img.height = 64
        cache_path = tmp_path / "cache" / "hash.png"

        db_mock.upsert_thumbnail.side_effect = RuntimeError("DB corruption!")

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        # Should NOT raise — upsert error is caught silently
        service._on_done(file_info, mock_img, cache_path)

        db_mock.upsert_thumbnail.assert_called_once()
        assert len(emitted) == 1, "thumbnailReady MUST be emitted even when DB upsert fails"
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is mock_img

    def test_on_done_none_image_skips_db_and_emits(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """_on_done wiv None image — no DB upsert, emits None."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service._on_done(file_info, None, None)

        db_mock.upsert_thumbnail.assert_not_called()
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is None

    def test_on_done_none_cache_path_skips_db_and_emits(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """_on_done wiv image but None cache_path — no DB upsert, emits."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        mock_img = MagicMock(spec=Image.Image)

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service._on_done(file_info, mock_img, None)

        db_mock.upsert_thumbnail.assert_not_called()
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is mock_img

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
# Edge cases — _RenderTask wiv format errors an' boundary conditions
# =========================================================================


class TestRenderTaskEdgeCases:
    """_RenderTask wiv format errors, None returns, an' corrupt data."""

    def test_render_task_cache_file_path_raises_value_error(
        self,
        tmp_path: Path,
    ) -> None:
        """_cache_file_path raises ValueError (bad format) → on_error called."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderTask(
            file_info=file_info,
            cache_format="invalid_format",
            on_done=on_done,
            on_error=on_error,
        )

        mock_img = MagicMock(spec=Image.Image)

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=mock_img),
            patch("tarragon.services.thumbnail_service._cache_file_path") as mock_cfp,
        ):
            mock_cfp.side_effect = ValueError("Unknown cache_format: 'invalid_format'")

            task.run()

        on_error.assert_called_once()
        args, _ = on_error.call_args
        assert args[0] is file_info
        assert "Unknown cache_format" in args[1]
        on_done.assert_not_called()

    def test_render_task_save_to_cache_raises_still_calls_on_error(
        self,
        tmp_path: Path,
    ) -> None:
        """save_to_cache raises (e.g. disk full) → on_error called."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        mock_img = MagicMock(spec=Image.Image)

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=mock_img),
            patch("tarragon.services.thumbnail_service._cache_file_path") as mock_cfp,
            patch("tarragon.services.thumbnail_service.save_to_cache") as mock_save,
        ):
            mock_cfp.return_value = (tmp_path / "cache" / "hash.png", "image/png")
            mock_save.side_effect = OSError("Disk full")

            task.run()

        on_error.assert_called_once()
        args, _ = on_error.call_args
        assert args[0] is file_info
        assert "Disk full" in args[1]
        on_done.assert_not_called()

    def test_render_task_with_empty_path(
        self,
        tmp_path: Path,
    ) -> None:
        """Empty file path → render_plain_image returns None → on_done wiv None."""
        file_info = FileInfo(
            path=Path(""),
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        with patch("tarragon.services.thumbnail_service.render_plain_image", return_value=None) as mock_render:
            task.run()

        mock_render.assert_called_once_with(Path(""))
        on_done.assert_called_once_with(file_info, None, None)
        on_error.assert_not_called()


# =========================================================================
# Edge cases — _RenderPSDTask wiv format errors
# =========================================================================


class TestRenderPSDTaskEdgeCases:
    """_RenderPSDTask wiv format errors and boundary conditions."""

    def test_render_psd_task_cache_file_path_raises_value_error(
        self,
        tmp_path: Path,
    ) -> None:
        """_cache_file_path raises ValueError for PSD task → on_error called."""
        file_info = FileInfo(
            path=tmp_path / "doc.psd",
            mtime=1000.0,
            size=500,
            extension=".psd",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderPSDTask(
            file_info=file_info,
            cache_format="bogus",
            on_done=on_done,
            on_error=on_error,
        )

        mock_img = MagicMock(spec=Image.Image)

        with (
            patch("tarragon.services.thumbnail_service.render_psd_image", return_value=mock_img),
            patch("tarragon.services.thumbnail_service._cache_file_path") as mock_cfp,
        ):
            mock_cfp.side_effect = ValueError("Unknown cache_format: 'bogus'")

            task.run()

        on_error.assert_called_once()
        args, _ = on_error.call_args
        assert args[0] is file_info
        assert "Unknown cache_format" in args[1]
        on_done.assert_not_called()

    def test_render_psd_task_render_returns_none_still_calls_on_done(
        self,
        tmp_path: Path,
    ) -> None:
        """render_psd_image returns None → on_done called wiv None, no save."""
        file_info = FileInfo(
            path=tmp_path / "doc.psd",
            mtime=1000.0,
            size=500,
            extension=".psd",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderPSDTask(
            file_info=file_info,
            cache_format="png",
            on_done=on_done,
            on_error=on_error,
        )

        with (
            patch("tarragon.services.thumbnail_service.render_psd_image", return_value=None),
            patch("tarragon.services.thumbnail_service.save_to_cache") as mock_save,
        ):
            task.run()

        on_done.assert_called_once_with(file_info, None, None)
        on_error.assert_not_called()
        mock_save.assert_not_called()


# =========================================================================
# Edge cases — ThumbnailService at the boundary
# =========================================================================


class TestThumbnailServiceEdgeCases:
    """Service-level edge cases: pool full, double start, config edge cases."""

    def test_threadpool_start_returns_false_triggers_error(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """QThreadPool.start() returns False (pool full) — error signal emitted."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        db_mock.get_thumbnail.return_value = None

        # Simulate pool full — start returns False
        service._threadpool.start.return_value = False

        error_emitted: list[tuple[str, str]] = []
        ready_emitted: list[tuple[str, object]] = []
        service.errorOccurred.connect(lambda p, e: error_emitted.append((p, e)))
        service.thumbnailReady.connect(lambda p, i: ready_emitted.append((p, i)))

        service.check_and_render(file_info)

        service._threadpool.start.assert_called_once()
        assert len(error_emitted) == 1
        assert error_emitted[0][0] == str(file_info.path)
        assert "queue full" in error_emitted[0][1].lower()
        # thumbnailReady should still fire with None so UI is not stuck
        assert len(ready_emitted) == 1
        assert ready_emitted[0][1] is None

    def test_two_rapid_cache_misses_dispatch_separate_tasks(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Two rapid check_and_render calls for different files → two separate tasks."""
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

        service.check_and_render(file_a)
        service.check_and_render(file_b)

        assert service._threadpool.start.call_count == 2
        task_a = service._threadpool.start.call_args_list[0][0][0]
        task_b = service._threadpool.start.call_args_list[1][0][0]
        assert isinstance(task_a, _RenderTask)
        assert isinstance(task_b, _RenderTask)
        assert task_a is not task_b, "Each call should create a fresh task"

    def test_check_and_render_same_file_twice_cache_hit_then_miss(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db_mock: MagicMock,
    ) -> None:
        """Same file checked twice: first cache hit, second cache miss (mtime changes)."""
        cache_dir = tmp_path / "cache"
        cache_dir.mkdir(parents=True)
        cache_path = cache_dir / "thumb.png"
        ref_img = Image.new("RGB", (64, 64), color="red")
        ref_img.save(cache_path)

        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )

        # First call — cache hit
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,
            "size": 500,
            "width": 64,
            "height": 64,
            "master_cache_path": str(cache_path),
        }

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        service.check_and_render(file_info)
        assert len(emitted) == 1, "First call should be a cache hit"
        service._threadpool.start.assert_not_called()
        emitted.clear()

        # Second call — file's mtime changed (stale)
        db_mock.get_thumbnail.return_value = {
            "path": str(file_info.path),
            "mtime": 1000,  # DB still has old mtime
            "size": 500,
            "width": 64,
            "height": 64,
            "master_cache_path": str(cache_path),
        }

        # But file_info now has different mtime
        file_info_updated = FileInfo(
            path=tmp_path / "source.png",
            mtime=2000.0,  # Changed!
            size=500,
            extension=".png",
        )

        service.check_and_render(file_info_updated)
        service._threadpool.start.assert_called_once()
        assert len(emitted) == 0, "No immediate emit — stale cache triggers render"

    def test_signals_are_distinct_instances(
        self,
        service: ThumbnailService,
    ) -> None:
        """Each signal is a distinct Qt signal object."""
        assert service.thumbnailReady is not service.thumbnailsUpdated
        assert service.thumbnailReady is not service.errorOccurred
        assert service.thumbnailsUpdated is not service.errorOccurred

    def test_cache_format_round_trip_changes(
        self,
        service: ThumbnailService,
    ) -> None:
        """set_cache_format to jpeg and back to png works correctly."""
        assert service._cache_format == "png"
        service.set_cache_format("jpeg")
        assert service._cache_format == "jpeg"
        service.set_cache_format("png")
        assert service._cache_format == "png"
        service.set_cache_format("jpeg")
        assert service._cache_format == "jpeg"


# =========================================================================
# Integration-style — cache_file_path error propagation through full flow
# =========================================================================


class TestFullFlowEdgeCases:
    """End-to-end edge cases wiv real cache_file_path behavior."""

    def test_render_task_with_unknown_format_propagates_error(
        self,
        tmp_path: Path,
    ) -> None:
        """Full task run wiv unknown format → on_error gets ValueError message."""
        file_info = FileInfo(
            path=tmp_path / "source.png",
            mtime=1000.0,
            size=500,
            extension=".png",
        )
        on_done = MagicMock()
        on_error = MagicMock()

        task = _RenderTask(
            file_info=file_info,
            cache_format="webp",  # Not supported by _cache_file_path
            on_done=on_done,
            on_error=on_error,
        )

        mock_img = MagicMock(spec=Image.Image)

        with patch("tarragon.services.thumbnail_service.render_plain_image", return_value=mock_img):
            # Don't patch _cache_file_path — use the REAL one which raises ValueError
            task.run()

        on_error.assert_called_once()
        args, _ = on_error.call_args
        assert args[0] is file_info
        assert "Unknown cache_format" in args[1]
        on_done.assert_not_called()
