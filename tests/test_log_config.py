"""Tests for file-based logging"""

from __future__ import annotations

import logging
from logging.handlers import RotatingFileHandler
from pathlib import Path
from unittest.mock import patch

from tarragon.app_paths import db_path
from tarragon.log_config import LogFormatter, setup_file_logging


class TestSetupFileLogging:
    """The setup_file_logging() function configures a rotating file handler."""

    def test_handler_writes_records_to_log_path_next_to_db(self, tmp_path: Path) -> None:
        """The file handler writes records to the log file next to the database."""
        with patch("tarragon.app_paths.platformdirs.user_data_dir", return_value=str(tmp_path)):
            log_path = db_path().parent / "tarragon.log"
        assert log_path.parent == tmp_path

        handler = setup_file_logging(log_path)
        logger = logging.getLogger("test.logging.file")
        logger.addHandler(handler)
        try:
            logger.setLevel(logging.INFO)
            logger.info("hello file")
            handler.flush()
            assert "hello file" in log_path.read_text(encoding="utf-8")
        finally:
            logger.removeHandler(handler)
            handler.close()

    def test_startup_rotation_renames_existing_log(self, tmp_path: Path) -> None:
        """An existing log file is rotated to .1 when setup runs again."""
        log_path = tmp_path / "tarragon.log"
        log_path.write_text("previous session\n", encoding="utf-8")

        handler = setup_file_logging(log_path)
        try:
            assert (tmp_path / "tarragon.log.1").read_text(encoding="utf-8") == "previous session\n"
            assert log_path.exists()
        finally:
            handler.close()

    def test_rotation_keeps_only_three_files(self, tmp_path: Path) -> None:
        """Startup rotation removes the oldest backup so only three files remain."""
        log_path = tmp_path / "tarragon.log"
        log_path.write_text("current\n", encoding="utf-8")
        (tmp_path / "tarragon.log.1").write_text("previous\n", encoding="utf-8")
        (tmp_path / "tarragon.log.2").write_text("oldest\n", encoding="utf-8")

        handler = setup_file_logging(log_path)
        try:
            rotated = sorted(tmp_path.glob("tarragon.log*"))
            assert [p.name for p in rotated] == ["tarragon.log", "tarragon.log.1", "tarragon.log.2"]
            assert (tmp_path / "tarragon.log.1").read_text(encoding="utf-8") == "current\n"
            assert (tmp_path / "tarragon.log.2").read_text(encoding="utf-8") == "previous\n"
        finally:
            handler.close()

    def test_first_run_without_existing_log_does_not_crash(self, tmp_path: Path) -> None:
        """Setup succeeds when no log file exists yet and creates no backups."""
        log_path = tmp_path / "tarragon.log"
        handler = setup_file_logging(log_path)
        logger = logging.getLogger("test.logging.first_run")
        logger.addHandler(handler)
        try:
            assert not (tmp_path / "tarragon.log.1").exists()
            logger.setLevel(logging.INFO)
            logger.info("first run")
            handler.flush()
            assert "first run" in log_path.read_text(encoding="utf-8")
        finally:
            logger.removeHandler(handler)
            handler.close()

    def test_handler_uses_log_formatter_and_utf8_encoding(self, tmp_path: Path) -> None:
        """The file handler is configured with LogFormatter and UTF-8 encoding."""
        log_path = tmp_path / "tarragon.log"
        handler = setup_file_logging(log_path)
        try:
            assert isinstance(handler.formatter, LogFormatter)
            assert isinstance(handler, RotatingFileHandler)
            assert handler.encoding == "utf-8"
        finally:
            handler.close()

    def test_handler_writes_utf8_encoded_records(self, tmp_path: Path) -> None:
        """The file handler writes non-ASCII characters as UTF-8."""
        log_path = tmp_path / "tarragon.log"
        handler = setup_file_logging(log_path)
        logger = logging.getLogger("test.logging.utf8")
        logger.addHandler(handler)
        try:
            logger.setLevel(logging.INFO)
            logger.info("caf\u00e9")
            handler.flush()
            assert "caf\u00e9" in log_path.read_text(encoding="utf-8")
        finally:
            logger.removeHandler(handler)
            handler.close()

    def test_handler_respects_root_logger_level(self, tmp_path: Path) -> None:
        """The file handler emits DEBUG records only when the root logger is at DEBUG."""
        log_path = tmp_path / "tarragon.log"
        handler = setup_file_logging(log_path)
        root = logging.getLogger()
        previous_level = root.level
        root.addHandler(handler)
        try:
            root.setLevel(logging.DEBUG)
            logging.debug("debug message")
            handler.flush()
            assert "debug message" in log_path.read_text(encoding="utf-8")

            root.setLevel(logging.INFO)
            logging.debug("suppressed message")
            handler.flush()
            assert "suppressed message" not in log_path.read_text(encoding="utf-8")
        finally:
            root.removeHandler(handler)
            root.setLevel(previous_level)
            handler.close()

    def test_setup_creates_missing_parent_directory(self, tmp_path: Path) -> None:
        """Setup creates the log directory when it does not exist yet."""
        log_path = tmp_path / "nested" / "logs" / "tarragon.log"
        handler = setup_file_logging(log_path)
        try:
            assert log_path.parent.is_dir()
        finally:
            handler.close()
