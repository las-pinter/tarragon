"""Theme loader — reads QSS and tokens from package resources."""

from __future__ import annotations

import json
import logging
from typing import Any

try:
    # Python 3.9+ preferred API for reading package files
    from importlib import resources as importlib_resources
except ImportError:  # pragma: no cover
    import importlib_resources  # type: ignore[no-redef]

from tarragon.theme.qss_generator import generate_qss

logger = logging.getLogger(__name__)


class ThemeLoader:
    """Loads theme QSS stylesheets and design tokens from package resources.

    Uses ``importlib.resources.files`` so the loader works whether the
    package is installed in editable mode or as a built distribution.
    """

    PACKAGE = "tarragon.theme"

    def load_qss(self) -> str:
        """Generate the QSS stylesheet dynamically from design tokens.

        Loads ``tokens.json`` and passes the tokens through
        :func:`~tarragon.theme.qss_generator.generate_qss` to produce
        a deterministic stylesheet.  Changing token values in ``tokens.json``
        and re-calling this method updates the entire theme.

        Returns:
            The generated QSS text content.
        """
        tokens = self.load_tokens()
        qss_content = generate_qss(tokens)
        logger.debug("Generated QSS stylesheet (%d chars)", len(qss_content))
        return qss_content

    def load_tokens(self) -> dict[str, Any]:
        """Read design tokens from *tokens.json* inside the theme package.

        Returns:
            Parsed JSON dictionary.

        Raises:
            FileNotFoundError: If ``tokens.json`` is missing from the package.
        """
        files = importlib_resources.files(self.PACKAGE)
        return dict(json.loads(files.joinpath("tokens.json").read_text(encoding="utf-8")))  # noqa: F405


def load_and_generate_qss() -> str:
    """Load tokens and generate the QSS stylesheet in one call.

    Convenience function that creates a :class:`ThemeLoader`, reads
    ``tokens.json``, and returns the generated QSS string.

    Returns:
        The generated QSS text ready for ``QApplication.setStyleSheet``.
    """
    loader = ThemeLoader()
    return loader.load_qss()


__all__ = ["ThemeLoader", "load_and_generate_qss"]
