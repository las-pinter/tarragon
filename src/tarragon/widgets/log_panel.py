"""LogPanel and QtLogHandler — dockable log viewer with color-coded levels.

LogPanel displays application log messages in a read-only scrollable text area
with color coding per severity.  QtLogHandler bridges Python's ``logging``
module to the panel via Qt signals, ensuring thread-safe delivery from any
worker thread.

Design patterns:
    - Observer pattern  — ``_log_signal`` for cross-thread message delivery
    - Adapter pattern   — ``QtLogHandler`` adapts ``logging.Handler`` to Qt
"""

from __future__ import annotations

import logging

from PySide6.QtCore import Signal
from PySide6.QtGui import QFont
from PySide6.QtWidgets import (
    QCheckBox,
    QHBoxLayout,
    QPlainTextEdit,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from tarragon.theme.colors import (
    AMBER_ACCENT,
    CORAL_MUTED,
    CORAL_STRONG,
    TEXT_PRIMARY,
    TEXT_TERTIARY,
)
from tarragon.theme.typography import LOG_SIZE

# Color mapping: log level → display color (sourced from theme tokens)
_LEVEL_COLORS: dict[int, str] = {
    logging.DEBUG: TEXT_TERTIARY.name(),
    logging.INFO: TEXT_PRIMARY.name(),
    logging.WARNING: AMBER_ACCENT.name(),
    logging.ERROR: CORAL_STRONG.name(),
    logging.CRITICAL: CORAL_MUTED.name(),
}

_DEFAULT_COLOR = TEXT_PRIMARY.name()


def apply_debug_level(enabled: bool) -> None:
    """Set root logger level to DEBUG or INFO based on enabled flag.

    Args:
        enabled: If ``True``, set root logger to ``DEBUG``; otherwise ``INFO``.
    """
    logging.getLogger().setLevel(logging.DEBUG if enabled else logging.INFO)


class LogPanel(QWidget):
    """Panel that displays application log messages.

    Messages are appended through a Qt signal (``_log_signal``) so that
    logging from background threads is delivered safely to the GUI thread.
    """

    _log_signal = Signal(str, int)  # (formatted_message, log_level)

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)

        # ── Layout ──────────────────────────────────────────────────────
        layout = QVBoxLayout(self)
        layout.setContentsMargins(4, 4, 4, 4)
        layout.setSpacing(4)

        # ── Toolbar row ─────────────────────────────────────────────────
        toolbar = QHBoxLayout()
        toolbar.setContentsMargins(0, 0, 0, 0)

        self._clear_btn = QPushButton("Clear")
        self._clear_btn.clicked.connect(self.clear_logs)
        toolbar.addWidget(self._clear_btn)

        toolbar.addStretch()

        self._debug_checkbox = QCheckBox("Debug")
        self._debug_checkbox.setChecked(False)
        self._debug_checkbox.toggled.connect(self.set_debug_enabled)
        toolbar.addWidget(self._debug_checkbox)

        layout.addLayout(toolbar)

        # ── Log text area ───────────────────────────────────────────────
        self._text_area = QPlainTextEdit()
        self._text_area.setObjectName("logText")
        self._text_area.setReadOnly(True)
        self._text_area.setMaximumBlockCount(5000)

        font = QFont()
        font.setFamilies(["Consolas", "Courier New", "monospace"])
        font.setPointSize(LOG_SIZE)
        self._text_area.setFont(font)

        layout.addWidget(self._text_area)

        # ── Signal connection ───────────────────────────────────────────
        self._log_signal.connect(self.append_log)

    # ── Public slots / methods ──────────────────────────────────────────

    def append_log(self, message: str, level: int) -> None:
        """Append a color-coded log line to the text area.

        Called automatically via ``_log_signal`` — do **not** call directly
        from background threads.
        """
        color = _LEVEL_COLORS.get(level, _DEFAULT_COLOR)
        self._text_area.appendHtml(
            f'<span style="color:{color};">{message}</span>'
        )

    def clear_logs(self) -> None:
        """Remove all log lines from the panel."""
        self._text_area.clear()

    def set_debug_enabled(self, enabled: bool) -> None:
        """Toggle the root logger between DEBUG and INFO levels."""
        apply_debug_level(enabled)


class QtLogHandler(logging.Handler):
    """Logging handler that routes messages to a LogPanel via Qt signals.

    Attach this handler to the root logger to have all Python log records
    appear in the on-screen log panel::

        handler = QtLogHandler(log_panel)
        handler.setFormatter(logging.Formatter(
            "[%(asctime)s] %(levelname)-8s %(name)s: %(message)s",
            datefmt="%H:%M:%S",
        ))
        logging.getLogger().addHandler(handler)
    """

    def __init__(self, log_panel: LogPanel) -> None:
        super().__init__()
        self._log_panel = log_panel
        self.setFormatter(
            logging.Formatter(
                "[%(asctime)s] %(levelname)-8s %(name)s: %(message)s",
                datefmt="%H:%M:%S",
            )
        )

    def emit(self, record: logging.LogRecord) -> None:
        """Format *record* and deliver it to the panel thread-safely."""
        # Filter out noisy third-party debug messages
        if record.name.startswith("PIL") and record.levelno < logging.WARNING:
            return
        try:
            msg = self.format(record)
            self._log_panel._log_signal.emit(msg, record.levelno)
        except Exception:
            self.handleError(record)
