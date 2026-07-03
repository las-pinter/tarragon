"""Spacing constants derived from tokens.json.

All values are in **pixels** and correspond to the ``spacing`` section of
*tokens.json*.  Use these instead of hard-coding pixel values in layouts,
margins, and paddings.

Example::

    from tarragon.theme.spacing import SM, MD
    layout.setContentsMargins(SM, MD, SM, MD)
"""

from __future__ import annotations

from tarragon.theme.tokens import load_tokens

_spacing: dict[str, int] = load_tokens()["spacing"]

#: Extra-small spacing — 4 px.  Tight gaps between related items.
XS: int = _spacing["xs"]

#: Small spacing — 8 px.  Default inner padding and icon-to-text gaps.
SM: int = _spacing["sm"]

#: Medium spacing — 12 px.  Standard group separation.
MD: int = _spacing["md"]

#: Large spacing — 16 px.  Section padding and major element gaps.
LG: int = _spacing["lg"]

#: Extra-large spacing — 24 px.  Page-level margins and major section breaks.
XL: int = _spacing["xl"]

__all__ = ["LG", "MD", "SM", "XL", "XS"]
