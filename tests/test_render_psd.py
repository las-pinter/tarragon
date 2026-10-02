"""Tests for PSD rendering"""

from __future__ import annotations

import builtins
import io
import logging
import threading
from concurrent.futures import CancelledError
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image
from psd_tools import PSDImage

import tarragon.renderers.psd as _tmod
from tarragon.renderers.psd import (
    PSD_FULL_TIER_MAX_EDGE,
    PSD_RENDER_TIMEOUT_S,
    _composite_psd_in_process,
    _compute_worker_count,
    get_executor,
    render_psd_image,
    shutdown_executor,
)
from tarragon.theme.constants import (
    PSD_WORKER_DEFAULT,
    PSD_WORKER_MAX,
    PSD_WORKER_MIN,
    PSD_WORKER_RAM_BYTES,
)

# Local long-edge target; the shared cache constant was deleted.
_LONG_EDGE = 2048


class TestRenderPSD:
    """PSD rendering pipeline behavior."""

    def test_compute_worker_count_default(self) -> None:
        """_compute_worker_count with no override returns a sensible value in the clamped range."""
        result = _compute_worker_count()
        assert isinstance(result, int)
        assert PSD_WORKER_MIN <= result <= PSD_WORKER_MAX

    @pytest.mark.parametrize(
        ("override", "expected"),
        [
            (PSD_WORKER_DEFAULT, PSD_WORKER_DEFAULT),
            (0, PSD_WORKER_MIN),  # below minimum - clamped to PSD_WORKER_MIN
            (10, PSD_WORKER_MAX),  # above maximum - clamped to PSD_WORKER_MAX
            (PSD_WORKER_MIN, PSD_WORKER_MIN),  # at minimum
            (PSD_WORKER_MAX, PSD_WORKER_MAX),  # at maximum
            (-5, PSD_WORKER_MIN),  # negative - clamped to PSD_WORKER_MIN
        ],
    )
    def test_compute_worker_count_manual_override(self, override: int, expected: int) -> None:
        """_compute_worker_count clamps manual override to [PSD_WORKER_MIN, PSD_WORKER_MAX]."""
        assert _compute_worker_count(override) == expected

    def test_compute_worker_count_minimum_one(self) -> None:
        """_compute_worker_count returns at least PSD_WORKER_MIN even with zero available RAM."""
        with patch("tarragon.renderers.psd.psutil.virtual_memory") as mock_vm:
            mock_vm.return_value.available = 0
            assert _compute_worker_count() == PSD_WORKER_MIN

    def test_shared_executor_is_singleton(self) -> None:
        """Multiple calls to get_executor return the same instance."""
        exec1 = get_executor()
        exec2 = get_executor()
        assert exec1 is exec2

    def test_render_psd_image_nonexistent_file(self, tmp_path: Path) -> None:
        """render_psd_image returns None when the file does not exist."""
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
    """Edge cases for the PSD rendering pipeline."""

    def test_compute_worker_count_multiple_calls_reevaluates_ram(self) -> None:
        """_compute_worker_count re-evaluates available RAM on each call (not cached)."""
        with patch("tarragon.renderers.psd.psutil.virtual_memory") as mock_vm:
            # First call: 2 workers' RAM available -> 2 workers
            mock_vm.return_value.available = 2 * PSD_WORKER_RAM_BYTES
            first = _compute_worker_count()
            assert first == 2

            # Second call: 8 workers' RAM available -> capped at PSD_WORKER_MAX
            mock_vm.return_value.available = PSD_WORKER_MAX * PSD_WORKER_RAM_BYTES
            second = _compute_worker_count()
            assert second == PSD_WORKER_MAX

            # Third call: 50 MB available -> below one worker's RAM -> PSD_WORKER_MIN
            mock_vm.return_value.available = 50_000_000
            third = _compute_worker_count()
            assert third == PSD_WORKER_MIN

    def test_compute_worker_count_max_ram_caps_at_maximum(self) -> None:
        """_compute_worker_count never exceeds PSD_WORKER_MAX even with absurdly high RAM."""
        with patch("tarragon.renderers.psd.psutil.virtual_memory") as mock_vm:
            mock_vm.return_value.available = 100_000_000_000  # 100 GB
            assert _compute_worker_count() == PSD_WORKER_MAX

    def test_composite_psd_in_process_nonexistent_file_returns_none(self) -> None:
        """_composite_psd_in_process returns None for a file path that does not exist."""
        result = _composite_psd_in_process("/tmp/this_path_definitely_does_not_exist.psd", 20.0, 2, 2)
        assert result is None

    def test_composite_psd_in_process_empty_file_returns_none(self, tmp_path: Path) -> None:
        """_composite_psd_in_process returns None when the file exists but is empty."""
        empty_path = tmp_path / "empty.psd"
        empty_path.write_text("")

        result = _composite_psd_in_process(str(empty_path), 20.0, 2, 2)
        assert result is None

    def test_composite_psd_in_process_truncated_file_returns_none(self, tmp_path: Path) -> None:
        """_composite_psd_in_process returns None when the PSD file is truncated/invalid."""
        bad_path = tmp_path / "truncated.psd"
        # Write just the PSD header magic bytes but no valid layer data
        bad_path.write_bytes(b"8BPS\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00")

        result = _composite_psd_in_process(str(bad_path), 20.0, 2, 2)
        assert result is None

    def test_composite_psd_in_process_missing_psdtools_returns_none(self, tmp_path: Path) -> None:
        """_composite_psd_in_process returns None when psd_tools is missing."""
        original_import = builtins.__import__

        def mock_import(name: str, *args: Any, **kwargs: Any) -> Any:
            if name == "psd_tools":
                raise ImportError("No module named psd_tools")
            return original_import(name, *args, **kwargs)

        with patch("builtins.__import__", side_effect=mock_import):
            result = _composite_psd_in_process(str(tmp_path / "fake.psd"), 20.0, 2, 2)
            assert result is None

    def test_composite_psd_in_process_large_canvas_uses_tiled_path(self) -> None:
        """_composite_psd_in_process uses tiled compositing for canvases over 20 MP."""
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

    def test_get_executor_creates_executor_on_first_call(self) -> None:
        """get_executor lazily creates the executor - it is None before first call."""
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

    def test_get_executor_after_shutdown_creates_new_instance(self) -> None:
        """get_executor after _shutdown_executor creates a brand new executor."""
        saved = _tmod._shared_executor
        _tmod._shared_executor = None
        try:
            first = _tmod.get_executor()
            assert first is not None

            # Shut it down
            _tmod.shutdown_executor()
            assert _tmod._shared_executor is None

            # Get again - should be a new instance
            second = _tmod.get_executor()
            assert second is not None
            assert second is not first
        finally:
            if _tmod._shared_executor is not None and _tmod._shared_executor is not saved:
                _tmod.shutdown_executor()
            _tmod._shared_executor = saved

    def test_render_psd_image_timeout_returns_none(self) -> None:
        """render_psd_image returns None when the future does not complete within the timeout."""
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
        """render_psd_image still works if the shared executor was previously shut down."""
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

            # First call - executor is dead (simulating shutdown between calls)
            result1 = render_psd_image(tmp_path / "test_first.psd", 20.0, 2, 2)
            assert result1 is None

            # Second call - new executor (should work)
            result2 = render_psd_image(tmp_path / "test_second.psd", 20.0, 2, 2)
            assert result2 is None

            # Verify both submits were called on their respective executors
            first_exec.submit.assert_called_once()
            second_exec.submit.assert_called_once()

    def test_render_psd_image_concurrent_calls_are_safe(self, tmp_path: Path) -> None:
        """Multiple concurrent calls to render_psd_image do not crash."""
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
        # Create a real PNG image as bytes
        dummy_img = Image.new("RGBA", (50, 50), (255, 0, 0, 128))
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
            assert isinstance(result, Image.Image)
            assert result.size == (50, 50)
            assert result.mode == "RGBA"
            mock_exec.submit.assert_called_once()

    def test_render_psd_image_with_tiny_psd_file(self, tmp_path: Path) -> None:
        """render_psd_image composits a real tiny PSD file successfully."""
        psd_path = tmp_path / "tiny_test.psd"
        psd = PSDImage.new(mode="RGBA", size=(10, 10))
        psd.save(str(psd_path))

        result = render_psd_image(psd_path, 20.0, 2, 2)

        assert result is not None, "render_psd_image returned None for a valid tiny PSD"
        assert result.size == (10, 10), f"Expected (10,10) got {result.size}"
        assert result.mode == "RGBA", f"Expected RGBA got {result.mode}"

        # Clean up the executor that was created
        shutdown_executor()

    def test_render_psd_image_resizes_large_composite(self, tmp_path: Path) -> None:
        """render_psd_image resizes composited output when target_size is specified."""
        psd_path = tmp_path / "large_test.psd"
        # Create a PSD larger than the long-edge target (2048)
        psd = PSDImage.new(mode="RGBA", size=(4000, 3000))
        psd.save(str(psd_path))

        result = render_psd_image(psd_path, 20.0, 2, 2, target_size=_LONG_EDGE)

        assert result is not None, "render_psd_image returned None for a large PSD"
        assert max(result.size) <= _LONG_EDGE, f"Result too large: {result.size} > {_LONG_EDGE}"
        # Aspect ratio should be preserved
        orig_ratio = 4000 / 3000
        result_ratio = result.size[0] / result.size[1]
        assert abs(result_ratio - orig_ratio) < 0.01, f"Aspect ratio changed: {result_ratio} != {orig_ratio}"

        # Clean up the executor
        shutdown_executor()

    def test_render_psd_image_invalid_path_in_subprocess(self, tmp_path: Path) -> None:
        """render_psd_image returns None when the sub-process worker gets an invalid path."""
        # Non-existent file - the worker (in subprocess) will try to open it and fail
        result = render_psd_image(tmp_path / "i_do_not_exist_at_all.psd", 20.0, 2, 2)
        assert result is None

        shutdown_executor()

    def test_atexit_handler_not_crashing_when_executor_was_never_created(self) -> None:
        """_shutdown_executor (registered via atexit) is safe when executor never started."""
        saved = _tmod._shared_executor
        _tmod._shared_executor = None
        try:
            # Simulate atexit calling shutdown when executor was never created
            _tmod.shutdown_executor()
            assert _tmod._shared_executor is None
        finally:
            _tmod._shared_executor = saved


