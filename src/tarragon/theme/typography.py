"""Typography constants and QFont helpers derived from tokens.json.

Raw values (sizes, weights, family string) come from the ``typography``
section of *tokens.json*.  Helper functions return ready-to-use
:class:`QFont` instances for the most common text styles.

Example::

    from tarragon.theme.typography import body_font, heading_font
    label.setFont(body_font())
    header.setFont(heading_font())
"""

from __future__ import annotations

from PySide6.QtGui import QFont

from tarragon.theme.tokens import load_tokens

# ── Load tokens once at import time ──────────────────────────────────────────
_typo: dict[str, int | str] = load_tokens()["typography"]

# ── Raw token values ─────────────────────────────────────────────────────────

#: Comma-separated font family fallback chain (e.g. ``"Segoe UI, …"``).
FONT_FAMILY: str = str(_typo["font_family"])

#: Body text size in points.
BODY_SIZE: int = int(_typo["body_size"])

#: Heading text size in points.
HEADING_SIZE: int = int(_typo["heading_size"])

#: Small / caption text size in points.
SMALL_SIZE: int = int(_typo["small_size"])

#: Regular font weight (400).
WEIGHT_REGULAR: int = int(_typo["weight_regular"])

#: Medium font weight (500).
WEIGHT_MEDIUM: int = int(_typo["weight_medium"])

#: Semi-bold font weight (600).
WEIGHT_SEMIBOLD: int = int(_typo["weight_semibold"])


# ── QFont helper functions ───────────────────────────────────────────────────


def _make_font(size: int, weight: int) -> QFont:
    """Create a QFont from the design-system family, size, and weight.

    Args:
        size: Font size in points.
        weight: Font weight as an integer (e.g. 400, 500, 600).

    Returns:
        A configured :class:`QFont` instance.
    """
    font = QFont(FONT_FAMILY)
    font.setPointSize(size)
    font.setWeight(QFont.Weight(weight))
    return font


def body_font(*, weight: int | None = None) -> QFont:
    """Return a QFont sized for body text.

    Args:
        weight: Optional weight override.  Defaults to ``WEIGHT_REGULAR``.

    Returns:
        A :class:`QFont` at ``BODY_SIZE`` with the given weight.
    """
    return _make_font(BODY_SIZE, weight if weight is not None else WEIGHT_REGULAR)


def heading_font(*, weight: int | None = None) -> QFont:
    """Return a QFont sized for headings.

    Args:
        weight: Optional weight override.  Defaults to ``WEIGHT_SEMIBOLD``.

    Returns:
        A :class:`QFont` at ``HEADING_SIZE`` with the given weight.
    """
    return _make_font(HEADING_SIZE, weight if weight is not None else WEIGHT_SEMIBOLD)


def small_font(*, weight: int | None = None) -> QFont:
    """Return a QFont sized for captions and small text.

    Args:
        weight: Optional weight override.  Defaults to ``WEIGHT_REGULAR``.

    Returns:
        A :class:`QFont` at ``SMALL_SIZE`` with the given weight.
    """
    return _make_font(SMALL_SIZE, weight if weight is not None else WEIGHT_REGULAR)


__all__ = [
    "BODY_SIZE",
    "FONT_FAMILY",
    "HEADING_SIZE",
    "SMALL_SIZE",
    "WEIGHT_MEDIUM",
    "WEIGHT_REGULAR",
    "WEIGHT_SEMIBOLD",
    "body_font",
    "heading_font",
    "small_font",
]
