"""Animation controller for thumbnail hover-scale and fade-in effects."""

from __future__ import annotations

from typing import Any

from PySide6.QtCore import QEasingCurve, QTime, QTimer
from PySide6.QtWidgets import QListView

from tarragon.theme.constants import DURATION_FAST, DURATION_NORMAL

# Animation tokens (from tokens.json motion values)
HOVER_SCALE_TARGET = 1.02
# Extra pixels per cell to accommodate hover-scale growth without overlapping neighbors
HOVER_MARGIN = 4
HOVER_DURATION_MS = DURATION_FAST
FADE_DURATION_MS = DURATION_NORMAL
ANIMATOR_INTERVAL_MS = 16  # ~60fps

# Easing curve mapped from the EASING token ("ease-out" → OutQuad).
_EASING_CURVE = QEasingCurve(QEasingCurve.Type.OutQuad)


class ThumbnailAnimator:
    """Drives hover-scale and fade-in animations for thumbnail grid cells.

    Since QStyledItemDelegate paints cells immediately (no per-cell QObjects),
    this controller maintains animation state per row and drives repaints via
    a single shared QTimer at ~60fps. The timer auto-stops when no animations
    are active to avoid unnecessary CPU usage.
    """

    def __init__(self, view: QListView) -> None:
        self._view = view
        self._timer = QTimer()
        self._timer.setInterval(ANIMATOR_INTERVAL_MS)
        self._timer.timeout.connect(self._tick)

        # Hover scale animations: row -> animation state dict
        # State: {"current": float, "target": float, "start_val": float, "start_time": QTime}
        self._hover_anims: dict[int, dict[str, Any]] = {}

        # Fade-in animations: row -> animation state dict
        # State: {"current": float, "start_time": QTime}
        self._fade_anims: dict[int, dict[str, Any]] = {}

        # Rows that have completed their fade-in (no animation needed)
        self._faded_in: set[int] = set()

    def start_hover(self, row: int, prev_row: int) -> None:
        """Begin scale-up animation for *row* and scale-down for *prev_row*."""
        now = QTime.currentTime()

        if row >= 0:
            existing = self._hover_anims.get(row)
            current_val = existing["current"] if existing else 1.0
            self._hover_anims[row] = {
                "current": current_val,
                "target": HOVER_SCALE_TARGET,
                "start_val": current_val,
                "start_time": now,
            }

        if prev_row >= 0 and prev_row != row:
            existing = self._hover_anims.get(prev_row)
            current_val = existing["current"] if existing else HOVER_SCALE_TARGET
            self._hover_anims[prev_row] = {
                "current": current_val,
                "target": 1.0,
                "start_val": current_val,
                "start_time": now,
            }

        self._ensure_running()

    def notify_rows_about_to_reset(self) -> None:
        """Clear all fade-in tracking when the model is about to reset."""
        self._fade_anims.clear()
        self._faded_in.clear()

    def notify_rows_inserted(self, first_row: int, last_row: int) -> None:
        """Start fade-in animation for newly inserted rows."""
        now = QTime.currentTime()
        for row in range(first_row, last_row + 1):
            if row not in self._faded_in:
                self._fade_anims[row] = {
                    "current": 0.0,
                    "start_time": now,
                }
        if first_row <= last_row:
            self._ensure_running()

    def get_scale(self, row: int) -> float:
        """Return the current animated scale factor for *row*."""
        anim = self._hover_anims.get(row)
        if anim is not None:
            return float(anim["current"])
        return 1.0

    def get_opacity(self, row: int) -> float:
        """Return the current animated opacity for *row*."""
        anim = self._fade_anims.get(row)
        if anim is not None:
            return float(anim["current"])
        return 1.0

    def is_animating(self) -> bool:
        """Return True if any animations are currently active."""
        return bool(self._hover_anims or self._fade_anims)

    def _ensure_running(self) -> None:
        """Start the timer if it isn't already running."""
        if not self._timer.isActive():
            self._timer.start()

    def _tick(self) -> None:
        """Advance all active animations by one frame and trigger a repaint."""
        now = QTime.currentTime()
        has_active = False

        # Advance hover scale animations
        finished_hover: list[int] = []
        for row, anim in self._hover_anims.items():
            elapsed = anim["start_time"].msecsTo(now)
            progress = min(elapsed / HOVER_DURATION_MS, 1.0)
            eased = _EASING_CURVE.valueForProgress(progress)
            anim["current"] = anim["start_val"] + (anim["target"] - anim["start_val"]) * eased

            if progress >= 1.0:
                anim["current"] = anim["target"]
                if anim["target"] == 1.0:
                    # Fully scaled down — remove from tracking
                    finished_hover.append(row)
                # If target is HOVER_SCALE_TARGET, keep tracking so we can
                # animate back down when hover ends
            else:
                has_active = True

        for row in finished_hover:
            del self._hover_anims[row]

        # Advance fade-in animations
        finished_fade: list[int] = []
        for row, anim in self._fade_anims.items():
            elapsed = anim["start_time"].msecsTo(now)
            progress = min(elapsed / FADE_DURATION_MS, 1.0)
            eased = _EASING_CURVE.valueForProgress(progress)
            anim["current"] = eased

            if progress >= 1.0:
                anim["current"] = 1.0
                self._faded_in.add(row)
                finished_fade.append(row)
            else:
                has_active = True

        for row in finished_fade:
            del self._fade_anims[row]

        # Trigger repaint of the viewport
        self._view.viewport().update()

        # Stop the timer if nothing is animating
        if not has_active:
            self._timer.stop()

    def shutdown(self) -> None:
        """Stop the timer and clear all state."""
        self._timer.stop()
        self._hover_anims.clear()
        self._fade_anims.clear()
        self._faded_in.clear()