class TestPSDFullTierCap:
    """Full-tier (target_size=None) composites are capped at PSD_FULL_TIER_MAX_EDGE."""

    def test_full_tier_max_edge_constant_is_8192(self) -> None:
        """PSD_FULL_TIER_MAX_EDGE caps the long edge of full-tier PNG output."""
        assert PSD_FULL_TIER_MAX_EDGE == 8192

    def test_full_tier_composite_over_cap_is_capped(self) -> None:
        """A full-tier canvas over the cap renders at the cap with aspect preserved."""
        mock_psd_cls = MagicMock()
        mock_psd_instance = MagicMock()
        mock_psd_instance.width = 9000
        mock_psd_instance.height = 1000
        big_img = Image.new("RGBA", (9000, 1000), (128, 128, 128, 255))
        mock_psd_instance.composite.return_value = big_img
        mock_psd_cls.open.return_value = mock_psd_instance

        with patch("psd_tools.PSDImage", mock_psd_cls):
            result = _composite_psd_in_process("/fake/over_cap.psd", 20.0, 2, 2)

        assert result is not None
        decoded = Image.open(io.BytesIO(result))
        assert max(decoded.size) == PSD_FULL_TIER_MAX_EDGE
        orig_ratio = 9000 / 1000
        result_ratio = decoded.size[0] / decoded.size[1]
        assert abs(result_ratio - orig_ratio) < 0.01

    def test_full_tier_composite_under_cap_is_unchanged(self) -> None:
        """A full-tier canvas under the cap is neither upscaled nor resized."""
        mock_psd_cls = MagicMock()
        mock_psd_instance = MagicMock()
        mock_psd_instance.width = 3000
        mock_psd_instance.height = 2000
        img = Image.new("RGBA", (3000, 2000), (64, 64, 64, 255))
        mock_psd_instance.composite.return_value = img
        mock_psd_cls.open.return_value = mock_psd_instance

        with patch("psd_tools.PSDImage", mock_psd_cls):
            result = _composite_psd_in_process("/fake/under_cap.psd", 20.0, 2, 2)

        assert result is not None
        decoded = Image.open(io.BytesIO(result))
        assert decoded.size == (3000, 2000)

    def test_explicit_target_size_path_unchanged_by_cap(self) -> None:
        """An explicit target_size wins over the cap and still resizes down."""
        mock_psd_cls = MagicMock()
        mock_psd_instance = MagicMock()
        mock_psd_instance.width = 4000
        mock_psd_instance.height = 3000
        img = Image.new("RGBA", (4000, 3000), (32, 32, 32, 255))
        mock_psd_instance.composite.return_value = img
        mock_psd_cls.open.return_value = mock_psd_instance

        with patch("psd_tools.PSDImage", mock_psd_cls):
            result = _composite_psd_in_process("/fake/explicit.psd", 20.0, 2, 2, target_size=_LONG_EDGE)

        assert result is not None
        decoded = Image.open(io.BytesIO(result))
        assert max(decoded.size) == _LONG_EDGE
        orig_ratio = 4000 / 3000
        result_ratio = decoded.size[0] / decoded.size[1]
        assert abs(result_ratio - orig_ratio) < 0.01


