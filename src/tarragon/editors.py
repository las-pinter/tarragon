"""Backward-compatibility shim — moved to tarragon.services.editors.

All public names are re-exported from the canonical location.
"""

from tarragon.services.editors import (  # noqa: F401
    launch_editor,
    resolve_editor_command,
    substitute_file_path,
)

__all__ = ["launch_editor", "resolve_editor_command", "substitute_file_path"]
