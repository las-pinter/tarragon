"""Natural (human-friendly) sorting of gallery paths.

Provides a pure-stdlib, case-insensitive natural sort key so that
"name 2" sorts before "name 10". Sorting happens at the Python level
(not in SQL) so that future sort modes (e.g. by date) can be added
without touching the database layer.
"""

from __future__ import annotations

import re
from collections.abc import Iterable
from enum import Enum
from pathlib import Path

# Pad every digit run to this many characters so that "2" sorts before
# "10" regardless of how many digits a run contains. 20 comfortably
# exceeds any realistic image sequence number.
_DIGIT_PAD_WIDTH = 20

_DIGIT_RUN = re.compile(r"\d+")


class SortMode(Enum):
    """Gallery sort modes. Member value is the persisted setting value."""

    NAME = "name"


def natural_key(path: Path) -> tuple[str, str]:
    """Return a sort key that orders *path* in natural, case-insensitive order.

    The key is a ``(padded, raw)`` tuple. *padded* is the casefolded path
    string with every digit run zero-padded to a fixed width, giving a total
    order where numeric runs compare by magnitude. *raw* is the original
    string, used as a deterministic tiebreak so that e.g. "IMG 1" sorts
    before "img 1" and "img 001" before "img 01" before "img 1".

    Parameters
    ----------
    path : Path
        Path to build a sort key for.

    Returns
    -------
    tuple[str, str]
        ``(padded, raw)`` sort key.
    """
    raw = str(path)
    casefolded = raw.casefold()
    padded = _DIGIT_RUN.sub(lambda m: m.group(0).zfill(_DIGIT_PAD_WIDTH), casefolded)
    return (padded, raw)


def sort_paths(paths: Iterable[Path], mode: SortMode = SortMode.NAME) -> list[Path]:
    """Sort *paths* into natural order according to *mode*.

    The input iterable is not mutated; a new list is returned.

    Parameters
    ----------
    paths : Iterable[Path]
        Paths to sort.
    mode : SortMode
        Sort mode to apply. Only ``SortMode.NAME`` is currently supported.

    Returns
    -------
    list[Path]
        Paths sorted into natural order.
    """
    if mode is SortMode.NAME:
        return sorted(paths, key=natural_key)
    raise ValueError(f"Unsupported sort mode: {mode!r}")
