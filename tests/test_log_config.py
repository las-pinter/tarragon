"""Tests for file-based logging"""

from __future__ import annotations

import logging
import sys
import threading
from logging.handlers import RotatingFileHandler
from pathlib import Path
from unittest.mock import MagicMock, patch

from tarragon.app_paths import db_path
from tarragon.log_config import LogFormatter, close_root_handlers, setup_file_logging


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


def _make_record(level: int, message: str) -> logging.LogRecord:
    """Build a LogRecord with a known funcName so templates are distinguishable."""
    return logging.LogRecord(
        name="test.logging",
        level=level,
        pathname=__file__,
        lineno=1,
        msg=message,
        args=(),
        exc_info=None,
        func="run_worker",
    )


class TestLogFormatter:
    """LogFormatter renders the function name only for DEBUG records."""

    def test_debug_record_includes_func_name(self) -> None:
        """A DEBUG record is formatted with the function name and parentheses."""
        formatted = LogFormatter().format(_make_record(logging.DEBUG, "hello"))
        assert "run_worker()" in formatted

    def test_warning_record_omits_func_name(self) -> None:
        """A WARNING record is formatted without the function name."""
        formatted = LogFormatter().format(_make_record(logging.WARNING, "hello"))
        assert "run_worker()" not in formatted
        assert "hello" in formatted

    def test_info_and_error_records_omit_func_name(self) -> None:
        """INFO and ERROR records are formatted without the function name."""
        for level in (logging.INFO, logging.ERROR):
            formatted = LogFormatter().format(_make_record(level, "hello"))
            assert "run_worker()" not in formatted, f"{logging.getLevelName(level)} had funcName"

    def test_output_matches_previous_templates(self) -> None:
        """Formatted output is byte-identical to the pre-fix templates."""
        formatter = LogFormatter()
        templates = (
            (logging.DEBUG, "%(asctime)s [%(levelname)s] %(funcName)s(): %(message)s"),
            (logging.INFO, "%(asctime)s [%(levelname)s] %(message)s"),
        )
        for level, template in templates:
            reference = logging.Formatter(template)
            record = _make_record(level, "hello")
            assert formatter.format(record) == reference.format(record)

    def test_concurrent_formatting_is_thread_safe(self) -> None:
        """Concurrent formatting never mixes templates across levels.

        32 threads each format ~200 mixed-level records through ONE shared
        LogFormatter (exactly what the three root handlers do in production).
        Every output must match its level's shape — funcName present iff
        DEBUG. On the pre-fix code, the shared ``_style._fmt`` mutation
        interleaves across threads and violates that invariant.
        """
        formatter = LogFormatter()
        levels = [logging.DEBUG, logging.INFO, logging.WARNING, logging.ERROR]
        n_threads = 32
        rounds = 200
        barrier = threading.Barrier(n_threads)
        violations: list[str] = []
        violations_lock = threading.Lock()

        # Tighten GIL preemption so the thread storm genuinely interleaves
        # format() calls. The pre-fix code mutates shared _style._fmt between
        # the write and its read, so interleaving surfaces template mismatches.
        previous_switch_interval = sys.getswitchinterval()
        sys.setswitchinterval(1e-5)
        try:

            def worker(seed: int) -> None:
                barrier.wait()
                for i in range(rounds):
                    level = levels[(seed + i) % len(levels)]
                    formatted = formatter.format(_make_record(level, f"msg {seed}-{i}"))
                    if level == logging.DEBUG:
                        if "run_worker()" not in formatted:
                            with violations_lock:
                                violations.append(f"DEBUG missing funcName: {formatted!r}")
                    elif "run_worker()" in formatted:
                        with violations_lock:
                            violations.append(f"{logging.getLevelName(level)} has funcName: {formatted!r}")

            threads = [threading.Thread(target=worker, args=(seed,)) for seed in range(n_threads)]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
        finally:
            sys.setswitchinterval(previous_switch_interval)

        assert violations == [], f"{len(violations)} template mismatches: {violations[:3]}"


class TestCloseRootHandlers:
    """close_root_handlers() closes every handler attached to the root logger."""

    def test_closes_every_attached_handler(self) -> None:
        """Each handler on the root logger is closed exactly once."""
        attached = [MagicMock(), MagicMock()]
        fake_root = MagicMock()
        fake_root.handlers = attached
        with patch.object(logging, "getLogger", return_value=fake_root):
            close_root_handlers()
        for handler in attached:
            handler.close.assert_called_once_with()