class TestRenderPSDDeadlineAndLogging:
    """Deadline enforcement and failure traceback logging for the PSD renderer."""

    def test_timeout_constant_matches_two_minute_contract(self) -> None:
        """PSD_RENDER_TIMEOUT_S is 120 seconds, matching the two-minute render deadline."""
        assert PSD_RENDER_TIMEOUT_S == 120

    def test_render_psd_image_times_out_and_cancels_when_deadline_passed(
        self,
        caplog: pytest.LogCaptureFixture,
    ) -> None:
        """render_psd_image cancels the future, warns, and returns None once the deadline passes."""
        test_path = Path("/fake/deadline_test.psd")
        mock_exec = MagicMock()
        mock_future = MagicMock()
        mock_future.done.return_value = False
        mock_future.result.side_effect = TimeoutError("poll")
        mock_exec.submit.return_value = mock_future

        with (
            patch("tarragon.renderers.psd.get_executor") as mock_get_exec,
            patch("tarragon.renderers.psd.time.monotonic", side_effect=[0.0, 121.0]),
            caplog.at_level(logging.WARNING, logger="tarragon.renderers.psd"),
        ):
            mock_get_exec.return_value = mock_exec
            result = render_psd_image(test_path, 20.0, 2, 2)

        assert result is None
        mock_future.cancel.assert_called_once()
        warning_messages = [r.message for r in caplog.records if r.name == "tarragon.renderers.psd"]
        assert any("timed out" in message and str(test_path) in message for message in warning_messages)

    def test_render_psd_image_returns_image_before_deadline(self, tmp_path: Path) -> None:
        """render_psd_image completes normally when the deadline has not elapsed."""
        dummy_img = Image.new("RGBA", (50, 50), (255, 0, 0, 128))
        buf = io.BytesIO()
        dummy_img.save(buf, "PNG")
        png_bytes = buf.getvalue()
        mock_exec = MagicMock()
        mock_future = MagicMock()
        mock_future.done.return_value = False
        mock_future.result.return_value = png_bytes
        mock_exec.submit.return_value = mock_future

        with (
            patch("tarragon.renderers.psd.get_executor") as mock_get_exec,
            patch("tarragon.renderers.psd.time.monotonic", side_effect=[0.0, 5.0]),
        ):
            mock_get_exec.return_value = mock_exec
            result = render_psd_image(tmp_path / "before_deadline.psd", 20.0, 2, 2)

        assert result is not None
        assert isinstance(result, Image.Image)
        assert result.size == (50, 50)
        mock_future.cancel.assert_not_called()

    def test_tile_composite_failure_logs_traceback(
        self,
        caplog: pytest.LogCaptureFixture,
    ) -> None:
        """_composite_psd_in_process warns with a traceback but completes when a tile fails."""
        mock_psd_cls = MagicMock()
        mock_psd_instance = MagicMock()
        mock_psd_instance.width = 5000
        mock_psd_instance.height = 5000
        mock_psd_instance.composite.side_effect = Exception("tile boom")
        mock_psd_cls.open.return_value = mock_psd_instance

        with caplog.at_level(logging.WARNING, logger="tarragon.renderers.psd"):
            with patch("psd_tools.PSDImage", mock_psd_cls):
                result = _composite_psd_in_process("/fake/failing_tiles.psd", 20.0, 2, 2)

        assert result is not None
        record = next(
            r for r in caplog.records if r.name == "tarragon.renderers.psd" and "Tile composite failed" in r.message
        )
        assert "/fake/failing_tiles.psd" in record.message
        assert "(0, 0)" in record.message
        assert record.exc_info is not None

    def test_worker_rendering_failure_logs_traceback(
        self,
        tmp_path: Path,
        caplog: pytest.LogCaptureFixture,
    ) -> None:
        """_composite_psd_in_process warns with a traceback when the worker rendering fails."""
        missing_path = tmp_path / "definitely_missing.psd"

        with caplog.at_level(logging.WARNING, logger="tarragon.renderers.psd"):
            result = _composite_psd_in_process(str(missing_path), 20.0, 2, 2)

        assert result is None
        record = next(
            r for r in caplog.records if r.name == "tarragon.renderers.psd" and "Rendering failed" in r.message
        )
        assert str(missing_path) in record.message
        assert record.exc_info is not None

    def test_render_psd_image_outer_failure_logs_traceback(
        self,
        caplog: pytest.LogCaptureFixture,
    ) -> None:
        """render_psd_image warns with a traceback and returns None when the poll loop fails."""
        test_path = Path("/fake/outer_failure.psd")
        mock_exec = MagicMock()
        mock_future = MagicMock()
        mock_future.done.return_value = False
        mock_future.result.side_effect = Exception("boom")
        mock_exec.submit.return_value = mock_future

        with caplog.at_level(logging.WARNING, logger="tarragon.renderers.psd"):
            with patch("tarragon.renderers.psd.get_executor") as mock_get_exec:
                mock_get_exec.return_value = mock_exec
                result = render_psd_image(test_path, 20.0, 2, 2)

        assert result is None
        record = next(
            r for r in caplog.records if r.name == "tarragon.renderers.psd" and "PSD render failed" in r.message
        )
        assert str(test_path) in record.message
        assert record.exc_info is not None
