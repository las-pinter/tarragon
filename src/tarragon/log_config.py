"""Logging settings and formatting"""

from __future__ import annotations

import logging
from logging.handlers import RotatingFileHandler
from pathlib import Path

# Rotation tunables: each log file grows up to MAX_LOG_BYTES before rolling,
# keeping at most LOG_BACKUP_COUNT rotated backups on disk.
# Module constants by design, NOT a SettingsService entry: rotation is an
# operational detail, not user-facing — change these values here.
LOG_BACKUP_COUNT = 2
MAX_LOG_BYTES = 10 * 1024 * 1024

# Immutable format templates. LogFormatter picks one per record instead of
# mutating shared style state, so concurrent formatting is thread-safe.
_DEBUG_FORMAT = "%(asctime)s [%(levelname)s] %(funcName)s(): %(message)s"
_STANDARD_FORMAT = "%(asctime)s [%(levelname)s] %(message)s"


class LogFormatter(logging.Formatter):
    """Formatter that adds the calling function name to DEBUG records.

    Thread-safe: builds one immutable formatter per output shape at
    construction and delegates per record. ``format`` never mutates state.
    """

    def __init__(self) -> None:
        super().__init__(fmt=_STANDARD_FORMAT)
        self._debug_formatter = logging.Formatter(fmt=_DEBUG_FORMAT)

    def format(self, record: logging.LogRecord) -> str:
        if record.levelno == logging.DEBUG:
            return self._debug_formatter.format(record)
        return super().format(record)


def close_root_handlers() -> None:
    """Close every handler attached to the root logger.

    Called at application shutdown so log streams are flushed and released
    (file handles, Qt log panel connections).
    """
    for handler in logging.getLogger().handlers:
        handler.close()


def setup_file_logging(log_path: Path) -> logging.Handler:
    """Configure and return a rotating file handler for the given log path.

    The handler writes UTF-8 encoded records formatted with ``LogFormatter``.
    On startup, an existing log file is rotated once so the previous session
    becomes ``.1`` and older backups shift up, keeping at most
    ``LOG_BACKUP_COUNT`` rotated files on disk.

    Args:
        log_path: Full path to the log file to write.

    Returns:
        A configured ``RotatingFileHandler`` ready to be attached to a logger.
    """
    log_path.parent.mkdir(parents=True, exist_ok=True)
    # Check existence before constructing: FileHandler opens (and creates) the file.
    needs_rollover = log_path.exists()
    handler = RotatingFileHandler(
        log_path,
        maxBytes=MAX_LOG_BYTES,
        backupCount=LOG_BACKUP_COUNT,
        encoding="utf-8",
    )
    handler.setFormatter(LogFormatter())
    if needs_rollover:
        handler.doRollover()
    return handler
