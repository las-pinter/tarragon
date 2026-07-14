"""Theme loader — reads QSS from tokens and generates stylesheets."""

from __future__ import annotations

import logging
from typing import Any

from tarragon.theme.qss_generator import generate_qss
from tarragon.theme.tokens import load_tokens

logger = logging.getLogger(__name__)


class ThemeLoader:
    """Loads theme QSS stylesheets and design tokens from the tokens module."""

    def load_qss(self) -> str:
        """Generate the QSS stylesheet dynamically from design tokens.

        Loads tokens via :func:`~tarragon.theme.tokens.load_tokens` and
        passes them through :func:`~tarragon.theme.qss_generator.generate_qss`
        to produce a deterministic stylesheet.  Changing token values in the
        tokens module and re-calling this method updates the entire theme.

        Returns:
            The generated QSS text content.
        """
        tokens = self.load_tokens()
        qss_content = generate_qss(tokens)
        logger.debug("Generated QSS stylesheet (%d chars)", len(qss_content))
        return qss_content

    def load_tokens(self) -> dict[str, Any]:
        """Load design tokens from the tokens module.

        Returns:
            Dictionary of design tokens.
        """
        return load_tokens()


def load_and_generate_qss() -> str:
    """Load tokens and generate the QSS stylesheet in one call.

    Convenience function that creates a :class:`ThemeLoader`, reads
    tokens, and returns the generated QSS string.

    Returns:
        The generated QSS text ready for ``QApplication.setStyleSheet``.
    """
    loader = ThemeLoader()
    return loader.load_qss()


__all__ = ["ThemeLoader", "load_and_generate_qss"]
