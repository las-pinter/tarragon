"""External editor launching with configurable command templates."""

from __future__ import annotations

import os
import shlex
import subprocess
import sys
from pathlib import Path

from tarragon.db.database import Database


def resolve_editor_command(db: Database, extension: str) -> str | None:
    """Look up editor command template for extension.

    First checks specific extension (e.g., ".psd"), then falls back to
    wildcard "*" if no specific match found.
    """
    cmd = db.get_editor_command(extension)
    if cmd is not None:
        return cmd
    return db.get_editor_command("*")


def substitute_file_path(template: str, file_path: Path) -> list[str]:
    """Safely substitute {file} placeholder and split into shell-args list.

    Raises ValueError if template doesn't contain {file} placeholder.
    """
    if "{file}" not in template:
        raise ValueError(f"Template must contain '{{file}}' placeholder: {template!r}")

    substituted = template.replace("{file}", shlex.quote(str(file_path)))
    return shlex.split(substituted)


def launch_editor(db: Database, file_path: Path, extension: str) -> None:
    """Look up command template, substitute file path, launch via subprocess.Popen.

    If no association found, falls back to OS default handler.
    Non-blocking — uses Popen.
    """
    cmd_template = resolve_editor_command(db, extension)

    if cmd_template is not None:
        args = substitute_file_path(cmd_template, file_path)
        subprocess.Popen(args, shell=False)
    else:
        # OS default handler
        if sys.platform == "win32":
            os.startfile(str(file_path))  # type: ignore[attr-defined]
        else:
            subprocess.Popen(["xdg-open", str(file_path)])
