"""Tests for rendering PSD"""

from __future__ import annotations

import threading
from concurrent.futures import CancelledError
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image


class TestRenderPSD:
    def test_compute_worker_count_default(self) -> None:
        """_compute_worker_count with no override returns a sensible value in [1, 8]."""
        from tarragon.renderers.psd import _compute_worker_count

        result = _compute_worker_count()
        assert isinstance(result, int)
        assert 1 <= result <= 8

    @pytest.mark.parametrize(
        ("override", "expected"),
        [
            (3, 3),
            (0, 1),  # below minimum — clamped to 1
            (10, 8),  # above maximum — clamped to 8
            (1, 1),  # at minimum
            (8, 8),  # at maximum
            (-5, 1),  # negative — clamped to 1
        ],
    )
    def test_compute_worker_count_manual_override(self, override: int, expected: int) -> None:
        """_compute_worker_count clamps manual override to [1, 8]."""
        from tarragon.renderers.psd import _compute_worker_count

        assert _compute_worker_count(override) == expected

    def test_compute_worker_count_minimum_one(self) -> None:
        """_compute_worker_count returns at least 1 even with zero available RAM."""
        from unittest.mock import patch

        from tarragon.renderers.psd import _compute_worker_count

        with patch("tarragon.renderers.psd.psutil.virtual_memory") as mock_vm:
            mock_vm.return_value.available = 0
            assert _compute_worker_count() == 1

    def test_shared_executor_is_singleton(self) -> None:
        """Multiple calls to get_executor return the same instance."""
        from tarragon.renderers.psd import get_executor

        exec1 = get_executor()
        exec2 = get_executor()
        assert exec1 is exec2

    def test_render_psd_image_nonexistent_file(self, tmp_path: Path) -> None:
        """render_psd_image returns None when the file does not exist."""
        from unittest.mock import MagicMock, patch

        from tarragon.renderers.psd import render_psd_image

        with patch("tarragon.renderers.psd.get_executor") as mock_get_exec:
            mock_exec = MagicMock()
            mock_get_exec.return_value = mock_exec
            mock_future = MagicMock()
            mock_future.result.return_value = None  # worker returns None
            mock_exec.submit.return_value = mock_future

            result = render_psd_image(tmp_path / "nonexistent.psd", 20.0, 2, 2)
            assert result is None
            mock_exec.submit.assert_called_once()

    def test_render_psd_image_corrupt_file(self, tmp_path: Path) -> None:
        """render_psd_image returns None when the worker encounters an error."""
        from unittest.mock import MagicMock, patch

        from tarragon.renderers.psd import render_psd_image

        with patch("tarragon.renderers.psd.get_executor") as mock_get_exec:
            mock_exec = MagicMock()
            mock_get_exec.return_value = mock_exec
            mock_future = MagicMock()
            mock_future.result.side_effect = Exception("Worker failure")
            mock_exec.submit.return_value = mock_future

            result = render_psd_image(tmp_path / "corrupt.psd", 20.0, 2, 2)
            assert result is None
            mock_exec.submit.assert_called_once()


