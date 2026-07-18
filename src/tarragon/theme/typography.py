"""Typography constants derived from design tokens.

Raw values (sizes) come from the ``typography`` section of *tokens.json*.
Only the constants with production consumers are exported.

Example::

    from tarragon.theme.typography import SMALL_SIZE, LOG_SIZE
    label.setFontSize(SMALL_SIZE)
    log.setFontSize(LOG_SIZE)
"""

from __future__ import annotations

from tarragon.theme.tokens import load_tokens

# ── Load tokens once at import time ──────────────────────────────────────────
_typo: dict[str, int | str] = load_tokens()["typography"]

# ── Raw token values ─────────────────────────────────────────────────────────

#: Small / caption text size in points.
SMALL_SIZE: int = int(_typo["small_size"])

#: Log / monospace text size in points.
LOG_SIZE: int = int(_typo["log_size"])

__all__ = ["LOG_SIZE", "SMALL_SIZE"]
