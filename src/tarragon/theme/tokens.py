"""Design tokens for Tarragon's dark coral-amber aesthetic."""

from __future__ import annotations

import json
from typing import Any

try:
    from importlib.resources import files  # Python 3.9+
except ImportError:
    from importlib_resources import files  # type: ignore[no-redef]


def _tokens_path():
    """Return the resource path to tokens.json."""
    return files("tarragon.theme") / "tokens.json"


def load_tokens() -> dict[str, Any]:
    """Load and return the design tokens from tokens.json.

    Returns:
        Dictionary with keys: colors, typography, spacing, radius, motion, layout.
    """
    content = _tokens_path().read_text(encoding="utf-8")
    return json.loads(content)


def get_token(section: str, key: str) -> Any:
    """Return a single token value by section and key.

    Args:
        section: Top-level token group (e.g. 'colors', 'typography').
        key: The specific token name within that section.

    Returns:
        The token value.

    Raises:
        KeyError: If the section or key does not exist in tokens.json.
    """
    return load_tokens()[section][key]


__all__ = ["load_tokens", "get_token"]
