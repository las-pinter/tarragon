"""Motion constants derived from tokens.json.

All duration values are in **milliseconds** and easing values are CSS
timing-function names.  These correspond to the ``motion`` section of
*tokens.json*.  Use these instead of hard-coding animation durations
or easing curves in transitions and animations.

Example::

    from tarragon.theme.motion import DURATION_FAST, EASING
    animation.setDuration(DURATION_FAST)
    animation.setEasingCurve(QEasingCurve(EASING))
"""

from __future__ import annotations

from tarragon.theme.tokens import load_tokens

_motion: dict[str, int | str] = load_tokens()["motion"]

#: Fast duration — 150 ms.  Micro-interactions: button press, checkbox toggle.
DURATION_FAST: int = _motion["duration_fast"]  # type: ignore[assignment]

#: Normal duration — 200 ms.  Standard transitions: panel slide, fade-in.
DURATION_NORMAL: int = _motion["duration_normal"]  # type: ignore[assignment]

#: Default easing curve — "ease-out".  Decelerating motion for entrances.
EASING: str = _motion["easing"]  # type: ignore[assignment]

__all__ = ["DURATION_FAST", "DURATION_NORMAL", "EASING"]
