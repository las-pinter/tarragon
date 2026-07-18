"""Database schema initialization and CRUD repository for Tarragon's SQLite store.

This package provides the :class:`Database` class and the :func:`normalize_path`
helper.  All other names are internal implementation details.
"""

from __future__ import annotations

from tarragon.db._base import normalize_path
from tarragon.db.database import Database

__all__ = ["Database", "normalize_path"]
