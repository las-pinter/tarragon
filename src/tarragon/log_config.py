"""Logging settings and formatting"""

from __future__ import annotations

import logging
from logging.handlers import RotatingFileHandler
from pathlib import Path

LOG_BACKUP_COUNT = 2
MAX_LOG_BYTES = 10 * 1024 * 1024


class LogFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        if record.levelno == logging.DEBUG:
            self._style._fmt = "%(asctime)s [%(levelname)s] %(funcName)s(): %(message)s"
        else:
            self._style._fmt = "%(asctime)s [%(levelname)s] %(message)s"
        return super().format(record)


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
