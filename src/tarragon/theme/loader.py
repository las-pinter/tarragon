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

logger = logging.getLogger(__name__)


class ThemeLoader:
    """Loads theme QSS stylesheets and design tokens from package resources.

    Uses ``importlib.resources.files`` so the loader works whether the
    package is installed in editable mode or as a built distribution.
    """

    PACKAGE = "tarragon.theme"

    def load_qss(self) -> str:
        """Read the QSS stylesheet from *app.qss* inside the theme package.

        Returns:
            The raw QSS text content.

        Raises:
            FileNotFoundError: If ``app.qss`` is missing from the package.
        """
        files = importlib_resources.files(self.PACKAGE)
        qss_content = files.joinpath("app.qss").read_text(encoding="utf-8")
        logger.debug("Loaded QSS stylesheet (%d chars)", len(qss_content))
        return qss_content

    def load_tokens(self) -> dict[str, Any]:
        """Read design tokens from *tokens.json* inside the theme package.

        Returns:
            Parsed JSON dictionary.

        Raises:
            FileNotFoundError: If ``tokens.json`` is missing from the package.
        """
        files = importlib_resources.files(self.PACKAGE)
        return json.loads(files.joinpath("tokens.json").read_text(encoding="utf-8"))  # noqa: F405


__all__ = ["ThemeLoader"]
