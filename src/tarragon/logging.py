"""Logging settings and formatting"""

from __future__ import annotations

import logging


class LogFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        if record.levelno == logging.DEBUG:
            self._style._fmt = "%(asctime)s [%(levelname)s] %(funcName)s(): %(message)s"
        else:
            self._style._fmt = "%(asctime)s [%(levelname)s] %(message)s"
        return super().format(record)