class TestRenderPSDEdgeCases:
    def test_compute_worker_count_explicit_none_override(self) -> None:
        """_compute_worker_count(None) falls through to RAM-based calculation same as no arg."""
        from tarragon.renderers.psd import _compute_worker_count

        default = _compute_worker_count()
        explicit_none = _compute_worker_count(None)

        assert isinstance(explicit_none, int)
        assert 1 <= explicit_none <= 8
        # Both paths go through the same RAM logic
        assert explicit_none == default

    def test_compute_worker_count_multiple_calls_reevaluates_ram(self) -> None:
        """_compute_worker_count re-evaluates available RAM on each call (not cached)."""
        from unittest.mock import patch

        from tarragon.renderers.psd import _compute_worker_count

        with patch("tarragon.renderers.psd.psutil.virtual_memory") as mock_vm:
            # First call: 400 MB available → 400 // 200 = 2
            mock_vm.return_value.available = 400_000_000
            first = _compute_worker_count()
            assert first == 2

            # Second call: 1.6 GB available → 1600 // 200 = 8 → min(8,8) = 8
            mock_vm.return_value.available = 1_600_000_000
            second = _compute_worker_count()
            assert second == 8

            # Third call: 50 MB available → 50 // 200 = 0 → max(1, 0) = 1
            mock_vm.return_value.available = 50_000_000
            third = _compute_worker_count()
            assert third == 1

    def test_compute_worker_count_max_ram_returns_at_most_8(self) -> None:
        """_compute_worker_count never exceeds 8 even with absurdly high RAM."""
        from unittest.mock import patch

        from tarragon.renderers.psd import _compute_worker_count

        with patch("tarragon.renderers.psd.psutil.virtual_memory") as mock_vm:
            mock_vm.return_value.available = 100_000_000_000  # 100 GB
            assert _compute_worker_count() == 8

    def test_composite_psd_in_process_nonexistent_file_returns_none(self) -> None:
        """_composite_psd_in_process returns None for a file path that does not exist."""
        from tarragon.renderers.psd import _composite_psd_in_process

        result = _composite_psd_in_process("/tmp/this_path_definitely_does_not_exist.psd", 20.0, 2, 2)
        assert result is None

    def test_composite_psd_in_process_empty_file_returns_none(self, tmp_path: Path) -> None:
        """_composite_psd_in_process returns None when the file exists but is empty."""
        from tarragon.renderers.psd import _composite_psd_in_process

        empty_path = tmp_path / "empty.psd"
        empty_path.write_text("")

        result = _composite_psd_in_process(str(empty_path), 20.0, 2, 2)
        assert result is None

    def test_composite_psd_in_process_truncated_file_returns_none(self, tmp_path: Path) -> None:
        """_composite_psd_in_process returns None when the PSD file is truncated/invalid."""
        from tarragon.renderers.psd import _composite_psd_in_process

        bad_path = tmp_path / "truncated.psd"
        # Write just the PSD header magic bytes but no valid layer data
        bad_path.write_bytes(b"8BPS\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00")

        result = _composite_psd_in_process(str(bad_path), 20.0, 2, 2)
        assert result is None

    def test_composite_psd_in_process_missing_psdtools_returns_none(self, tmp_path: Path) -> None:
        """_composite_psd_in_process returns None when psd_tools is missing.

        The import is now inside the try/except block, so ImportError is caught
        and the function returns None gracefully.
        """
        import builtins

        from tarragon.renderers.psd import _composite_psd_in_process

        original_import = builtins.__import__

        def mock_import(name: str, *args: Any, **kwargs: Any) -> Any:
            if name == "psd_tools":
                raise ImportError("No module named psd_tools")
            return original_import(name, *args, **kwargs)

        with patch("builtins.__import__", side_effect=mock_import):
            result = _composite_psd_in_process(str(tmp_path / "fake.psd"), 20.0, 2, 2)
            assert result is None

    def test_composite_psd_in_process_large_canvas_uses_tiled_path(self) -> None:
        """_composite_psd_in_process uses tiled compositing for canvases >20 MP.

        A 5000x5000 canvas (25 MP) triggers the tiled path, which calls
        composite() with viewport arguments for each of the 4 tiles.
        """
        from tarragon.renderers.psd import _composite_psd_in_process

        # Mock PSDImage class with open() returning a mock instance
        mock_psd_cls = MagicMock()
        mock_psd_instance = MagicMock()
        mock_psd_instance.width = 5000
        mock_psd_instance.height = 5000
        # Each tile composite returns a small RGBA image
        tile_img = Image.new("RGBA", (2500, 2500), (255, 0, 0, 255))
        mock_psd_instance.composite.return_value = tile_img
        mock_psd_cls.open.return_value = mock_psd_instance

        with patch("psd_tools.PSDImage", mock_psd_cls):
            result = _composite_psd_in_process("/fake/large_canvas.psd", 20.0, 2, 2)

        # Should succeed with tiled path
        assert result is not None
        assert isinstance(result, bytes)
        assert len(result) > 0

        # Composite should have been called 4 times (2x2 grid) with viewport
        assert mock_psd_instance.composite.call_count == 4
        for call in mock_psd_instance.composite.call_args_list:
            _, kwargs = call
            assert "viewport" in kwargs, "Tiled path should pass viewport to composite()"
            assert kwargs.get("force") is True

    def testget_executor_creates_executor_on_first_call(self) -> None:
        """get_executor lazily creates the executor — it is None before first call."""
        import tarragon.renderers.psd as _tmod

        # Start clean
        saved = _tmod._shared_executor
        _tmod._shared_executor = None
        try:
            assert _tmod._shared_executor is None
            executor = _tmod.get_executor()
            assert executor is not None
            assert _tmod._shared_executor is executor
        finally:
            # Cleanup only if we created one
            if _tmod._shared_executor is not None and _tmod._shared_executor is not saved:
                _tmod.shutdown_executor()
            _tmod._shared_executor = saved

    def testget_executor_after_shutdown_creates_new_instance(self) -> None:
        """get_executor after _shutdown_executor creates a brand new executor."""
        import tarragon.renderers.psd as _tmod

        saved = _tmod._shared_executor
        _tmod._shared_executor = None
        try:
            first = _tmod.get_executor()
            assert first is not None

            # Shut it down
            _tmod.shutdown_executor()
            assert _tmod._shared_executor is None

            # Get again — should be a new instance
            second = _tmod.get_executor()
            assert second is not None
            assert second is not first
        finally:
            if _tmod._shared_executor is not None and _tmod._shared_executor is not saved:
                _tmod.shutdown_executor()
            _tmod._shared_executor = saved

    def test_shutdown_executor_when_already_none_is_safe(self) -> None:
        """_shutdown_executor does not error when called with _shared_executor already None."""
        import tarragon.renderers.psd as _tmod

        saved = _tmod._shared_executor
        _tmod._shared_executor = None
        try:
            # Should not raise
            _tmod.shutdown_executor()
            _tmod.shutdown_executor()
            _tmod.shutdown_executor()
            assert _tmod._shared_executor is None
        finally:
            _tmod._shared_executor = saved

    def test_render_psd_image_timeout_returns_none(self) -> None:
        """render_psd_image returns None when the future does not complete within the timeout."""
        from tarragon.renderers.psd import render_psd_image

        with patch("tarragon.renderers.psd.get_executor") as mock_get_exec:
            mock_exec = MagicMock()
            mock_get_exec.return_value = mock_exec
            mock_future = MagicMock()
            # Simulate a future that never completes: done() returns False,
            # result() keeps raising TimeoutError, then eventually a generic Exception.
            mock_future.done.return_value = False
            mock_future.result.side_effect = [
                TimeoutError("poll 1"),
                TimeoutError("poll 2"),
                Exception("giving up"),
            ]
            mock_exec.submit.return_value = mock_future

            result = render_psd_image(Path("/fake/timeout_test.psd"), 20.0, 2, 2)
            assert result is None
            mock_exec.submit.assert_called_once()

    def test_render_psd_image_cancelled_future_returns_none(self) -> None:
        """render_psd_image returns None when the future is cancelled."""
        from tarragon.renderers.psd import render_psd_image

        with patch("tarragon.renderers.psd.get_executor") as mock_get_exec:
            mock_exec = MagicMock()
            mock_get_exec.return_value = mock_exec
            mock_future = MagicMock()
            mock_future.result.side_effect = CancelledError()
            mock_exec.submit.return_value = mock_future

            result = render_psd_image(Path("/fake/cancelled_test.psd"), 20.0, 2, 2)
            assert result is None
            mock_exec.submit.assert_called_once()

    def test_render_psd_image_after_executor_shutdown_succeeds(self, tmp_path: Path) -> None:
        """render_psd_image still works if the shared executor was previously shut down.

        get_executor should create a new executor transparently.
        """
        from tarragon.renderers.psd import render_psd_image

        with patch("tarragon.renderers.psd.get_executor") as mock_get_exec:
            # Simulate: first call returns an executor, shutdown sets it to None,
            # second call returns a new one
            first_exec = MagicMock()
            second_exec = MagicMock()

            mock_get_exec.side_effect = [first_exec, second_exec]

            first_future = MagicMock()
            first_future.result.side_effect = Exception("First executor dead")
            first_exec.submit.return_value = first_future

            second_future = MagicMock()
            second_future.result.return_value = None  # worker returns None
            second_exec.submit.return_value = second_future

            # First call — executor is dead (simulating shutdown between calls)
            result1 = render_psd_image(tmp_path / "test_first.psd", 20.0, 2, 2)
            assert result1 is None

            # Second call — new executor (should work)
            result2 = render_psd_image(tmp_path / "test_second.psd", 20.0, 2, 2)
            assert result2 is None

            # Verify both submits were called on their respective executors
            first_exec.submit.assert_called_once()
            second_exec.submit.assert_called_once()

    def test_render_psd_image_concurrent_calls_are_safe(self, tmp_path: Path) -> None:
        """Multiple concurrent calls to render_psd_image do not crash.

        Each call gets its own future from the shared executor.
        """
        from tarragon.renderers.psd import render_psd_image

        call_count = 0
        call_lock = threading.Lock()
        barrier = threading.Barrier(5, timeout=5)
        results: list[Exception | object | None] = [None] * 5

        with patch("tarragon.renderers.psd.get_executor") as mock_get_exec:
            mock_exec = MagicMock()
            mock_get_exec.return_value = mock_exec

            def make_future(*args: object) -> MagicMock:
                mock_future = MagicMock()
                mock_future.result.return_value = None
                return mock_future

            mock_exec.submit.side_effect = make_future

            def worker(idx: int) -> None:
                nonlocal call_count
                try:
                    barrier.wait()
                    result = render_psd_image(tmp_path / f"concurrent_{idx}.psd", 20.0, 2, 2)
                    results[idx] = result
                    with call_lock:
                        call_count += 1
                except Exception as exc:
                    results[idx] = exc
                    with call_lock:
                        call_count += 1

            threads = [threading.Thread(target=worker, args=(i,), daemon=True) for i in range(5)]

            for t in threads:
                t.start()
            for t in threads:
                t.join(timeout=10)

        # All 5 should have completed
        assert call_count == 5, f"Only {call_count}/5 threads completed"
        # All results should be None (worker returned None)
        for i, r in enumerate(results):
            assert r is None, f"Thread {i} got unexpected result: {r!r}"
        # submit should have been called 5 times
        assert mock_exec.submit.call_count == 5

    def test_render_psd_image_worker_returns_valid_bytes(self, tmp_path: Path) -> None:
        """render_psd_image returns a PIL Image when the worker returns valid PNG bytes."""
        import io

        from PIL import Image as PILImage
        from tarragon.renderers.psd import render_psd_image

        # Create a real PNG image as bytes
        dummy_img = PILImage.new("RGBA", (50, 50), (255, 0, 0, 128))
        buf = io.BytesIO()
        dummy_img.save(buf, "PNG")
        png_bytes = buf.getvalue()

        with patch("tarragon.renderers.psd.get_executor") as mock_get_exec:
            mock_exec = MagicMock()
            mock_get_exec.return_value = mock_exec
            mock_future = MagicMock()
            # Simulate a future that completes on first poll
            mock_future.done.return_value = False
            mock_future.result.return_value = png_bytes
            mock_exec.submit.return_value = mock_future

            result = render_psd_image(tmp_path / "success.psd", 20.0, 2, 2)

            assert result is not None
            assert isinstance(result, PILImage.Image)
            assert result.size == (50, 50)
            assert result.mode == "RGBA"
            mock_exec.submit.assert_called_once()

    def test_render_psd_image_with_tiny_psd_file(self, tmp_path: Path) -> None:
        """render_psd_image composits a real tiny PSD file successfully.

        This is an integration test that uses the actual ProcessPoolExecutor
        and psd-tools library.
        """
        from psd_tools import PSDImage
        from tarragon.renderers.psd import render_psd_image

        psd_path = tmp_path / "tiny_test.psd"
        psd = PSDImage.new(mode="RGBA", size=(10, 10))
        psd.save(str(psd_path))

        result = render_psd_image(psd_path, 20.0, 2, 2)

        assert result is not None, "render_psd_image returned None for a valid tiny PSD"
        assert result.size == (10, 10), f"Expected (10,10) got {result.size}"
        assert result.mode == "RGBA", f"Expected RGBA got {result.mode}"

        # Clean up the executor that was created
        from tarragon.renderers.psd import shutdown_executor

        shutdown_executor()

    def test_render_psd_image_resizes_large_composite(self, tmp_path: Path) -> None:
        """render_psd_image resizes composited output when target_size is specified."""
        from psd_tools import PSDImage
        from tarragon.renderers.cache import MASTER_LONG_EDGE
        from tarragon.renderers.psd import render_psd_image

        psd_path = tmp_path / "large_test.psd"
        # Create a PSD larger than MASTER_LONG_EDGE (2048)
        psd = PSDImage.new(mode="RGBA", size=(4000, 3000))
        psd.save(str(psd_path))

        result = render_psd_image(psd_path, 20.0, 2, 2, target_size=MASTER_LONG_EDGE)

        assert result is not None, "render_psd_image returned None for a large PSD"
        assert max(result.size) <= MASTER_LONG_EDGE, f"Result too large: {result.size} > {MASTER_LONG_EDGE}"
        # Aspect ratio should be preserved
        orig_ratio = 4000 / 3000
        result_ratio = result.size[0] / result.size[1]
        assert abs(result_ratio - orig_ratio) < 0.01, f"Aspect ratio changed: {result_ratio} != {orig_ratio}"

        # Clean up the executor
        from tarragon.renderers.psd import shutdown_executor

        shutdown_executor()

    def test_render_psd_image_invalid_path_in_subprocess(self, tmp_path: Path) -> None:
        """render_psd_image returns None when the sub-process worker gets an invalid path."""
        from tarragon.renderers.psd import render_psd_image

        # Non-existent file — the worker (in subprocess) will try to open it and fail
        result = render_psd_image(tmp_path / "i_do_not_exist_at_all.psd", 20.0, 2, 2)
        assert result is None

        from tarragon.renderers.psd import shutdown_executor

        shutdown_executor()

    def test_atexit_handler_not_crashing_when_executor_was_never_created(self) -> None:
        """_shutdown_executor (registered via atexit) is safe when executor never started."""
        import tarragon.renderers.psd as _tmod

        saved = _tmod._shared_executor
        _tmod._shared_executor = None
        try:
            # Simulate atexit calling shutdown when executor was never created
            _tmod.shutdown_executor()
            assert _tmod._shared_executor is None
        finally:
            _tmod._shared_executor = saved
